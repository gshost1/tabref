use std::path::PathBuf;

use super::absolutize;
use crate::cli::command::Command;

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tabref-paths-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn relative_upload_files_resolve_against_the_client_directory() {
    let cwd = std::env::temp_dir().join("work");
    let absolute = std::env::temp_dir().join("abs").join("b.txt");
    let mut command = Command::Upload {
        target: "@s1:e1".into(),
        files: vec![PathBuf::from("a.txt"), absolute.clone()],
    };
    absolutize(&mut command, &cwd);
    let Command::Upload { files, .. } = command else {
        unreachable!()
    };
    assert_eq!(files, [cwd.join("a.txt"), absolute]);
}

#[test]
fn open_rewrites_only_an_existing_relative_file() {
    let cwd = temp_dir();
    std::fs::write(cwd.join("page.html"), "<p>hi</p>").unwrap();
    for (given, expected) in [
        (
            "page.html",
            cwd.join("page.html").to_string_lossy().into_owned(),
        ),
        ("example.com", "example.com".to_owned()),
        (
            "https://x.test/page.html",
            "https://x.test/page.html".to_owned(),
        ),
    ] {
        let mut command = Command::Open {
            url: given.into(),
            launch: false,
            headed: false,
        };
        absolutize(&mut command, &cwd);
        let Command::Open { url, .. } = command else {
            unreachable!()
        };
        assert_eq!(url, expected);
    }
    std::fs::remove_dir_all(cwd).unwrap();
}
