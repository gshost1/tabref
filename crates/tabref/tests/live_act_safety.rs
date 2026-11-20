//! Live regressions for actions that must fail rather than reach the wrong
//! element: a scope that names a row without the control, a sold-out card
//! beside one in stock, a short ref from another tab, a navigation that
//! starts after its click settled, and loop values that look like
//! placeholders. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_act_safety -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};

const SHOP: &str = r#"<!doctype html><title>Shop</title>
<table><tr><th>Name</th><th>Stock</th><th>Action</th></tr>
<tr><td>Row 1</td><td>0</td><td>none</td></tr>
<tr><td>Row 10</td><td>3</td><td><button onclick="document.getElementById('out').textContent='edited 10'">Edit</button> <a href="/done">Preview</a></td></tr>
<tr><td>Row 10</td><td>8</td><td><button onclick="document.getElementById('out').textContent='edited the other row'">Edit</button></td></tr></table>
<section><article><h3>Widget A</h3><button onclick="document.getElementById('out').textContent='A added'">Add</button></article>
<article><h3>Widget B</h3><p>Sold out</p></article>
<article><h3>Widget C</h3><button onclick="document.getElementById('out').textContent='C added'">Add</button></article></section>
<button onclick="setTimeout(() => { location.href = '/done'; }, 30)">Go later</button>
<a href="/danger" target="_blank">Open danger</a>
<p id=out>Nothing</p>"#;

const DANGER: &str = r#"<!doctype html><title>Danger</title><button>Delete everything</button>"#;
const DONE: &str = "<!doctype html><title>Done</title><h1>Thank you</h1>";
const TABLE: &str = r#"<!doctype html><title>Stock</title>
<table><tr><th>Product</th><th>Variant</th><th>Price</th><th>Action</th></tr>
<tr><td>Mug</td><td></td><td>9.50</td><td><button onclick="out.textContent='edited Mug'">Edit</button></td></tr>
<tr><td>Cup</td><td>Mug</td><td>9.50</td><td><button onclick="out.textContent='edited Cup'">Edit</button></td></tr>
<tr><td>Plate</td><td>12.50 <button onclick="out.textContent='edited Plate'">Edit</button></td></tr>
</table><p id=out>Nothing</p>"#;

const ROW_MARKUP: &str = r#"<!doctype html><title>Stock</title>
<table><tr><th>Product</th><th>Variant</th><th>Price</th><th>Action</th></tr>
<tr><td><input type=checkbox checked aria-label="Select Mug"></td><td>Mug</td><td><button disabled>Unavailable</button></td>
    <td><button onclick="out.textContent='edited checked'">Edit</button></td></tr>
<tr><td>Fields</td><td><input aria-label="Code" value="A ] | [ B"></td><td><select aria-label="Size"><option>S</option><option>M</option></select></td>
    <td><button onclick="out.textContent='edited fields'">Edit</button></td></tr>
<tr><td>Long</td><td><button aria-label="Long name for a row button that includes characters past the renderer limit and should match the complete accessibility name">Info</button></td>
    <td><button onclick="out.textContent='edited long'">Edit</button></td></tr>
<tr><td>Mug | 9.50</td><td><button onclick="out.textContent='edited one cell'">Edit</button></td></tr>
<tr><td>Mug</td><td>9.50</td><td><button onclick="out.textContent='edited two cells'">Edit</button></td></tr>
</table><p id=out>Nothing</p>"#;

fn site() -> (Harness, String) {
    let port = serve(&[("/shop", SHOP), ("/danger", DANGER), ("/done", DONE)]);
    (harness("act-safety"), format!("http://127.0.0.1:{port}"))
}

#[test]
fn a_scope_whose_record_lacks_the_control_fails_instead_of_using_a_neighbour() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/shop")]);
    let row = tabref.run(&["click", r#""Edit" in "Row 1""#]);
    assert!(
        row.stderr.starts_with("error ELEMENT_NOT_FOUND"),
        "{}",
        row.stderr
    );
    let card = tabref.run(&["click", r#""Add" in "Widget B""#]);
    assert!(
        card.stderr.starts_with("error ELEMENT_NOT_FOUND"),
        "{}",
        card.stderr
    );
    let untouched = tabref.ok(&["js", "document.getElementById('out').textContent"]);
    assert_eq!(untouched.stdout.trim(), "Nothing");
    let added = tabref.ok(&["click", r#""Add" in "Widget C""#]);
    assert!(
        added.stdout.starts_with(r#"clicked "Add" in "Widget C""#),
        "{}",
        added.stdout
    );
    assert!(added.stdout.contains("C added"), "{}", added.stdout);
}

#[test]
fn a_short_ref_from_another_tab_names_that_tab_instead_of_acting_here() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    let opened = tabref.ok(&["open", &format!("{base}/shop")]);
    let link = opened.ref_for("Open danger");
    let short = format!("e{}", link.rsplit(":e").next().unwrap());
    let popped = tabref.ok(&["click", &short]);
    assert!(
        popped.stdout.contains("opened a new tab"),
        "{}",
        popped.stdout
    );
    let again = tabref.run(&["click", &short]);
    assert!(
        again.stderr.starts_with("error STALE_REF"),
        "{}",
        again.stderr
    );
    assert!(again.stderr.contains("belongs to tab"), "{}", again.stderr);
}

#[test]
fn a_do_step_can_use_a_row_copied_from_snap() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/shop")]);
    let snap = tabref.ok(&["snap"]);
    let row = snap
        .stdout
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("Row 10 | 3 | ["))
        .unwrap_or_else(|| panic!("{}", snap.stdout));
    let batch = format!(r#"click "Edit" in "{row}""#);
    let edited = tabref.ok(&["do", &batch]);
    assert!(edited.stdout.contains("edited 10"), "{}", edited.stdout);
    let missing = tabref.run(&["do", r#"click "Edit" in "Row 10 | 0""#]);
    assert!(
        missing.stderr.starts_with("error ELEMENT_NOT_FOUND"),
        "{}",
        missing.stderr
    );
}

#[test]
fn copied_rows_skip_empty_cells_and_match_text_alongside_controls() {
    if !live() {
        return;
    }
    let port = serve(&[("/stock", TABLE)]);
    let tabref = harness("row-scope");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/stock")]);
    for (prefix, expected) in [
        ("Mug | 9.50 | [", "edited Mug"),
        ("Plate | 12.50 [", "edited Plate"),
    ] {
        let snap = tabref.ok(&["snap"]);
        let row = snap
            .stdout
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with(prefix))
            .unwrap_or_else(|| panic!("{}", snap.stdout));
        let batch = format!(r#"click "Edit" in "{row}""#);
        tabref.ok(&["do", &batch]);
        let edited = tabref.ok(&["js", "document.getElementById('out').textContent"]);
        assert_eq!(edited.stdout.trim(), expected);
    }
}

#[test]
fn copied_rows_keep_control_names_when_the_tokens_include_details() {
    if !live() {
        return;
    }
    let tabref = open_row_markup();
    for (fragment, expected) in [
        ("checkbox Select Mug (checked)", "edited checked"),
        ("options: S | M", "edited fields"),
        ("Long name for a row button", "edited long"),
    ] {
        let row = printed_row(&tabref, |line| line.contains(fragment));
        click_copied_row(&tabref, &row, expected);
    }
}

#[test]
fn copying_rows_distinguishes_a_literal_pipe_from_a_cell_separator() {
    if !live() {
        return;
    }
    let tabref = open_row_markup();
    let literal = printed_row(&tabref, |line| line.starts_with(r"Mug \| 9.50 | "));
    let separator = printed_row(&tabref, |line| line.starts_with("Mug | 9.50 | "));
    click_copied_row(&tabref, &literal, "edited one cell");
    click_copied_row(&tabref, &separator, "edited two cells");
}

fn open_row_markup() -> common::Harness {
    let port = serve(&[("/stock", ROW_MARKUP)]);
    let tabref = harness("row-markup");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/stock")]);
    tabref
}

fn printed_row(tabref: &common::Harness, matches: impl Fn(&str) -> bool) -> String {
    let snap = tabref.ok(&["snap"]);
    snap.stdout
        .lines()
        .map(str::trim)
        .find(|line| matches(line))
        .unwrap_or_else(|| panic!("{}", snap.stdout))
        .to_owned()
}

fn click_copied_row(tabref: &common::Harness, row: &str, expected: &str) {
    let batch = format!(r#"click "Edit" in '{}'"#, row);
    tabref.ok(&["do", &batch]);
    let result = tabref.ok(&["js", "document.getElementById('out').textContent"]);
    assert_eq!(result.stdout.trim(), expected);
}

#[test]
fn a_navigation_that_starts_after_its_click_settled_is_followed_before_the_next_step() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/shop")]);
    let run = tabref.ok(&["do", "click \"Go later\"; find \"Thank you\""]);
    assert!(
        run.stdout.contains("(after the step settled)"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("(1 matches)"), "{}", run.stdout);
}
