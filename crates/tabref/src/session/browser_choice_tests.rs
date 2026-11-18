use std::path::PathBuf;

use tabref_cdp::{LaunchOptions, LaunchedBrowser};
use tabref_core::RefStore;

use super::choice_key;
use crate::session::launch_record::save_launched;
use crate::session::session_options::SessionOptions;

fn launched(profile: &str) -> LaunchedBrowser {
    LaunchedBrowser {
        pid: 1,
        port: 40000,
        websocket_path: "/devtools/browser/x".to_owned(),
        profile_dir: PathBuf::from(profile),
        headless: true,
        started: None,
        profile_use: tabref_cdp::ProfileUse::PERSISTENT,
    }
}

fn launch_on(profile: &str) -> SessionOptions {
    SessionOptions {
        launch: Some(LaunchOptions {
            profile_dir: Some(PathBuf::from(profile)),
            ..LaunchOptions::default()
        }),
        ..SessionOptions::default()
    }
}

#[test]
fn a_launch_on_another_profile_is_never_served_by_the_recorded_browser() {
    let state = std::env::temp_dir().join(format!("tabref-choice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).unwrap();
    let store = RefStore::new(&state);
    let plain = SessionOptions::default();

    let none = choice_key(&store, &plain);
    save_launched(&store, &launched("/profiles/a")).unwrap();
    let recorded = choice_key(&store, &plain);
    let same_profile = choice_key(&store, &launch_on("/profiles/a"));
    let other_profile = choice_key(&store, &launch_on("/profiles/b"));
    save_launched(&store, &launched("/profiles/b")).unwrap();
    let relaunched = choice_key(&store, &plain);
    let _ = std::fs::remove_dir_all(&state);

    assert_eq!(none, None);
    assert!(recorded.is_some());
    assert_eq!(same_profile, recorded);
    assert_ne!(other_profile, recorded);
    assert_ne!(relaunched, recorded);
}
