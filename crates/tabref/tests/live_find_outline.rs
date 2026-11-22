#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

#[test]
fn chart_find_combines_dom_text_with_accessibility_only_names() {
    if !common::live() {
        return;
    }
    let tabref = common::harness("find-chart");
    let html = r#"<!doctype html><title>Dashboard</title>
<p>Electricity use (kWh) by month</p>
<svg role="img" aria-labelledby="units" width="200" height="100">
  <rect width="100" height="50" aria-label="March 2024: 45,371 kWh" role="img"><title>March 2024: 45,371 kWh</title></rect>
</svg>
<span id="units" hidden>Monthly totals in kWh</span>
<button aria-description="Yearly total in kWh">Totals</button>
"#;
    let port = common::serve(&[("/", html)]);
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let found = tabref.ok(&["find", "kWh"]).stdout;
    assert!(found.contains("Electricity use (kWh) by month"), "{found}");
    assert_eq!(
        found.matches("March 2024: 45,371 kWh").count(),
        1,
        "{found}"
    );
    assert_eq!(found.matches("Yearly total in kWh").count(), 1, "{found}");
}

#[test]
fn long_pre_find_stays_within_the_match_and_snapshot_budgets() {
    if !common::live() {
        return;
    }
    let tabref = common::harness("find-long-pre");
    let rows = (1..=500)
        .map(|line| format!("SIGWINCH {line:03} {}\n", "x".repeat(600)))
        .collect::<String>();
    let html = format!("<!doctype html><title>Signals</title><h1>Signals</h1><pre>{rows}</pre>");
    let port = common::serve(&[("/", &html)]);
    let opened = tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    assert!(opened.stdout.chars().count() <= 12_000);
    let found = tabref.ok(&["find", "SIGWINCH"]).stdout;
    assert!(found.starts_with("4 matches for "), "{found}");
    assert_eq!(found.lines().count(), 5);
    assert!(found.contains("… 497 more in this block"), "{found}");
    assert!(found.chars().count() < 4_000);
}
