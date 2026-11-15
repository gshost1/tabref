use std::path::Path;

use tabref_core::{Deadline, ErrorCode};

use super::{browser_args, launch_chrome};
use crate::LaunchOptions;

#[test]
fn a_headless_launch_asks_for_a_picked_port_and_a_throwaway_profile() {
    let args = browser_args(&LaunchOptions::default(), Path::new("/tmp/p"));
    assert!(args.contains(&"--headless=new".to_owned()));
    assert!(args.contains(&"--remote-debugging-port=0".to_owned()));
    assert!(args.contains(&"--user-data-dir=/tmp/p".to_owned()));
    assert_eq!(args.last().map(String::as_str), Some("about:blank"));
}

#[test]
fn a_headed_launch_keeps_its_window_and_extra_flags() {
    let options = LaunchOptions {
        headed: true,
        extra_args: vec!["--no-sandbox".into()],
        ..LaunchOptions::default()
    };
    let args = browser_args(&options, Path::new("/tmp/p"));
    assert!(!args.iter().any(|arg| arg.starts_with("--headless")));
    assert!(args.contains(&"--no-sandbox".to_owned()));
}

#[test]
fn a_binary_that_does_not_exist_fails_before_waiting() {
    let options = LaunchOptions {
        binary: Some("/nonexistent/tabref-test-browser".into()),
        profile_root: Some(std::env::temp_dir()),
        ..LaunchOptions::default()
    };
    let error = launch_chrome(&options, Deadline::after(2_000).unwrap()).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.kind(), Some("chrome_spawn_failed"));
}

#[cfg(unix)]
#[test]
fn the_throwaway_profile_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;

    use super::profile_dir;
    let options = LaunchOptions {
        profile_root: Some(std::env::temp_dir()),
        ..LaunchOptions::default()
    };
    let dir = profile_dir(&options).unwrap();
    let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
    std::fs::remove_dir(&dir).unwrap();
    assert_eq!(mode, 0o700);
}

#[test]
fn a_port_file_left_by_an_earlier_browser_is_not_taken_for_a_new_one() {
    let dir = std::env::temp_dir().join(format!("tabref-stale-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let nothing = super::port_file_print(&dir);
    std::fs::write(
        dir.join("DevToolsActivePort"),
        "40001\n/devtools/browser/old\n",
    )
    .unwrap();
    let before = super::port_file_print(&dir);

    let stale = super::changed_port_file(&dir, before.as_deref());
    let first = super::changed_port_file(&dir, nothing.as_deref());
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(
        dir.join("DevToolsActivePort"),
        "40002\n/devtools/browser/new\n",
    )
    .unwrap();
    let fresh = super::changed_port_file(&dir, before.as_deref());
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(stale, None);
    assert_eq!(first.map(|active| active.port), Some(40001));
    assert_eq!(fresh.map(|active| active.port), Some(40002));
}

#[cfg(unix)]
#[test]
fn a_new_persistent_profile_is_private_and_an_existing_one_is_kept() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("tabref-persist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let options = LaunchOptions {
        profile_dir: Some(root.join("profile")),
        ..LaunchOptions::default()
    };

    let made = super::profile_dir(&options).unwrap();
    let mode = std::fs::metadata(&made).unwrap().permissions().mode() & 0o777;
    std::fs::write(made.join("Cookies"), "kept").unwrap();
    let again = super::profile_dir(&options).unwrap();
    let kept = std::fs::read_to_string(again.join("Cookies")).unwrap();
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(made, root.join("profile"));
    assert_eq!(mode, 0o700);
    assert_eq!(kept, "kept");
}
