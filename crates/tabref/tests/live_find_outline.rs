#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

#[test]
fn do_find_applies_the_five_row_budget_to_weighted_results() {
    if !common::live() {
        return;
    }
    let tabref = common::harness("find-weighted-do");
    let prose: String = (1..=7).map(|i| format!("<p>needle {i}</p>")).collect();
    let linked: String = (1..=6)
        .map(|i| format!("{i}. See <a href='/open.2'>open(2)</a> for details.\n"))
        .collect();
    let linked = format!("<pre>{linked}</pre>");
    let long = format!(
        "<p>first let</p><p>second let</p><h2>Example</h2><pre>{}</pre><h2>Summary</h2><p>after let</p>",
        "let value = 1;\n".repeat(100)
    );
    let port = common::serve(&[("/prose", &prose), ("/linked", &linked), ("/long", &long)]);
    let open = |path| {
        tabref.ok(&["open", &format!("http://127.0.0.1:{port}/{path}")]);
    };
    let step = |query: &str| tabref.ok(&["do", &format!("find '{query}'")]).stdout;
    let step_json = |query: &str| {
        tabref
            .ok(&["--json", "do", &format!("find '{query}'")])
            .json()["data"]["steps"][0]["data"]
            .clone()
    };
    open("prose");
    let output = step("needle");
    assert!(
        output.contains("ok find \"needle\" (7 matches)"),
        "{output}"
    );
    assert!(
        output.contains("  +2 more: tabref find \"needle\""),
        "{output}"
    );
    let prose = step_json("needle");
    assert_eq!(prose["count"], 7);
    let rows = prose["matches"].as_array().unwrap();
    for (i, row) in rows[..5].iter().enumerate() {
        assert_eq!(row.as_str().unwrap(), format!("- needle {}", i + 1));
    }
    assert_eq!(rows.len(), 6);
    assert!(rows[5].as_str().unwrap().starts_with("… 2 more;"));

    open("linked");
    let output = step("open(2)");
    assert!(
        output.contains("ok find \"open(2)\" (6 matches)"),
        "{output}"
    );
    assert_eq!(
        output
            .lines()
            .filter(|line| line.starts_with("  - "))
            .count(),
        4,
        "{output}"
    );
    assert!(output.contains("… 3 more in this block (@s"), "{output}");
    assert!(!output.contains("more: tabref find"), "{output}");
    let linked = step_json("open(2)");
    assert_eq!(linked["count"], 6);
    assert_eq!(linked["matches"].as_array().unwrap().len(), 4);

    open("long");
    let output = step("let");
    let rows: Vec<_> = output
        .lines()
        .filter(|line| line.starts_with("  - "))
        .collect();
    assert_eq!(rows.len(), 5, "{output}");
    assert_eq!(rows[4], "  - Example | let value = 1;");
    assert!(output.contains("ok find \"let\" (103 matches)"), "{output}");
    assert!(
        output.contains("  +98 more: tabref find \"let\""),
        "{output}"
    );
    let long = step_json("let");
    assert_eq!(long["count"], 103);
    assert!(
        long["matches"][5]
            .as_str()
            .unwrap()
            .starts_with("… 98 more;")
    );
}

#[test]
fn inline_pre_links_count_as_lines_without_an_extra_overflow_footer() {
    if !common::live() {
        return;
    }
    let tabref = common::harness("find-pre-links");
    let rows: String = (1..=6)
        .map(|line| format!("{line}. See <a href='/open.2'>open(2)</a> for details.\n"))
        .collect();
    let html = format!("<!doctype html><title>Manual</title><pre>{rows}</pre>");
    let port = common::serve(&[("/", &html)]);
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let stdout = tabref.ok(&["find", "open(2)"]).stdout;
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 5, "{stdout}");
    assert_eq!(lines[0], "6 matches for \"open(2)\"");
    for (line, result) in lines[1..4].iter().enumerate() {
        assert!(
            result.contains(&format!("{}. See open(2) for details.", line + 1)),
            "{stdout}"
        );
        assert!(result.contains("@s"), "{stdout}");
    }
    assert!(lines[4].contains("… 3 more in this block"), "{stdout}");
}

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
    assert!(found.starts_with("500 matches for "), "{found}");
    assert_eq!(found.lines().count(), 5);
    assert!(found.contains("… 497 more in this block"), "{found}");
    assert!(found.chars().count() < 4_000);
}
