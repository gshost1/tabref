//! Live runs of how `select` proves an ARIA pick took: a listbox reports
//! the selected option's label, not every option's text; a multi-select
//! listbox is refused rather than toggled; an editable combobox whose
//! option click the page ignores fails although the typed text equals the
//! label, and gets its old text back; a combobox whose popup sits inside
//! it fails when the click is ignored; a read-only combobox is never typed
//! into, and its refusal closes the popup.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_select_aria_checks`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, Run, harness, live, serve};

const LISTBOX: &str = include_str!("fixtures/aria/listbox.html");
const IGNORED: &str = include_str!("fixtures/aria/ignored.html");

struct Page {
    tabref: Harness,
    opened: Run,
}

impl Drop for Page {
    fn drop(&mut self) {
        let _ = self.tabref.run(&["daemon", "stop"]);
    }
}

fn page(name: &str, html: &'static str) -> Page {
    let port = serve(&[("/p", html)]);
    let tabref = harness(name);
    let opened = tabref.ok(&["open", &format!("http://127.0.0.1:{port}/p")]);
    Page { tabref, opened }
}

fn js(page: &Page, expression: &str) -> String {
    page.tabref.ok(&["js", expression]).stdout.trim().to_owned()
}

#[test]
fn a_listbox_reports_the_selected_label_and_a_multi_select_one_is_refused() {
    if !live() {
        return;
    }
    let page = page("aria-listbox", LISTBOX);
    assert!(
        page.opened.stdout.contains("option Dog]"),
        "{}",
        page.opened.stdout
    );
    let picked = page.tabref.ok(&["select", "Pet", "Dog"]);
    let first = picked.stdout.lines().next().unwrap_or_default().to_owned();
    assert!(first.ends_with(r#""Cat" -> "Dog""#), "{}", picked.stdout);
    assert!(!first.contains("Fish"), "{}", picked.stdout);
    let refused = page.tabref.run(&["select", "Toys", "Ball"]);
    assert!(!refused.ok, "{}", refused.stdout);
    assert!(
        refused.stderr.contains("multi-select") && refused.stderr.contains("click"),
        "{}",
        refused.stderr
    );
    assert_eq!(
        js(
            &page,
            "document.getElementById('t-ball').getAttribute('aria-selected')"
        ),
        "true"
    );
}

#[test]
fn an_ignored_click_fails_even_when_the_typed_text_is_the_label() {
    if !live() {
        return;
    }
    let page = page("aria-ignored", IGNORED);
    let town = page.opened.ref_for("combobox Town");
    let ignored = page.tabref.run(&["select", &town, "Bern"]);
    assert!(
        !ignored.ok,
        "reported a pick the page ignored: {}",
        ignored.stdout
    );
    assert!(
        ignored.stderr.contains(r#"clicked option "Bern""#)
            && ignored.stderr.contains(r#"put back "Oslo""#),
        "{}",
        ignored.stderr
    );
    assert_eq!(js(&page, "document.getElementById('town').value"), "Oslo");
    let size = page.opened.ref_for("combobox Size");
    let inside = page.tabref.run(&["select", &size, "Large"]);
    assert!(
        !inside.ok,
        "reported a pick the page ignored: {}",
        inside.stdout
    );
    assert!(
        inside.stderr.contains(r#"still shows "Small""#),
        "{}",
        inside.stderr
    );
}

#[test]
fn a_read_only_combobox_is_picked_by_click_and_never_typed_into() {
    if !live() {
        return;
    }
    let page = page("aria-readonly", IGNORED);
    let kind = page.opened.ref_for("combobox Kind");
    let picked = page.tabref.ok(&["select", &kind, "Coffee"]);
    assert!(
        picked.stdout.contains(r#""Tea" -> "Coffee""#),
        "{}",
        picked.stdout
    );
    let missing = page.tabref.run(&["select", &kind, "Milk"]);
    assert!(!missing.ok, "{}", missing.stdout);
    assert!(
        missing
            .stderr
            .contains(r#"no option matches "Milk"; shown options: Tea | Coffee"#)
            && !missing.stderr.contains("typed"),
        "{}",
        missing.stderr
    );
    assert_eq!(js(&page, "document.getElementById('kind').value"), "Coffee");
    assert_eq!(js(&page, "document.getElementById('kinds').hidden"), "true");
}
