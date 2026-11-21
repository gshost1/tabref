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
    assert!(result.lines.iter().all(|line| line.text.contains("--from")));
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
            ("License", "MIT"),
            ("Installation", "pip install sample"),
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
            budget: 2_000,
            ..ViewOptions::snap("http://fixture.test", false, RowWindow::default())
        };
        let (kept, report) = fit(fixture(kind), &options, false);
        let output: String = kept
            .iter()
            .map(|line| format!("{}\n", line.printed()))
            .collect();
        println!("{kind}: {} bytes\n{output}", output.len());
        assert!(report.budget_from.is_some());
        assert!(kept.iter().map(ViewLine::cost).sum::<usize>() <= options.budget);
        if kind == "PyPI" {
            pypi = output;
        }
    }
    let outline = &pypi[pypi.find("## Key dates").unwrap()..];
    assert_eq!(outline, include_str!("cut_inline_pypi.golden"));
}
