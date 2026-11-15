use super::remove_profile_soon;

#[test]
fn only_a_directory_named_as_a_throwaway_profile_is_removed() {
    let base = std::env::temp_dir().join(format!("tabref-abandon-{}", std::process::id()));
    let throwaway = base.join("tabref-profile-x");
    let other = base.join("work");
    for dir in [&throwaway, &other] {
        std::fs::create_dir_all(dir).unwrap();
    }
    remove_profile_soon(&throwaway);
    remove_profile_soon(&other);
    let left = (throwaway.exists(), other.exists());
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(left, (false, true));
}
