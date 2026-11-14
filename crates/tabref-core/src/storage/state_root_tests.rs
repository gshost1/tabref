use std::path::PathBuf;

use super::root_from;
use crate::ErrorCode;

#[test]
fn home_fallback_is_dot_tabref() {
    let home = std::env::temp_dir();
    assert_eq!(
        root_from(None, Some(home.clone())).unwrap(),
        home.join(".tabref")
    );
}

#[test]
fn a_configured_absolute_directory_wins() {
    let configured = std::env::temp_dir().join("tabref-state-root-test-missing");
    assert_eq!(
        root_from(Some(configured.clone()), Some(PathBuf::from("/nowhere"))).unwrap(),
        configured
    );
}

#[test]
fn a_relative_or_empty_root_is_invalid_args() {
    for configured in ["relative/dir", ""] {
        let error = root_from(Some(PathBuf::from(configured)), None).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgs, "{configured}");
    }
}

#[test]
fn a_file_root_is_invalid_args() {
    let file = std::env::temp_dir().join(format!("tabref-root-file-{}", std::process::id()));
    std::fs::write(&file, b"x").unwrap();
    let error = root_from(Some(file.clone()), None).unwrap_err();
    std::fs::remove_file(&file).unwrap();
    assert_eq!(error.code, ErrorCode::InvalidArgs);
}

#[test]
fn no_home_and_no_root_is_internal() {
    assert_eq!(root_from(None, None).unwrap_err().code, ErrorCode::Internal);
}
