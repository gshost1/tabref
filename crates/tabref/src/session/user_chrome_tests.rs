#![cfg_attr(not(target_os = "linux"), allow(unused_imports, dead_code))]

use std::path::{Path, PathBuf};

use tabref_cdp::{LaunchOptions, ProcessView, process_start};
use tabref_core::RefStore;

use super::{find_user_chrome, runs_profile, same_process, shown_dir};
use crate::session::session_options::SessionOptions;
use crate::session::user_chrome_record::{
    UserChromeRecord, choice_in_use, forget_user_chrome, opt_in_guard, save_user_chrome,
};

fn process(pid: u32, exe: &str) -> ProcessView {
    ProcessView {
        pid,
        exe: PathBuf::from(exe),
        cmdline: vec!["chrome".to_owned()],
    }
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tabref-uc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A profile whose lock names `pid`, with remote debugging on.
#[cfg(unix)]
fn profile(name: &str, pid: u32) -> PathBuf {
    let dir = temp(name);
    std::fs::write(
        dir.join("DevToolsActivePort"),
        "40123\n/devtools/browser/abc\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(format!("myhost-{pid}"), dir.join("SingletonLock")).unwrap();
    dir
}

#[test]
fn only_the_browser_holding_the_profile_lock_runs_the_profile() {
    let ours = runs_profile(Some(4242));
    assert!(ours(&process(4242, "/opt/google/chrome/chrome")));
    assert!(!ours(&process(4243, "/opt/google/chrome/chrome")));
    assert!(!ours(&process(4242, "/usr/bin/python3")));
    assert!(!runs_profile(None)(&process(
        4242,
        "/opt/google/chrome/chrome"
    )));
}

#[cfg(unix)]
#[test]
fn a_named_profile_needs_remote_debugging_and_a_running_browser() {
    let empty = temp("empty");
    assert_eq!(
        find_user_chrome(Some(&empty)).unwrap_err().kind(),
        Some("cdp_active_port_missing")
    );
    std::fs::write(
        empty.join("DevToolsActivePort"),
        "40123\n/devtools/browser/abc\n",
    )
    .unwrap();
    assert_eq!(
        find_user_chrome(Some(&empty)).unwrap_err().kind(),
        Some("cdp_user_chrome_not_found")
    );

    let me = std::process::id();
    let held = profile("held", me);
    let record = find_user_chrome(Some(&held)).unwrap();
    let _ = std::fs::remove_dir_all(&empty);
    let _ = std::fs::remove_dir_all(&held);
    assert_eq!((record.user_data_dir, record.pid), (held, me));
    assert_eq!(record.started, process_start(me));
}

#[cfg(unix)]
#[test]
fn a_browser_that_exited_or_restarted_is_not_the_one_opted_in_to() {
    let me = std::process::id();
    let dir = profile("restart", me);
    let record = |pid: u32, started: Option<String>| UserChromeRecord {
        name: "Google Chrome".to_owned(),
        user_data_dir: dir.clone(),
        pid,
        started,
    };

    let results = (
        same_process(&record(me, process_start(me))),
        same_process(&record(me + 1, process_start(me))),
        same_process(&record(me, Some("an earlier start".to_owned()))),
        same_process(&record(me, None)),
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(results, (true, false, false, false));
}

#[test]
fn launch_and_cdp_never_use_the_users_browser() {
    let state = temp("choice");
    let store = RefStore::new(&state);
    let record = UserChromeRecord {
        name: "Google Chrome".to_owned(),
        user_data_dir: PathBuf::from("/p"),
        pid: 1,
        started: None,
    };
    save_user_chrome(&store, &record).unwrap();
    let plain = SessionOptions::default();
    let launch = SessionOptions {
        launch: Some(LaunchOptions::default()),
        ..SessionOptions::default()
    };
    let named = SessionOptions {
        cdp: Some("9333".to_owned()),
        ..SessionOptions::default()
    };

    let used = (
        choice_in_use(&store, &plain).map(|(_, r)| r),
        choice_in_use(&store, &launch).is_none(),
        choice_in_use(&store, &named).is_none(),
    );
    forget_user_chrome(&store).unwrap();
    let after = choice_in_use(&store, &plain).is_none();
    let _ = std::fs::remove_dir_all(&state);
    assert_eq!(used, (Some(record), true, true));
    assert!(after);
}

#[cfg(unix)]
#[test]
fn an_opt_in_that_cannot_be_removed_is_an_error_not_a_quiet_success() {
    use std::os::unix::fs::PermissionsExt;
    let state = temp("stuck");
    let store = RefStore::new(&state);
    let record = UserChromeRecord {
        name: "Google Chrome".to_owned(),
        user_data_dir: PathBuf::from("/p"),
        pid: 1,
        started: None,
    };
    save_user_chrome(&store, &record).unwrap();
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o500)).unwrap();
    let stuck = forget_user_chrome(&store);
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
    let cleared = forget_user_chrome(&store);
    let _ = std::fs::remove_dir_all(&state);
    assert!(stuck.is_err());
    assert!(cleared.is_ok());
}

#[test]
fn a_running_command_stops_once_the_opt_in_is_off() {
    let state = temp("guard");
    let store = RefStore::new(&state);
    let record = UserChromeRecord {
        name: "Google Chrome".to_owned(),
        user_data_dir: PathBuf::from("/p"),
        pid: 1,
        started: Some("1".to_owned()),
    };
    save_user_chrome(&store, &record).unwrap();
    let chosen = std::fs::read_to_string(state.join("user_chrome.json")).unwrap();
    let guard = opt_in_guard(&store, chosen);

    let while_on = guard();
    forget_user_chrome(&store).unwrap();
    let after_off = guard();
    let _ = std::fs::remove_dir_all(&state);
    assert!(while_on.is_ok());
    assert_eq!(after_off.unwrap_err().kind(), Some("cdp_user_chrome_off"));
}

#[test]
fn a_profile_under_home_is_shown_short() {
    if let Some(home) = std::env::var_os("HOME") {
        let dir = Path::new(&home).join(".config/google-chrome");
        assert_eq!(shown_dir(&dir), "~/.config/google-chrome");
    }
    assert_eq!(shown_dir(Path::new("/srv/profile")), "/srv/profile");
}
