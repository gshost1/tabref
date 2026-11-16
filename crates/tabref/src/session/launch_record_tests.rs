use tabref_cdp::{LaunchedBrowser, ProfileUse};
use tabref_core::RefStore;

use super::{LAUNCH_FILE, load_launched, save_launched};

fn record() -> LaunchedBrowser {
    LaunchedBrowser {
        pid: 4242,
        port: 40000,
        websocket_path: "/devtools/browser/x".to_owned(),
        profile_dir: std::path::PathBuf::from("/tmp/tabref-profile-s0000001"),
        headless: true,
        started: Some("boot:1".to_owned()),
        profile_use: ProfileUse::THROWAWAY,
    }
}

#[test]
fn a_launch_record_others_could_have_written_is_ignored() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("tabref-launch-record-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = RefStore::new(&dir);
    save_launched(&store, &record()).unwrap();
    let private = load_launched(&store);
    let file = dir.join(LAUNCH_FILE);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o666)).unwrap();
    let open = load_launched(&store);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(private, Some(record()));
    assert_eq!(open, None, "a record others could write was used");
}
