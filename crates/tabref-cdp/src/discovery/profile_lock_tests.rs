#![cfg_attr(not(target_os = "linux"), allow(unused_imports, dead_code))]

use super::{lock_pid, profile_lock_pid};

#[test]
fn the_lock_names_the_browser_pid_after_the_host() {
    assert_eq!(lock_pid("host-123456"), Some(123_456));
    assert_eq!(lock_pid("host.local-42"), Some(42));
    assert_eq!(lock_pid("no-pid-here"), None);
    assert_eq!(lock_pid("garbage"), None);
}

#[cfg(unix)]
#[test]
fn a_profile_without_a_lock_has_no_browser() {
    let dir = std::env::temp_dir().join(format!("tabref-lock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(profile_lock_pid(&dir), None);
    std::os::unix::fs::symlink("myhost-7777", dir.join("SingletonLock")).unwrap();
    assert_eq!(profile_lock_pid(&dir), Some(7777));
    let _ = std::fs::remove_dir_all(&dir);
}
