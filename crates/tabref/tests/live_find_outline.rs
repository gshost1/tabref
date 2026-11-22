#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

#[test]
#[cfg(unix)]
fn pre_printed_pointers_reach_late_examples_and_huge_man_pages() {
    if !common::live() {
        return;
    }
    let tabref = common::harness("find-pre-pointers");
    let examples = (0..30)
        .map(|number| {
            format!(
                "<h2>§ Examples</h2><pre>let mut vec = Vec::new();\nvec.push({number});\n</pre>"
            )
        })
        .collect::<String>();
    let man_target = "SIGTARGET user's $HOME `uname` ; final line";
    let man = format!(
        "<h2>DESCRIPTION top</h2><pre>{}\n{man_target}\n</pre>",
        "ordinary signal prose\n".repeat(2_000)
    );
    let port = common::serve(&[("/vec", &examples), ("/man", &man)]);
    for (path, needle, target) in [
        ("vec", "push(11)", "vec.push(11);"),
        ("man", "SIGTARGET", man_target),
    ] {
        tabref.ok(&["open", &format!("http://127.0.0.1:{port}/{path}")]);
        let found = tabref.ok(&["find", needle]).stdout;
        let line = found
            .lines()
            .find(|line| line.contains(target) && line.contains(" (tabref "))
            .expect(&found);
        let pointer = line
            .rsplit_once(" (tabref ")
            .unwrap()
            .1
            .strip_suffix(')')
            .unwrap();
        let output = std::process::Command::new("sh")
            .args(["-c", &format!("tabref() {{ \"$TABREF_TEST_BIN\" --cdp \"$TABREF_TEST_PORT\" --timeout 30000 \"$@\"; }}; tabref {pointer}")])
            .env("TABREF_TEST_BIN", env!("CARGO_BIN_EXE_tabref"))
            .env("TABREF_TEST_PORT", tabref.browser.port.to_string())
            .env("TABREF_HOME", &tabref.home)
            .env("TABREF_REFS", "long")
            .output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "tabref {pointer}: {stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.lines().skip(1).any(|line| line.contains(target)),
            "tabref {pointer}: {stdout}"
        );
    }
}

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
