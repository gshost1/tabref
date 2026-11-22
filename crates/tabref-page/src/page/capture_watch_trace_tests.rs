#![allow(clippy::unwrap_used)]

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn directory() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tabref-trace-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    path
}

#[test]
fn logged_urls_and_event_params_are_private() {
    let dir = directory();
    let path = dir.join("trace");
    record_to(
        &path,
        "opaque.events",
        json!({
            "urls":["https://example.test/style.css?token=secret#fragment", "../x.css?private#x"],
            "why":"frame:iframe:https://example.test/frame?session=private#x",
            "late":[{"method":"CSS.fontsUpdated", "params":{"fontFamily":"private-family"}}],
            "error":"failed for https://example.test/a?private#x during receive"
        }),
    );
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains("secret")
            && !text.contains("private")
            && !text.contains("fragment")
            && !text.contains("params"),
        "{text}"
    );
    assert!(
        text.contains("CSS.fontsUpdated")
            && text.contains("style.css")
            && text.contains("during receive")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn new_trace_files_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = directory();
    let path = dir.join("trace");
    record_to(&path, "check", json!({}));
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn existing_trace_is_restricted_before_appending() {
    use std::os::unix::fs::PermissionsExt;
    let dir = directory();
    let path = dir.join("trace");
    std::fs::write(&path, "").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    record_to(&path, "check", json!({"verdict":"same"}));
    record_to(&path, "check", json!({"verdict":"mutated"}));
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn trace_symlinks_are_refused() {
    let dir = directory();
    let target = dir.join("target");
    let path = dir.join("trace");
    std::fs::write(&target, "unchanged").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    record_to(&path, "check", json!({}));
    assert_eq!(std::fs::read_to_string(target).unwrap(), "unchanged");
    let dangling = dir.join("dangling");
    std::os::unix::fs::symlink(dir.join("absent"), &dangling).unwrap();
    record_to(&dangling, "check", json!({}));
    assert!(!dir.join("absent").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn empty_trace_and_unwritable_destination_are_safe() {
    let dir = directory();
    record_to(&dir, "check", json!({}));
    let path = dir.join("trace");
    record_to(&path, "check", json!({}));
    assert!(serde_json::from_str::<Value>(&std::fs::read_to_string(path).unwrap()).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}
