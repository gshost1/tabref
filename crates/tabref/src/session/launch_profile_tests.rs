use std::path::{Path, PathBuf};

use super::{refusal, same_dir};

fn never(_: &Path) -> bool {
    false
}

#[test]
fn the_users_own_profile_is_refused_with_the_opt_in_named() {
    let theirs = PathBuf::from("/home/u/.config/google-chrome");
    let error = refusal(&theirs, Path::new("/home/u/.tabref"), &|dir| dir == theirs).unwrap();
    assert_eq!(error.kind(), Some("launch_profile_refused"));
    assert!(error.suggestion.unwrap().contains("connect --user-chrome"));
}

#[test]
fn tabrefs_state_directory_is_refused_in_and_around_it() {
    let root = Path::new("/home/u/.tabref");
    for dir in ["/home/u/.tabref", "/home/u/.tabref/profile", "/home/u"] {
        let error = refusal(Path::new(dir), root, &never);
        assert!(error.is_some(), "{dir}");
    }
    assert!(refusal(Path::new("/home/u/profiles/work"), root, &never).is_none());
}

#[test]
fn two_spellings_of_one_directory_are_the_same() {
    let dir = std::env::temp_dir();
    assert!(same_dir(
        &dir,
        &dir.join(".").join("..").join(dir.file_name().unwrap())
    ));
    assert!(!same_dir(&dir, &dir.join("other")));
}
