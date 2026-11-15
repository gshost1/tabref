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
    let config = std::env::temp_dir().join(format!("tabref-defaults-{}", std::process::id()));
    let browsers = user_data_dirs_with(|name| {
        (name == "XDG_CONFIG_HOME").then(|| OsString::from(config.as_os_str()))
    });
    let chrome = config.join("google-chrome");
    std::fs::create_dir_all(&chrome).unwrap();
    let spelled = config
        .join("google-chrome")
        .join("..")
        .join("google-chrome");

    let found = (
        super::is_default_in(&chrome, &browsers),
        super::is_default_in(&spelled, &browsers),
        super::is_default_in(&config.join("tabref-profile-x"), &browsers),
    );
    let _ = std::fs::remove_dir_all(&config);
    assert_eq!(found, (true, true, false));
}
