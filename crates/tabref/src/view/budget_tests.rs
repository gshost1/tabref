use super::fit;
use crate::view::row_window::RowWindow;
use crate::view::view_line::ViewLine;
use crate::view::view_options::ViewOptions;

fn line(number: usize, text: &str, heading: Option<u8>, chrome: Option<usize>) -> ViewLine {
    let mut line = ViewLine::new(0, text);
    line.heading = heading;
    line.chrome = chrome;
    line.number = number;
    line
}

fn options(budget: usize, whole_chrome: bool) -> ViewOptions {
    ViewOptions {
        budget,
        whole_chrome,
        ..ViewOptions::snap("http://site.test/", false, RowWindow::default())
    }
}

#[test]
fn past_the_budget_a_marker_names_the_line_to_go_on_from_and_headings_keep_their_size() {
    let lines = vec![
        line(1, "# Title", Some(1), None),
        line(2, &"a".repeat(40), None, None),
        line(3, "## Early life", Some(2), None),
        line(4, &"b".repeat(40), None, None),
        line(5, &"c".repeat(40), None, None),
        line(6, "## Death", Some(2), None),
        line(7, "d", None, None),
    ];
    let (kept, report) = fit(lines, &options(60, true), false);
    let texts: Vec<&str> = kept.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "# Title",
            &"a".repeat(40),
            "… lines 3-7 cut here to fit: tabref snap --from 3",
            "## Early life [not shown: 2 lines, --from 3]",
            "## Death [not shown: 1 lines, --from 6]",
        ]
    );
    assert_eq!(
        (
            report.sections.count,
            report.lines_shown,
            report.lines_total
        ),
        (2, 2, 7)
    );
    assert_eq!(report.sections.first_heading.as_deref(), Some("Early life"));
    assert_eq!(report.budget_from, Some(3));
    assert!(!report.is_complete());
}

#[test]
fn a_page_with_no_headings_after_the_cut_still_says_how_to_go_on() {
    let lines: Vec<ViewLine> = (1..=400)
        .map(|n| line(n, &format!("Paragraph {n:03}"), None, None))
        .collect();
    let (kept, report) = fit(lines, &options(200, true), false);
    assert_eq!(report.budget_from, Some(15));
    assert_eq!(
        kept.last().unwrap().text,
        "… lines 15-400 cut here to fit: tabref snap --from 15"
    );
}

#[test]
fn chrome_keeps_its_first_lines_when_the_page_has_main_content() {
    let mut lines: Vec<ViewLine> = (1..=9)
        .map(|n| line(n, &format!("link {n}"), None, Some(1)))
        .collect();
    lines.push(line(10, "content", None, None));
    let (kept, report) = fit(lines.clone(), &options(10_000, false), true);
    assert_eq!(kept.len(), 8);
    assert_eq!(kept[6].text, "… 3 more lines here: tabref snap --all");
    assert_eq!(report.chrome_cut, 3);
    let (whole, report) = fit(lines, &options(10_000, true), true);
    assert_eq!((whole.len(), report.is_complete()), (10, true));
}
