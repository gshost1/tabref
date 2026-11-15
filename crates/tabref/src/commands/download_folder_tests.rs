use std::path::{Path, PathBuf};

use super::within;

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-files")
        .join(format!("download-folder-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::path::absolute(dir).unwrap()
}

#[test]
fn the_state_directory_and_folders_inside_it_are_within_it() {
    let base = scratch("lexical");
    let root = base.join("state");
    assert!(within(&root, &root).unwrap());
    assert!(within(&root.join("downloads").join("x"), &root).unwrap());
    assert!(!within(&base.join("state2"), &root).unwrap());
    assert!(!within(&base, &root).unwrap());
    let _ = std::fs::remove_dir_all(&base);
}

#[cfg(unix)]
#[test]
fn a_link_into_the_state_directory_is_within_it() {
    let base = scratch("linked");
    let root = base.join("state");
    std::fs::create_dir_all(&root).unwrap();
    let link = base.join("innocent");
    std::os::unix::fs::symlink(&root, &link).unwrap();
    assert!(within(&link, &root).unwrap());
    assert!(within(&link.join("new").join("deeper"), &root).unwrap());
    let dangling = base.join("dangling");
    std::os::unix::fs::symlink(base.join("nowhere"), &dangling).unwrap();
    assert!(within(&dangling.join("x"), &root).is_err());
    let _ = std::fs::remove_dir_all(&base);
}
