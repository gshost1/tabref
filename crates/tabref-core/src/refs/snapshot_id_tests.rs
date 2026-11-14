use std::collections::HashSet;

use super::{new_snapshot_id, validate_snapshot_id};

#[test]
fn new_ids_are_short_valid_and_distinct() {
    let ids: HashSet<String> = (0..500).map(|_| new_snapshot_id()).collect();
    assert_eq!(ids.len(), 500);
    for id in ids {
        assert_eq!(id.len(), 8, "{id}");
        assert!(id.starts_with('s'));
        validate_snapshot_id(&id).unwrap();
    }
}

#[test]
fn ids_that_could_escape_a_directory_are_refused() {
    for id in ["", "ab", "../x", "a/b", "a.b", &"x".repeat(65)] {
        assert!(validate_snapshot_id(id).is_err(), "{id}");
    }
}
