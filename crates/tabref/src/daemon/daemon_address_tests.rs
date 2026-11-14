use std::path::PathBuf;

use super::DaemonAddress;

#[test]
fn the_socket_path_is_short_stable_and_distinct_per_fingerprint() {
    let long_root = PathBuf::from(format!("/home/{}/state", "x".repeat(300)));
    let one = DaemonAddress::from_fingerprint("root\n9222\n\n", long_root.clone());
    let again = DaemonAddress::from_fingerprint("root\n9222\n\n", long_root.clone());
    let other = DaemonAddress::from_fingerprint("root\n9333\n\n", long_root);

    assert_eq!(one, again);
    assert_ne!(one.dir, other.dir);
    assert!(one.socket().to_string_lossy().len() < 64);
    assert!(one.dir.starts_with("/tmp"));
    let name = one.dir.file_name().unwrap().to_string_lossy();
    assert!(name.starts_with("tabref-") && name.len() == 23);
}

#[test]
fn the_hash_is_fnv1a() {
    let address = DaemonAddress::from_fingerprint("", PathBuf::from("/s"));
    assert_eq!(address.dir, PathBuf::from("/tmp/tabref-cbf29ce484222325"));
}
