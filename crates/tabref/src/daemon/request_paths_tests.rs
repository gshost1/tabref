use std::path::PathBuf;

use super::{apply_paths, resolved_paths, runs_in_process};
use crate::cli::command::Command;

fn open(url: &str) -> Command {
    Command::Open {
        url: url.into(),
        launch: false,
        headed: false,
    }
}

#[test]
fn open_sends_the_address_an_in_process_run_would_navigate_to() {
    let paths = resolved_paths(&open("example.com")).unwrap();
    assert_eq!(paths, ["https://example.com"]);

    let mut command = open("page.html");
    apply_paths(&mut command, Some(vec!["file:///work/page.html".into()]));
    let Command::Open { url, .. } = command else {
        unreachable!()
    };
    assert_eq!(url, "file:///work/page.html");
}

#[test]
fn upload_files_are_made_absolute_and_a_missing_one_runs_in_process() {
    let here = std::env::current_dir().unwrap();
    let mut command = Command::Upload {
        target: "@s1:e1".into(),
        files: vec![PathBuf::from("Cargo.toml")],
    };
    assert!(!runs_in_process(&command));
    let paths = resolved_paths(&command).unwrap();
    assert_eq!(paths, [here.join("Cargo.toml").to_string_lossy()]);
    apply_paths(&mut command, Some(paths));
    let Command::Upload { files, .. } = &command else {
        unreachable!()
    };
    assert!(files[0].is_absolute());

    let missing = Command::Upload {
        target: "@s1:e1".into(),
        files: vec![PathBuf::from("no-such-file.txt")],
    };
    assert!(runs_in_process(&missing));
}
