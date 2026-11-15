use super::{landed_size, numbered, place, safe_file_name};

fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tabref-download-place-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_landed_download_takes_its_name_and_a_taken_name_gets_a_number() {
    let dir = folder("names");
    std::fs::write(dir.join("report.csv"), "old").unwrap();
    std::fs::write(dir.join("0b1c-2d"), "new").unwrap();
    assert_eq!(landed_size(&dir, "0b1c-2d"), Some(3));

    let placed = place(&dir, "0b1c-2d", "report.csv").unwrap();

    assert_eq!(placed, dir.join("report (1).csv"));
    assert_eq!(
        std::fs::read_to_string(dir.join("report.csv")).unwrap(),
        "old"
    );
    assert_eq!(std::fs::read_to_string(placed).unwrap(), "new");
    assert!(!dir.join("0b1c-2d").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_download_still_being_written_has_not_landed() {
    let dir = folder("partial");
    std::fs::write(dir.join("ab12"), "").unwrap();
    std::fs::write(dir.join("ab12.crdownload"), "part").unwrap();
    assert_eq!(landed_size(&dir, "ab12"), None);
    assert_eq!(landed_size(&dir, "../etc"), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_suggested_name_cannot_leave_the_folder() {
    assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
    assert_eq!(safe_file_name("a:b?.txt"), "a_b_.txt");
    assert_eq!(safe_file_name(".."), "download");
    assert_eq!(numbered("archive.tar.gz", 2), "archive.tar (2).gz");
    assert_eq!(numbered("README", 1), "README (1)");
}
