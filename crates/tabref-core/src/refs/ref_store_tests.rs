use super::RefStore;
use crate::{ErrorCode, RefEntry, RefId, RefMap};

fn store(name: &str) -> (RefStore, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("tabref-store-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    (RefStore::new(&dir), dir)
}

fn map_with(token: &str) -> RefMap {
    let mut map = RefMap::new("TAB", "https://example.test/");
    map.push(RefEntry {
        role: "button".into(),
        name: Some("Go".into()),
        token: token.into(),
    });
    map
}

#[test]
fn a_saved_snapshot_loads_back_and_becomes_latest() {
    let (store, dir) = store("roundtrip");
    store.save("s1111111", &map_with("cdp:T:F:L:1")).unwrap();
    store.save("s2222222", &map_with("cdp:T:F:L:2")).unwrap();

    let (map, entry) = store.entry(&RefId::new("s1111111", 1)).unwrap();
    let latest = store.latest_id();
    std::fs::remove_dir_all(&dir).unwrap();

    assert_eq!(map.tab_id, "TAB");
    assert_eq!(entry.token, "cdp:T:F:L:1");
    assert_eq!(latest.as_deref(), Some("s2222222"));
}

#[test]
fn a_missing_snapshot_and_an_unknown_element_are_distinct_errors() {
    let (store, dir) = store("missing");
    store.save("s1111111", &map_with("cdp:T:F:L:1")).unwrap();

    let missing = store.entry(&RefId::new("s9999999", 1)).unwrap_err();
    let unknown = store.entry(&RefId::new("s1111111", 2)).unwrap_err();
    std::fs::remove_dir_all(&dir).unwrap();

    assert_eq!(missing.code, ErrorCode::SnapshotNotFound);
    assert_eq!(unknown.code, ErrorCode::StaleRef);
}

#[test]
fn state_files_round_trip_and_remove() {
    let (store, dir) = store("state");
    store.write_state("tab-9222", "TARGET").unwrap();
    let read = store.read_state("tab-9222");
    store.remove_state("tab-9222");
    let gone = store.read_state("tab-9222");
    std::fs::remove_dir_all(&dir).unwrap();

    assert_eq!(read.as_deref(), Some("TARGET"));
    assert_eq!(gone, None);
}

#[test]
fn old_snapshots_are_pruned_and_the_newest_survives() {
    let (store, dir) = store("prune");
    let map = map_with("cdp:T:F:L:1");
    for index in 0..130 {
        store.save(&format!("s{index:07}"), &map).unwrap();
    }
    let kept = std::fs::read_dir(dir.join("snapshots")).unwrap().count();
    let newest = store.load("s0000129").is_ok();
    std::fs::remove_dir_all(&dir).unwrap();

    assert!(kept <= 128, "{kept}");
    assert!(newest);
}
