use super::parse_lsof_names;

#[test]
fn lsof_names_are_read_from_n_lines() {
    let output = "p42\nf3\nn/Users/me/Chrome/Default/History\nf4\nn127.0.0.1:9222\n";
    assert_eq!(
        parse_lsof_names(output),
        vec!["/Users/me/Chrome/Default/History", "127.0.0.1:9222"]
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_process_holds_the_folders_of_the_files_it_has_open() {
    use super::holds_profile;

    let dir = std::env::temp_dir().join(format!("tabref-holder-{}", std::process::id()));
    let other = std::env::temp_dir().join(format!("tabref-holder-other-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("Default")).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    let file = std::fs::File::create(dir.join("Default").join("History")).unwrap();

    let held = holds_profile(std::process::id(), &dir);
    let not_held = holds_profile(std::process::id(), &other);
    drop(file);
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&other);

    assert_eq!(held, Some(true));
    assert_eq!(not_held, Some(false));
}
