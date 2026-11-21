#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, serve};

const PAGE: &str = r#"<!doctype html><title>Qualifiers</title>
<section aria-label="docs-preview-old"><button onclick="document.title='revoked'">Revoke</button></section>
<button onclick="document.title='first'">Edit</button><button onclick="document.title='second'">Edit</button>
<h2>Billing</h2><section aria-label="Billing form"><label>Email<input></label><button onclick="document.title='saved'">Save</button></section>
<h2>Shipping</h2><p>Two Words</p>"#;

#[test]
fn split_qualifiers_and_multiword_find_work_like_steps() {
    if !live() {
        return;
    }
    let port = serve(&[("/", PAGE)]);
    let tabref = harness("argv-qualifiers");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    for (args, title) in [
        (vec!["click", "Revoke", "in", "docs-preview-old"], "revoked"),
        (vec!["click", "Edit", "nth", "2"], "second"),
        (vec!["click", "Edit", "nth", "1"], "first"),
        (vec!["click", "button \"Edit\"", "nth", "2"], "second"),
        (vec!["click", "Save", "after", "Billing"], "saved"),
        (vec!["click", "Save", "before", "Shipping"], "saved"),
    ] {
        tabref.ok(&args);
        assert_eq!(tabref.ok(&["js", "document.title"]).stdout.trim(), title);
    }
    let filled = tabref.ok(&["fill", "Email", "in", "Billing form", "a@b.c"]);
    assert!(filled.stdout.contains("a@b.c"), "{}", filled.stdout);
    assert_eq!(
        tabref
            .ok(&["js", "document.querySelector('input').value"])
            .stdout
            .trim(),
        "a@b.c"
    );
    let ambiguous = tabref.run(&["click", "Edit", "nth"]);
    assert!(!ambiguous.ok);
    assert!(
        ambiguous
            .stderr
            .contains("quote the whole target: tabref click '"),
        "{}",
        ambiguous.stderr
    );
    let found = tabref.ok(&["find", "Two", "Words"]);
    assert!(
        found.stdout.contains("1 match for \"Two Words\""),
        "{}",
        found.stdout
    );
}
