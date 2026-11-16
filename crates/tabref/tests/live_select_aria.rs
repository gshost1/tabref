//! Live runs of `select` on the ARIA patterns a page builds instead of a
//! `<select>`: a select-only combobox, an editable combobox whose options
//! appear only once text is typed, a button that opens a listbox, and a
//! combobox with 200 options. Each is opened as a user would, the option is
//! matched as a native select's is, clicked, and the control is read back;
//! the result is one line with the old and new value and no option lists.
//! A disabled, ambiguous or missing option is refused with the options, a
//! pick the page ignores fails, and an element that is none of these is
//! refused with the two-step way.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_select_aria`.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, Run, harness, live, serve};

const SELECT_ONLY: &str = include_str!("fixtures/aria/select_only.html");
const EDITABLE: &str = include_str!("fixtures/aria/editable.html");
const BUTTON: &str = include_str!("fixtures/aria/button.html");
const LONG: &str = include_str!("fixtures/aria/long.html");
const WIDGET: &str = include_str!("fixtures/aria/widget.html");

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

fn assert_one_line(run: &Run, first: &str) {
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert!(
        lines.first().is_some_and(|line| line.contains(first)),
        "{}",
        run.stdout
    );
    assert!(
        lines.iter().skip(1).all(|line| !line.starts_with("+ ")
            && !line.starts_with("- ")
            && !line.starts_with("new ")),
        "the popup's options were printed:\n{}",
        run.stdout
    );
}

#[test]
fn a_select_only_combobox_opens_picks_and_reads_back() {
    if !live() {
        return;
    }
    let page = page("aria-select-only", SELECT_ONLY);
    let fruit = page.opened.ref_for("combobox Fruit");
    let picked = page.tabref.ok(&["select", &fruit, "Banana"]);
    assert_one_line(&picked, r#""Apple" -> "Banana""#);
    assert_eq!(
        js(&page, "document.getElementById('fruit').textContent"),
        "Banana"
    );
    assert_eq!(
        js(&page, "document.getElementById('fruits').hidden"),
        "true"
    );
    let prefix = page.tabref.ok(&["select", "Fruit", "cher"]);
    assert_one_line(&prefix, r#""Banana" -> "Cherry""#);
    let stepped = page
        .tabref
        .ok(&["do", "select Fruit Blueberry\nclick Save"]);
    assert!(
        stepped
            .stdout
            .contains(r#"ok select "Fruit" "Cherry" -> "Blueberry""#),
        "{}",
        stepped.stdout
    );
}

#[test]
fn a_disabled_ambiguous_or_missing_option_is_refused_with_the_options() {
    if !live() {
        return;
    }
    let page = page("aria-refused", SELECT_ONLY);
    let fruit = page.opened.ref_for("combobox Fruit");
    for (choice, says) in [
        ("Durian", r#"option "Durian" is disabled"#),
        ("B", r#""B" matches 2 options: Banana | Blueberry"#),
        (
            "Mango",
            r#"no option matches "Mango"; shown options: Apple | Banana | Blueberry | Cherry | Durian (disabled); more may load as the list scrolls or as you type"#,
        ),
    ] {
        let refused = page.tabref.run(&["select", &fruit, choice]);
        assert!(!refused.ok, "{choice}: {}", refused.stdout);
        assert!(
            refused.stderr.contains(says),
            "{choice}: {}",
            refused.stderr
        );
        assert_eq!(
            js(&page, "document.getElementById('fruits').hidden"),
            "true",
            "{choice}: left open"
        );
        assert_eq!(
            js(&page, "document.getElementById('fruit').textContent"),
            "Apple"
        );
    }
}

#[test]
fn a_pick_the_page_ignores_fails() {
    if !live() {
        return;
    }
    let page = page("aria-ignored", SELECT_ONLY);
    let size = page.opened.ref_for("combobox Size");
    let ignored = page.tabref.run(&["select", &size, "Large"]);
    assert!(!ignored.ok, "{}", ignored.stdout);
    assert!(
        ignored.stderr.contains(r#"still shows "Small""#),
        "{}",
        ignored.stderr
    );
}

#[test]
fn an_editable_combobox_gets_the_text_typed_when_no_option_shows() {
    if !live() {
        return;
    }
    let page = page("aria-editable", EDITABLE);
    let city = page.opened.ref_for("combobox City");
    let picked = page.tabref.ok(&["select", &city, "Bern"]);
    assert_one_line(&picked, r#""Oslo" -> "Bern""#);
    assert_eq!(js(&page, "document.getElementById('city').value"), "Bern");
    assert_eq!(
        js(&page, "document.getElementById('cities').children.length"),
        "0"
    );
    let missing = page.tabref.run(&["select", &city, "Paris"]);
    assert!(!missing.ok, "{}", missing.stdout);
    assert!(
        missing.stderr.contains(r#"no option matches "Paris""#)
            && missing.stderr.contains(r#"put back "Bern""#),
        "{}",
        missing.stderr
    );
}

#[test]
fn a_button_that_opens_a_listbox_is_selected_like_a_combobox() {
    if !live() {
        return;
    }
    let page = page("aria-button", BUTTON);
    let colour = page.opened.ref_for("button Colour");
    let picked = page.tabref.ok(&["select", &colour, "Green"]);
    assert_one_line(&picked, r#""Red" -> "Green""#);
    assert_eq!(
        js(&page, "document.getElementById('colour').textContent"),
        "Green"
    );
}

#[test]
fn an_option_far_down_a_long_list_is_scrolled_to_and_picked() {
    if !live() {
        return;
    }
    let page = page("aria-long", LONG);
    let item = page.opened.ref_for("combobox Item");
    let picked = page.tabref.ok(&["select", &item, "Item 187"]);
    assert_one_line(&picked, r#""Item 1" -> "Item 187""#);
    let missing = page.tabref.run(&["select", &item, "Item 999"]);
    assert!(
        missing.stderr.contains("… and 170 more"),
        "{}",
        missing.stderr
    );
}

#[test]
fn a_widget_with_no_aria_pattern_is_refused_with_the_two_step_way() {
    if !live() {
        return;
    }
    let page = page("aria-widget", WIDGET);
    let theme = page.opened.ref_for("Light");
    let refused = page.tabref.run(&["select", &theme, "Dark"]);
    assert!(!refused.ok, "{}", refused.stdout);
    assert!(
        refused
            .stderr
            .contains("not a <select> or an ARIA combobox")
            && refused.stderr.contains("click it, then click the option"),
        "{}",
        refused.stderr
    );
    assert_eq!(
        js(&page, "document.getElementById('theme').textContent"),
        "Light"
    );
}
