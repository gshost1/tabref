use std::path::{Path, PathBuf};

use super::DaemonAddress;

#[test]
fn the_socket_path_is_short_stable_and_distinct_per_fingerprint_and_user() {
    let long_root = PathBuf::from(format!("/home/{}/state", "x".repeat(300)));
    let root = Path::new("/run/user/1000");
    let one = DaemonAddress::from_parts(root, 1000, "root\n9222\n\n", long_root.clone());
    let again = DaemonAddress::from_parts(root, 1000, "root\n9222\n\n", long_root.clone());
    let other = DaemonAddress::from_parts(root, 1000, "root\n9333\n\n", long_root.clone());
    let someone = DaemonAddress::from_parts(root, 1001, "root\n9222\n\n", long_root);

    assert_eq!(one, again);
    assert_ne!(one.dir, other.dir);
    assert_ne!(one.dir, someone.dir);
    assert!(one.socket().to_string_lossy().len() < 64);
    assert!(one.dir.starts_with(root));
    let name = one.dir.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with(&DaemonAddress::user_prefix(1000)));
}

#[test]
fn the_hash_is_fnv1a() {
    let address = DaemonAddress::from_parts(Path::new("/tmp"), 7, "", PathBuf::from("/s"));
    assert_eq!(address.dir, PathBuf::from("/tmp/tabref-7-cbf29ce484222325"));
}
