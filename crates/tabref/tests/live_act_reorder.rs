//! Live runs of an action that reorders a table: the page draws the rows
//! again in a new order, and the action must print the rows it now starts
//! with instead of a cell-by-cell diff and a list of re-created cells.
//! Opt-in: `TABREF_LIVE=1 cargo test -p tabref --test live_act_reorder -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::Write;
use std::process::{Command, Stdio};

use common::{Harness, Run, harness, live, serve};

const TABLE: &str = r#"<!doctype html><title>Parcels</title><h1>Parcels</h1>
<table><thead><tr><th>Code</th><th><button id=sort>Weight</button></th></tr></thead><tbody id=rows></tbody></table>
<p>Page 1 of 1</p>
<script>
const parcels = [['P-101', 12.5], ['P-102', 44.0], ['P-103', 3.25], ['P-104', 27.75], ['P-105', 9.0], ['P-106', 61.5], ['P-107', 18.0], ['P-108', 33.5]];
let descending = false;
function draw() {
  const body = document.getElementById('rows');
  body.innerHTML = '';
  for (const [code, kg] of parcels) {
    const row = document.createElement('tr');
    row.innerHTML = '<td><a></a></td><td></td>';
    row.firstChild.firstChild.textContent = code;
    row.firstChild.firstChild.href = '/parcel/' + code;
    row.lastChild.textContent = kg + ' kg';
    body.appendChild(row);
  }
}
document.getElementById('sort').onclick = () => {
  descending = !descending;
  parcels.sort((a, b) => descending ? b[1] - a[1] : a[1] - b[1]);
  draw();
};
draw();
</script>"#;

fn batch(tabref: &Harness, steps: &str) -> Run {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tabref"))
        .args(["--cdp", &tabref.browser.port.to_string(), "do"])
        .env("TABREF_HOME", &tabref.home)
        .env("TABREF_REFS", "long")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(steps.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    Run {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn site() -> (Harness, String) {
    let port = serve(&[("/parcels", TABLE)]);
    (
        harness("reorder"),
        format!("http://127.0.0.1:{port}/parcels"),
    )
}

#[test]
fn sorting_a_table_prints_the_rows_it_now_starts_with() {
    if !live() {
        return;
    }
    let (tabref, url) = site();
    tabref.ok(&["open", &url]);
    let run = tabref.ok(&["click", "Weight"]);
    let out = run.stdout;
    assert!(out.contains("reordered 8 rows, now starting:"), "{out}");
    let first = out.find("P-106 61.5 kg").expect(&out);
    let second = out.find("P-102 44 kg").expect(&out);
    assert!(first < second, "{out}");
    assert!(!out.contains("new "), "{out}");
    assert!(!out.contains(" -> "), "{out}");
    assert!(out.len() < 700, "{} bytes:\n{out}", out.len());
}

#[test]
fn a_sort_inside_a_batch_names_the_new_first_rows_on_its_line() {
    if !live() {
        return;
    }
    let (tabref, url) = site();
    tabref.ok(&["open", &url]);
    let run = batch(&tabref, "click Weight\nclick Weight\n");
    let mut lines = run.stdout.lines();
    let step = lines.next().unwrap();
    assert!(
        step.contains("; reordered 8 rows, now starting: P-106 61.5 kg"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout
            .contains("reordered 8 rows, now starting:\n  P-103 3.25 kg"),
        "{}",
        run.stdout
    );
}
