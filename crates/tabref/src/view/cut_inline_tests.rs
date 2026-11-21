use super::budget::fit;
use super::cut_outline::cut_outline;
use super::row_window::RowWindow;
use super::view_line::ViewLine;
use super::view_options::ViewOptions;

fn lines(text: &str) -> Vec<ViewLine> {
    text.lines()
        .enumerate()
        .map(|(at, text)| {
            let mut line = ViewLine::new(0, text);
            line.number = at + 1;
            let level = text.chars().take_while(|&ch| ch == '#').count();
            line.heading = (level > 0).then_some(level as u8);
            line
        })
        .collect()
}

fn outline(text: &str, room: usize) -> String {
    let options = ViewOptions::snap("http://fixture.test", false, RowWindow::default());
    let result = cut_outline(&lines(text), 0, room, &options.route, false);
    assert!(result.cost() <= room);
    result
        .lines
        .iter()
        .map(|line| format!("{}\n", line.printed()))
        .collect()
}

#[test]
fn short_bodies_inline_but_empty_long_and_truncated_bodies_do_not() {
    let text = format!(
        "## Dates\nReleased: Jan 24, 2017\n## Pair\none\ntwo\n## Empty\n## Long\n{}\n## Many\na\nb\nc",
        "x".repeat(161)
    );
    let shown = outline(&text, 2_000);
    assert!(
        shown.contains("## Dates\nReleased: Jan 24, 2017\n"),
        "{shown}"
    );
    assert!(shown.contains("## Pair\none\ntwo\n"), "{shown}");
    assert!(shown.contains("## Empty (0 lines"));
    assert!(shown.contains("## Long (1 lines"));
    assert!(shown.contains("## Many (3 lines"));
    assert_eq!(outline("", 100), "");
    for room in 0..300 {
        outline(&text, room);
    }
}

#[test]
fn inline_selection_prefers_shallow_then_near_and_caps_total_bodies() {
    let body = "x".repeat(100);
    let text = format!("### Deep\n{body}\n## Near\n{body}\n## Far\n{body}\n## Last\n{body}");
    let shown = outline(&text, 410);
    assert!(shown.contains(&format!("## Near\n{body}")), "{shown}");
    assert!(shown.contains(&format!("## Far\n{body}")), "{shown}");
    assert!(shown.contains("### Deep (1 lines"), "{shown}");
    assert!(shown.contains("## Last (1 lines"), "{shown}");
}

#[test]
fn truncated_and_multiline_bodies_stay_stubs() {
    let options = ViewOptions::snap("http://fixture.test", false, RowWindow::default());
    let mut source = lines("## Truncated\npartial\n## Pre\none");
    source[1].cut = Some("+100 chars".to_owned());
    source[3].text = "one\ntwo\nthree".to_owned();
    let result = cut_outline(&source, 0, 1_000, &options.route, false);
    assert_eq!(
        result
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        [
            "## Truncated (1 lines, --from 1)",
            "## Pre (1 lines, --from 3)"
        ]
    );
}

#[test]
fn to_boundary_bodies_stay_stubs() {
    let mut options = ViewOptions::snap("http://fixture.test", false, RowWindow::default());
    options.route.to = Some(664);
    let mut source = lines("## Key dates\nReleased: Jan 24, 2017");
    source[0].number = 663;
    source[1].number = 664;
    let result = cut_outline(&source, 0, 1_000, &options.route, false);
    assert_eq!(result.lines[0].text, "## Key dates (1 lines, --from 663)");
    options.route.to = Some(700);
    let result = cut_outline(&source, 0, 1_000, &options.route, false);
    assert_eq!(result.lines[0].text, "## Key dates\nReleased: Jan 24, 2017");
}

#[test]
fn live_refs_and_controls_stay_stubs() {
    for body in [
        "[e12 link Download files]",
        "[@session:e12 Download -> /files]",
        "[button Save]",
        "[Download -> /files]",
    ] {
        assert_eq!(
            outline(&format!("## Links\n{body}"), 1_000),
            "## Links (1 lines, --from 1)\n"
        );
    }
    assert_eq!(
        outline("## Text\n\\[ordinary text\\]", 1_000),
        "## Text\n\\[ordinary text\\]\n"
    );
}

#[test]
fn every_stub_only_heading_is_preserved_when_inlining() {
    let source = fixture("PyPI");
    let mut baseline = source.clone();
    for line in &mut baseline {
        line.cut = Some("disable inline bodies".to_owned());
    }
    for budget in (500..=12_000).step_by(37) {
        let options = ViewOptions {
            budget,
            ..ViewOptions::snap("http://fixture.test", false, RowWindow::default())
        };
        let (main, _) = fit(baseline.clone(), &options, false);
        let (branch, _) = fit(source.clone(), &options, false);
        let main: Vec<_> = main
            .iter()
            .filter(|line| line.heading.is_some())
            .map(|line| line.number)
            .collect();
        let branch: Vec<_> = branch
            .iter()
            .filter(|line| line.heading.is_some())
            .map(|line| line.number)
            .collect();
        assert_eq!(main, branch, "budget {budget}");
    }
}

fn fixture(kind: &str) -> Vec<ViewLine> {
    let mut text = format!("# {kind}\n");
    for n in 0..80 {
        text.push_str(&format!(
            "{kind} paragraph {n}: {}\n",
            "Detailed reference material. ".repeat(5)
        ));
    }
    let sections = match kind {
        "PyPI" => vec![
            ("Key dates", "Released: Jan 24, 2017"),
            ("Meta", "Version: 1.2.3\nRequires: Python >=3.9"),
            (
                "Classifiers",
                "Development Status :: 5 - Production/Stable\nProgramming Language :: Python\nTopic :: Software Development",
            ),
            (
                "Maintainers",
                "[e12 link Example maintainer -> /user/example]",
            ),
            (
                "Project links",
                "[@session:e13 Homepage -> https://example.test]",
            ),
            ("Navigation", "[e14 link Release history -> #history]"),
            ("License", "MIT"),
            ("Installation", "pip install sample"),
            ("Requirements", "Python >=3.9"),
            (
                "Statistics",
                "This project has many releases, downloads, contributors and dependent projects. The statistics sidebar contains a detailed explanation of how download counts are collected and filtered.",
            ),
            (
                "Download files",
                "[e15 link sample-1.2.3.tar.gz -> /files/sample.tar.gz]",
            ),
            (
                "Description",
                "This example project provides a reference implementation for processing structured input documents and rendering useful output. See the complete documentation for installation options, supported platforms, and examples.",
            ),
            ("Footer", "Published by the Example team"),
        ],
        "RFC" => vec![
            ("Status", "Proposed Standard"),
            (
                "Security considerations",
                "Implementations MUST validate inputs.",
            ),
            ("Authors", "A. Example"),
        ],
        _ => vec![
            ("Born", "24 January 1917"),
            ("Occupation", "Researcher"),
            ("References", "Collected works, 1980"),
        ],
    };
    for (heading, body) in sections {
        text.push_str(&format!("## {heading}\n{body}\n"));
    }
    lines(&text)
}

#[test]
fn fixture_output_measurements() {
    let mut pypi = String::new();
    for kind in ["PyPI", "RFC", "Wikipedia"] {
        let options = ViewOptions {
            budget: if kind == "PyPI" { 4_000 } else { 2_000 },
            ..ViewOptions::snap("http://fixture.test", false, RowWindow::default())
        };
        let source = fixture(kind);
        let mut baseline = source.clone();
        for line in &mut baseline {
            line.cut = Some("disable inline bodies".to_owned());
        }
        let (main, _) = fit(baseline, &options, false);
        let before: String = main
            .iter()
            .map(|line| format!("{}\n", line.printed()))
            .collect();
        if kind == "PyPI" {
            assert_eq!(
                &before[before.find("## Key dates").unwrap()..],
                include_str!("cut_inline_pypi_main.golden")
            );
        }
        let (kept, report) = fit(source, &options, false);
        let output: String = kept
            .iter()
            .map(|line| format!("{}\n", line.printed()))
            .collect();
        println!(
            "{kind}: main {} bytes, branch {} bytes\n{output}",
            before.len(),
            output.len()
        );
        assert!(report.budget_from.is_some());
        assert!(kept.iter().map(ViewLine::cost).sum::<usize>() <= options.budget);
        if kind == "PyPI" {
            pypi = output;
        }
    }
    let outline = &pypi[pypi.find("## Key dates").unwrap()..];
    assert_eq!(outline, include_str!("cut_inline_pypi.golden"));
}
