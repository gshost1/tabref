use super::remove_profile_soon;

fn fresh(name: &str) -> std::path::PathBuf {
    let dir = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn only_a_generated_throwaway_profile_right_in_the_temp_folder_is_removed() {
    let tag = format!("{:07}", std::process::id() % 10_000_000);
    let generated = fresh(&format!("tabref-profile-s{tag}"));
    let loose = fresh(&format!("tabref-profile-mine{tag}"));
    let nest = fresh(&format!("tabref-abandon-{tag}"));
    let nested = nest.join(format!("tabref-profile-s{tag}"));
    std::fs::create_dir_all(&nested).unwrap();

    for dir in [&generated, &loose, &nested] {
        remove_profile_soon(dir);
    }
    let left = (generated.exists(), loose.exists(), nested.exists());
    let _ = std::fs::remove_dir_all(&loose);
    let _ = std::fs::remove_dir_all(&nest);
    assert_eq!(left, (false, true, true));
}

#[cfg(unix)]
#[test]
fn a_link_named_as_a_throwaway_profile_is_not_followed() {
    let tag = format!("{:07}", (std::process::id() + 1) % 10_000_000);
    let target = fresh(&format!("tabref-abandon-target-{tag}"));
    std::fs::write(target.join("keep"), "x").unwrap();
    let link = target.with_file_name(format!("tabref-profile-s{tag}"));
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&target, &link).unwrap();

    remove_profile_soon(&link);
    let kept = target.join("keep").exists();
    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_dir_all(&target);
    assert!(kept, "the link was followed and its target emptied");
}
