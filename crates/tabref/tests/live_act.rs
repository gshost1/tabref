//! Live runs of the action side against headless Chrome and local fixture
//! pages: `do` batches, targets by name, what an action reports it changed,
//! natural values, waits and errors. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_act -- --test-threads=1`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::Write;
use std::process::{Command, Stdio};

use common::{Harness, Run, harness, live, serve};

const LIST: &str = r#"<!doctype html><title>Products</title><h1>Products</h1>
<table><tr><th>Name</th><th>Price</th><th>Action</th></tr>
<tr><td>Hoodie-XS-Blue</td><td>$67.00</td><td><a href="/edit?id=1">Edit</a></td></tr>
<tr><td>Hoodie-S-Blue</td><td>$67.00</td><td><a href="/edit?id=2">Edit</a></td></tr>
<tr><td>Hoodie-M-Blue</td><td>$67.00</td><td><a href="/edit?id=3">Edit</a></td></tr>
</table>"#;

const EDIT: &str = r#"<!doctype html><title>Edit product</title>
<form action="/list"><label>Product Name <input name=name value="Hoodie"></label>
<label>Price ($) <input name=price value="67.00"></label>
<input type=hidden name=saved value=1><button>Save</button></form>"#;

const WIDGETS: &str = r#"<!doctype html><title>Widgets</title>
<table><tr><th>Votes</th><th>Title</th></tr><tr><td><button title="Upvote" onclick="const s=this.nextElementSibling; s.textContent=String(Number(s.textContent)+1)">▲</button><span>3</span></td><td>Books set in winter</td></tr>
<tr><td><button title="Upvote" onclick="const s=this.nextElementSibling; s.textContent=String(Number(s.textContent)+1)">▲</button><span>9</span></td><td>Long series</td></tr></table>
<button>Noop</button>
<button onclick="document.getElementById('modal').hidden=false">New issue</button>
<div id=modal role=dialog aria-label="New issue" hidden><label>Title <input id=title></label>
<button onclick="document.getElementById('out').textContent='Created '+document.getElementById('title').value; document.getElementById('modal').hidden=true">Create</button></div>
<p id=out>Nothing yet</p>
<label>Due date <input type=date></label>
<label>Amount <input type=number></label>
<label>Assignee <select><option value="">Unassigned</option><option value=rdorsey>Rosa Dorsey (@rdorsey)</option><option value=ruribe>Rafael Uribe (@ruribe)</option></select></label>
<a href="/done">Finish</a>
<a href="/spa/a" id=spa onclick="event.preventDefault(); history.pushState({}, '', '/spa/b'); document.getElementById('out').textContent='Route b'">Go to b</a>"#;

const DONE: &str = "<!doctype html><title>Done</title><h1>Thank you</h1>";

fn site() -> (Harness, String) {
    let port = serve(&[
        ("/list", LIST),
        ("/edit", EDIT),
        ("/widgets", WIDGETS),
        ("/done", DONE),
    ]);
    (harness("act"), format!("http://127.0.0.1:{port}"))
}

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

#[test]
fn a_grid_edit_runs_in_one_batch_on_pages_never_snapshotted() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/list")]);
    let run = batch(
        &tabref,
        "click \"Edit\" in \"Hoodie-S-Blue\"\nfill \"Price\" 62.00\nclick \"Save\"\nexpect url ~saved=1\n",
    );
    assert!(run.ok, "{}\n{}", run.stdout, run.stderr);
    let lines: Vec<&str> = run.stdout.lines().collect();
    assert!(lines[0].starts_with("ok click \"Edit\""), "{}", run.stdout);
    assert!(lines[0].contains("-> /edit?id=2"), "{}", run.stdout);
    assert_eq!(lines[1], r#"ok fill "Price ($)" "67.00" -> "62.00""#);
    assert!(
        lines[2].starts_with("ok click \"Save\" -> /list?"),
        "{}",
        run.stdout
    );
    assert!(lines[2].contains("price=62.00"), "{}", run.stdout);
    assert!(
        lines[3].starts_with("ok expect url ~saved=1"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_click_reports_the_counter_it_moved_and_a_noop_reports_nothing() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/widgets")]);
    let voted = tabref.ok(&["click", r#""Upvote" in "Books set in winter""#]);
    assert!(voted.stdout.contains("{3 -> 4}"), "{}", voted.stdout);
    assert!(!voted.stdout.contains("{9 ->"), "{}", voted.stdout);
    let noop = tabref.ok(&["click", "Noop"]);
    assert_eq!(noop.stdout.trim(), r#"clicked "Noop" (no visible change)"#);
}

#[test]
fn a_modal_shows_its_new_controls_and_a_batch_fills_it() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/widgets")]);
    let opened = tabref.ok(&["click", "New issue"]);
    assert!(
        opened
            .stdout
            .lines()
            .any(|line| line.starts_with("new @s") && line.contains("textfield \"Title\"")),
        "{}",
        opened.stdout
    );
    let filled = tabref.ok(&["do", "fill Title \"401 bad gateway\"; click Create"]);
    assert!(
        filled.stdout.contains("Created 401 bad gateway"),
        "{}",
        filled.stdout
    );
}

#[test]
fn long_fills_preview_both_ends_and_keep_full_json_values() {
    if !live() {
        return;
    }
    let previous = format!("old start {} old end", "x".repeat(5_500));
    let requested = format!("new start {} new end", "é".repeat(5_500));
    let page = format!(
        "<!doctype html><meta charset=utf-8><label>Notes <textarea>{previous}</textarea></label>\
         <label>Limited <textarea maxlength=4>seed</textarea></label>"
    );
    let port = serve(&[("/", &page)]);
    let tabref = harness("fill-preview");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);

    let filled = tabref.ok(&["fill", "Notes", &requested]);
    for fragment in ["old start", "old end", "new start", "new end", "…"] {
        assert!(filled.stdout.contains(fragment), "{}", filled.stdout);
    }
    let length = format!("({} chars)", requested.chars().count());
    assert_eq!(
        filled.stdout.matches(&length).count(),
        2,
        "{}",
        filled.stdout
    );
    assert!(filled.stdout.len() < 400, "{}", filled.stdout);

    let batch = tabref.ok(&["do", "fill Notes short"]);
    assert!(batch.stdout.contains(&length), "{}", batch.stdout);
    assert!(batch.stdout.contains(r#"-> "short""#), "{}", batch.stdout);

    let json = tabref.ok(&["--json", "fill", "Notes", &requested]).json();
    assert_eq!(json["data"]["previous"], "short");
    assert_eq!(json["data"]["observed"], requested);
    assert_eq!(json["data"]["verified"], true);

    let limited = tabref.ok(&["fill", "Limited", &requested]);
    assert!(
        limited.stdout.contains(r#"(asked for "new start"#)
            && limited.stdout.contains(&length)
            && limited.stdout.len() < 250,
        "{}",
        limited.stdout
    );
}

#[test]
fn dates_numbers_and_options_take_natural_values_and_read_back() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/widgets")]);
    let run = batch(
        &tabref,
        "fill \"Due date\" 2027-7-31\nfill Amount 62.00\nselect Assignee \"rosa dorsey\"\n",
    );
    assert!(run.ok, "{}\n{}", run.stdout, run.stderr);
    assert!(
        run.stdout.contains(r#""" -> "2027-07-31""#),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains(r#""" -> "62.00""#), "{}", run.stdout);
    assert!(
        run.stdout
            .contains(r#""Unassigned" -> "Rosa Dorsey (@rdorsey)" (value "rdorsey")"#),
        "{}",
        run.stdout
    );
}

#[test]
fn a_route_change_and_a_navigation_report_where_they_led() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/widgets")]);
    let routed = tabref.ok(&["click", "Go to b"]);
    assert!(routed.stdout.contains("-> /spa/b"), "{}", routed.stdout);
    assert!(routed.stdout.contains("Route b"), "{}", routed.stdout);
    let finished = tabref.ok(&["click", "Finish"]);
    assert!(
        finished.stdout.contains("page \"Done\""),
        "{}",
        finished.stdout
    );
    assert!(finished.stdout.contains("Thank you"));
}

#[test]
fn a_missing_or_ambiguous_target_fails_with_candidates_and_stops_the_batch() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/list")]);
    let missing = tabref.run(&["click", "Edt"]);
    assert!(!missing.ok);
    assert!(
        missing.stderr.starts_with("error ELEMENT_NOT_FOUND"),
        "{}",
        missing.stderr
    );
    assert!(
        missing.stderr.contains(r#"link "Edit""#),
        "{}",
        missing.stderr
    );
    let ambiguous = tabref.run(&["click", "Edit"]);
    assert!(
        ambiguous.stderr.starts_with("error AMBIGUOUS_TARGET"),
        "{}",
        ambiguous.stderr
    );
    let stopped = batch(
        &tabref,
        "click \"Edit\" nth 1\nfill \"Pirce\" 1\nclick Save\n",
    );
    assert!(!stopped.ok);
    assert!(
        stopped.stderr.contains("\ndetail: ok click \"Edit\""),
        "{}",
        stopped.stderr
    );
    assert!(
        stopped.stderr.contains("step 2 `fill \"Pirce\" 1`"),
        "{}",
        stopped.stderr
    );
    assert!(
        stopped
            .stderr
            .contains("the 1 step(s) after it did not run")
    );
    assert!(
        stopped.stderr.contains("page \"Edit product\""),
        "{}",
        stopped.stderr
    );
}

#[test]
fn a_stale_ref_names_its_fresh_candidate() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    let opened = tabref.ok(&["open", &format!("{base}/widgets")]);
    let noop = opened.ref_for("button Noop");
    tabref.ok(&["js", "location.reload(); 1"]);
    tabref.ok(&["wait", "--text", "Nothing yet"]);
    let stale = tabref.run(&["click", &noop]);
    assert!(
        stale.stderr.starts_with("error STALE_REF"),
        "{}",
        stale.stderr
    );
    assert!(stale.stderr.contains("fresh: @s"), "{}", stale.stderr);
}

#[test]
fn short_refs_keep_their_numbers_and_go_stale_with_their_document() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    let opened = tabref.ok(&["open", &format!("{base}/widgets")]);
    let long = opened.ref_for("button Noop");
    let short = long.rsplit(':').next().unwrap().to_owned();
    let again = tabref.ok(&["snap"]);
    assert_eq!(
        again.ref_for("button Noop").rsplit(':').next(),
        Some(short.as_str())
    );
    let clicked = tabref.ok(&["click", &short]);
    assert!(
        clicked.stdout.starts_with("clicked @s"),
        "{}",
        clicked.stdout
    );
    tabref.ok(&["js", "location.reload(); 1"]);
    tabref.ok(&["wait", "--text", "Nothing yet"]);
    tabref.ok(&["snap"]);
    let stale = tabref.run(&["click", &short]);
    assert!(
        stale.stderr.starts_with("error STALE_REF"),
        "{}",
        stale.stderr
    );
}
