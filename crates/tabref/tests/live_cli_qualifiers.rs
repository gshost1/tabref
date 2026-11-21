#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, scratch, serve};

const PAGE: &str = r##"<!doctype html><title>Qualifiers</title>
<section aria-label="docs-preview-old"><button onclick="document.title='revoked'">Revoke</button></section>
<button onclick="document.title='first'">Edit</button><button onclick="document.title='second'">Edit</button>
<h2>Billing</h2><section aria-label="Billing form"><label>Email<input></label><button onclick="document.title='saved'">Save</button></section>
<h2>Shipping</h2><p>Two Words</p>
<label>Search<input id="search"></label>
<a href="#" onclick="document.title='link1';return false">One</a>
<a href="#" onclick="document.title='link2';return false">Two</a>
<a href="#" onclick="document.title='link3';return false">Three</a>
<button onclick="document.title='apostrophe1'">Don't save</button>
<button onclick="document.title='apostrophe2'">Don't save</button>
<div role="region" aria-label="Results" id="results" style="height:120px;overflow:auto">
<button>Results</button><div style="height:1500px">Scrollable results</div></div>
<label>Photo<input id="photo" type="file" multiple></label>"##;

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

#[test]
fn role_words_apostrophes_keyword_values_and_interrupted_qualifiers_work() {
    if !live() {
        return;
    }
    let port = serve(&[("/", PAGE)]);
    let tabref = harness("argv-review");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    tabref.ok(&["fill", "Search", "in"]);
    assert_eq!(
        tabref
            .ok(&["js", "document.getElementById('search').value"])
            .stdout
            .trim(),
        "in"
    );
    for (args, title) in [
        (vec!["click", "link", "nth", "3"], "link3"),
        (vec!["click", "link", "Two"], "link2"),
        (vec!["click", "Don't save", "nth", "2"], "apostrophe2"),
        (vec!["click", "Don't save", "nth", "1"], "apostrophe1"),
        (
            vec![
                "click",
                "Revoke",
                "--timeout",
                "30000",
                "in",
                "docs-preview-old",
            ],
            "revoked",
        ),
    ] {
        tabref.ok(&args);
        assert_eq!(tabref.ok(&["js", "document.title"]).stdout.trim(), title);
    }
    tabref.ok(&["js", "document.querySelectorAll('button').forEach(b=>{if(b.textContent===\"Don't save\")b.remove()});void 0"]);
    tabref.ok(&["js", "const b=document.createElement('button');b.textContent=\"Don't save\";b.onclick=()=>document.title='single';document.body.append(b);void 0"]);
    tabref.ok(&["click", "Don't save"]);
    assert_eq!(tabref.ok(&["js", "document.title"]).stdout.trim(), "single");
    tabref.ok(&["do", "scroll down in \"Results\""]);
    assert_eq!(
        tabref
            .ok(&[
                "js",
                "String(document.getElementById('results').scrollTop>0)"
            ])
            .stdout
            .trim(),
        "true"
    );
    tabref.ok(&[
        "js",
        "document.getElementById('results').scrollTop=0;void 0",
    ]);
    tabref.ok(&["scroll", "down", "in", "Results"]);
    assert_eq!(
        tabref
            .ok(&[
                "js",
                "String(document.getElementById('results').scrollTop>0)"
            ])
            .stdout
            .trim(),
        "true"
    );
    let files = scratch("argv-review-screenshot");
    let image = files.0.join("chart.png");
    tabref.ok(&[
        "screenshot",
        "Edit",
        "--out",
        image.to_str().unwrap(),
        "nth",
        "2",
    ]);
    assert!(std::fs::metadata(image).unwrap().len() > 0);
}

#[test]
fn ambiguous_words_and_empty_targets_have_actionable_usage_errors() {
    if !live() {
        return;
    }
    let tabref = harness("argv-review-errors");
    for (args, expected) in [
        (
            vec!["fill", "Search", "nth", "2"],
            "tabref fill 'Search nth' '2'",
        ),
        (
            vec!["fill", "Email", "in", "Billing", "extra", "a@b.c"],
            "tabref fill 'Email in Billing extra' 'a@b.c'",
        ),
        (vec!["click", "Sign", "in"], "tabref click 'Sign in'"),
        (vec!["scroll", "e5", "e7"], "tabref scroll 'e5 e7'"),
        (vec!["click", "Edit", "nth", "-1"], "nth NUMBER from 1"),
        (vec!["click", ""], "must not be empty"),
        (vec!["click", "Edit", "in", ""], "must not be empty"),
    ] {
        let result = tabref.run(&args);
        assert!(!result.ok);
        assert!(result.stderr.contains(expected), "{}", result.stderr);
    }
}

#[test]
fn upload_keeps_paths_that_happen_to_be_qualifier_words() {
    if !live() {
        return;
    }
    let port = serve(&[("/", PAGE)]);
    let tabref = harness("argv-review-upload");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let files = scratch("argv-review-upload");
    for name in ["after", "b.png", "c.png"] {
        std::fs::write(files.0.join(name), b"fixture").unwrap();
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_tabref"))
        .current_dir(&files.0)
        .args([
            "--cdp",
            &tabref.browser.port.to_string(),
            "--timeout",
            "30000",
            "upload",
            "Photo",
            "after",
            "b.png",
            "c.png",
        ])
        .env("TABREF_HOME", &tabref.home)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        tabref
            .ok(&[
                "js",
                "Array.from(document.getElementById('photo').files,f=>f.name).join(',')"
            ])
            .stdout
            .trim(),
        "after,b.png,c.png"
    );
}
