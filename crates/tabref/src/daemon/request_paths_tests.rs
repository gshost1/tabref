use std::path::PathBuf;

use super::{apply_paths, resolved_paths, runs_in_process};
use crate::cli::command::Command;

fn open(url: &str) -> Command {
    Command::Open {
        url: url.into(),
        launch: false,
        headed: false,
        profile: None,
        new_tab: false,
        download_dir: None,
        no_download_dir: false,
        download_max_mb: None,
    }
}

#[test]
fn a_relative_launch_profile_reaches_the_daemon_as_the_clients_directory() {
    let here = std::env::current_dir().unwrap();
    let mut asked = open("example.com");
    if let Command::Open { profile, .. } = &mut asked {
        *profile = Some(PathBuf::from("profiles/work"));
    }
    let paths = resolved_paths(&asked).unwrap();

    let mut command = open("example.com");
    apply_paths(&mut command, Some(paths));
    let Command::Open {
        profile,
        download_dir,
        ..
    } = command
    else {
        unreachable!()
    };
    assert_eq!(profile, Some(here.join("profiles/work")));
    assert_eq!(
        download_dir, None,
        "the profile was taken for the download folder"
    );
}

#[test]
fn a_relative_download_folder_is_sent_as_the_one_where_the_command_was_typed() {
    let here = std::env::current_dir().unwrap();
    let mut typed = open("example.com");
    if let Command::Open { download_dir, .. } = &mut typed {
        *download_dir = Some(PathBuf::from("dl"));
    }
    let paths = resolved_paths(&typed).unwrap();
    assert_eq!(paths[1], here.join("dl").to_string_lossy());

    let mut parsed = typed;
    if let Command::Open { download_dir, .. } = &mut parsed {
        *download_dir = Some(PathBuf::from("/dl"));
    }
    apply_paths(&mut parsed, Some(paths));
    let Command::Open { download_dir, .. } = parsed else {
        unreachable!()
    };
    assert_eq!(download_dir, Some(here.join("dl")));
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
