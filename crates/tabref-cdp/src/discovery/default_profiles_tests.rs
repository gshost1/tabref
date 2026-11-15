use std::ffi::OsString;
use std::path::PathBuf;

use super::user_data_dirs_with;

#[cfg(target_os = "linux")]
#[test]
fn linux_profiles_live_under_the_xdg_config_dir_or_home() {
    let xdg = user_data_dirs_with(|name| {
        (name == "XDG_CONFIG_HOME").then(|| OsString::from("/x/config"))
    });
    assert_eq!(xdg[0].name, "Google Chrome");
    assert_eq!(
        xdg[0].user_data_dir,
        PathBuf::from("/x/config/google-chrome")
    );
    assert!(
        xdg.iter()
            .any(|b| b.user_data_dir == PathBuf::from("/x/config/chromium"))
    );

    let home = user_data_dirs_with(|name| (name == "HOME").then(|| OsString::from("/home/u")));
    assert_eq!(
        home[0].user_data_dir,
        PathBuf::from("/home/u/.config/google-chrome")
    );
    assert!(user_data_dirs_with(|_| None).is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn a_default_profile_is_recognised_in_any_spelling_and_others_are_not() {
    let Some(first) = super::default_user_data_dirs().into_iter().next() else {
        return;
    };
    let spelled = first
        .user_data_dir
        .join("..")
        .join(first.user_data_dir.file_name().unwrap_or_default());
    assert!(super::is_default_user_data_dir(&first.user_data_dir));
    assert!(super::is_default_user_data_dir(&spelled) || !first.user_data_dir.exists());
    assert!(!super::is_default_user_data_dir(
        &std::env::temp_dir().join("tabref-profile-x")
    ));
}
