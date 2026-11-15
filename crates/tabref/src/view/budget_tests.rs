use super::fit;
use crate::view::view_line::ViewLine;

fn line(text: &str, heading: Option<u8>, chrome: Option<usize>) -> ViewLine {
    ViewLine {
        depth: 0,
        text: text.into(),
        heading,
        chrome,
    }
}

#[test]
fn past_the_budget_only_headings_print_with_their_size() {
    let lines = vec![
        line("# Title", Some(1), None),
        line(&"a".repeat(40), None, None),
        line("## Early life", Some(2), None),
        line(&"b".repeat(40), None, None),
        line(&"c".repeat(40), None, None),
        line("## Death", Some(2), None),
        line("d", None, None),
    ];
    let (kept, report) = fit(lines, 60, true, false);
    let texts: Vec<&str> = kept.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(
        texts,
        vec![
            "# Title",
            &"a".repeat(40),
            "## Early life [not shown: 2 lines]",
            "## Death [not shown: 1 lines]"
        ]
    );
    assert_eq!(
        (report.sections_cut, report.lines_shown, report.lines_total),
        (2, 2, 7)
    );
    assert_eq!(report.first_cut_heading.as_deref(), Some("Early life"));
    assert!(!report.is_complete());
}

#[test]
fn chrome_keeps_its_first_lines_when_the_page_has_main_content() {
    let mut lines: Vec<ViewLine> = (0..9)
        .map(|n| line(&format!("link {n}"), None, Some(1)))
        .collect();
    lines.push(line("content", None, None));
    let (kept, report) = fit(lines.clone(), 10_000, false, true);
    assert_eq!(kept.len(), 8);
    assert_eq!(kept[6].text, "… 3 more lines here: tabref snap --all");
    assert_eq!(report.chrome_cut, 3);
    let (whole, report) = fit(lines, 10_000, true, true);
    assert_eq!((whole.len(), report.is_complete()), (10, true));
}
