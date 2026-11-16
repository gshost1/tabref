use std::path::{Path, PathBuf};

use super::{default_out, screenshot_path};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tabref-shot-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(&dir).unwrap()
}

#[test]
fn with_no_out_the_screenshot_goes_in_the_working_directory() {
    let base = temp("default");
    let state = base.join("state");
    let path = screenshot_path(&default_out(&base), &state);
    let _ = std::fs::remove_dir_all(&base);
    let path = path.unwrap();
    assert_eq!(path.parent(), Some(base.as_path()));
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("tabref-screenshot-") && name.ends_with(".png"),
        "{name}"
    );
}

#[test]
fn an_out_in_or_through_the_state_directory_is_refused() {
    let base = temp("state");
    let state = base.join("state");
    std::fs::create_dir_all(state.join("snapshots")).unwrap();
    std::fs::create_dir_all(base.join("x")).unwrap();
    let refused = [
        screenshot_path(&state.join("shot.png"), &state),
        screenshot_path(&state.join("snapshots").join("shot.png"), &state),
        screenshot_path(&default_out(&state), &state),
        screenshot_path(
            &base.join("x").join("..").join("state").join("s.png"),
            &state,
        ),
    ];
    let fine = screenshot_path(&base.join("shot.png"), &state);
    let _ = std::fs::remove_dir_all(&base);
    for result in refused {
        let error = result.unwrap_err();
        assert_eq!(error.kind(), Some("screenshot_path_refused"), "{error:?}");
    }
    assert_eq!(fine.unwrap(), base.join("shot.png"));
}

#[cfg(unix)]
#[test]
fn an_out_through_a_link_to_the_state_directory_is_refused() {
    let base = temp("alias");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    std::os::unix::fs::symlink(&state, base.join("alias")).unwrap();
    let through = screenshot_path(&base.join("alias").join("shot.png"), &state);
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(through.unwrap_err().kind(), Some("screenshot_path_refused"));
}

#[test]
fn an_out_in_a_missing_folder_or_naming_a_folder_is_an_argument_error() {
    let base = temp("missing");
    let state = base.join("state");
    let missing = screenshot_path(&base.join("nope").join("shot.png"), &state);
    let folder = screenshot_path(&base, &state);
    let _ = std::fs::remove_dir_all(&base);
    for result in [missing, folder] {
        let error = result.unwrap_err();
        assert_eq!(error.code, tabref_core::ErrorCode::InvalidArgs, "{error:?}");
    }
}

#[test]
fn the_default_name_differs_between_two_screenshots() {
    let cwd = Path::new("/work");
    assert_ne!(default_out(cwd), default_out(cwd));
}
