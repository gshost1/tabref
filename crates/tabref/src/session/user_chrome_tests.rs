use std::path::{Path, PathBuf};

use tabref_cdp::ProcessView;

use super::{find_user_chrome, runs_profile, shown_dir};

fn process(pid: u32, exe: &str) -> ProcessView {
    ProcessView {
        pid,
        exe: PathBuf::from(exe),
        cmdline: vec!["chrome".to_owned()],
    }
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

#[test]
fn a_named_profile_without_remote_debugging_is_an_error() {
    let dir = std::env::temp_dir().join(format!("tabref-userchrome-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let missing = find_user_chrome(Some(&dir)).unwrap_err();
    assert_eq!(missing.kind(), Some("cdp_active_port_missing"));

    std::fs::write(
        dir.join("DevToolsActivePort"),
        "40123\n/devtools/browser/abc\n",
    )
    .unwrap();
    let record = find_user_chrome(Some(&dir)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(record.user_data_dir, dir);
}

#[test]
fn a_profile_under_home_is_shown_short() {
    if let Some(home) = std::env::var_os("HOME") {
        let dir = Path::new(&home).join(".config/google-chrome");
        assert_eq!(shown_dir(&dir), "~/.config/google-chrome");
    }
    assert_eq!(shown_dir(Path::new("/srv/profile")), "/srv/profile");
}
