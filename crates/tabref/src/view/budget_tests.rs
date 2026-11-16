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

fn paragraph(number: usize, chars: usize) -> ViewLine {
    line(number, &"p".repeat(chars), None, None)
}

fn texts(kept: &[ViewLine]) -> Vec<&str> {
    kept.iter().map(|line| line.text.as_str()).collect()
}

fn printed_cost(kept: &[ViewLine]) -> usize {
    kept.iter().map(ViewLine::cost).sum()
}

#[test]
fn past_the_budget_a_marker_names_the_line_to_go_on_from_and_plain_headings_follow() {
    let mut lines = vec![line(1, "# Title", Some(1), None)];
    lines.extend((2..=21).map(|n| paragraph(n, 49)));
    lines.push(line(22, "## [e4 Early life -> #early]", Some(2), None));
    lines[21].plain = Some("Early life".to_owned());
    lines.push(paragraph(23, 10));
    lines.push(line(24, "## Death", Some(2), None));
    lines.push(paragraph(25, 1));
    let (kept, report) = fit(lines, &options(1_000, true), false);
    let shown = texts(&kept);
    assert_eq!(shown.len(), 18, "{shown:#?}");
    assert_eq!(shown[0], "# Title");
    assert_eq!(
        &shown[15..],
        &[
            "… lines 16-25 cut here to fit: tabref snap --from 16",
            "## Early life (1 lines, --from 22)",
            "## Death (1 lines, --from 24)",
        ]
    );
    assert_eq!(
        (
            report.sections.count,
            report.sections.listed,
            report.lines_shown,
            report.lines_total
        ),
        (2, 2, 15, 25)
    );
    assert_eq!(report.sections.first_heading.as_deref(), Some("Early life"));
    assert_eq!(report.budget_from, Some(16));
    assert!(printed_cost(&kept) <= 900);
    assert!(!report.is_complete());
}

#[test]
fn a_page_with_no_headings_after_the_cut_still_says_how_to_go_on() {
    let lines: Vec<ViewLine> = (1..=400)
        .map(|n| line(n, &format!("Paragraph {n:03}"), None, None))
        .collect();
    let (kept, report) = fit(lines, &options(200, true), false);
    assert_eq!(report.budget_from, Some(12));
    assert_eq!(
        kept.last().unwrap().text,
        "… lines 12-400 cut here to fit: tabref snap --from 12"
    );
}

#[test]
fn deeper_headings_that_do_not_all_fit_leave_every_top_level_one_listed_and_fill_the_rest() {
    let mut lines: Vec<ViewLine> = (1..=30).map(|n| paragraph(n, 99)).collect();
    for part in 1..=5 {
        let number = lines.len() + 1;
        lines.push(line(number, &format!("## Part {part}"), Some(2), None));
        for sub in 1..=4 {
            let number = lines.len() + 1;
            lines.push(line(
                number,
                &format!("### Sub {part}.{sub}"),
                Some(3),
                None,
            ));
            lines.push(paragraph(number + 1, 9));
        }
    }
    let (kept, report) = fit(lines, &options(2_000, true), false);
    let shown = texts(&kept);
    let outline: Vec<&str> = shown
        .iter()
        .skip_while(|text| !text.starts_with("… lines"))
        .skip(1)
        .copied()
        .collect();
    assert_eq!(
        outline,
        vec![
            "## Part 1 (8 lines, --from 31)",
            "### Sub 1.1 (1 lines, --from 32)",
            "### Sub 1.2 (1 lines, --from 34)",
            "### Sub 1.3 (1 lines, --from 36)",
            "### Sub 1.4 (1 lines, --from 38)",
            "## Part 2 (8 lines, --from 40)",
            "### Sub 2.1 (1 lines, --from 41)",
            "## Part 3 (8 lines, --from 49)",
            "## Part 4 (8 lines, --from 58)",
            "## Part 5 (8 lines, --from 67)",
            "… 15 more headings; tabref find <text> or tabref snap --from 43",
        ]
    );
    assert_eq!((report.sections.count, report.sections.listed), (25, 10));
    assert!(printed_cost(&kept) <= 1_800, "{}", printed_cost(&kept));
}

#[test]
fn too_many_top_level_headings_list_the_first_ones_in_order() {
    let mut lines: Vec<ViewLine> = (1..=20).map(|n| paragraph(n, 99)).collect();
    for release in 0..200 {
        let number = lines.len() + 1;
        lines.push(line(number, &format!("### 2.{release}.0"), Some(3), None));
        lines.push(paragraph(number + 1, 30));
    }
    let (kept, report) = fit(lines, &options(2_000, true), false);
    let shown = texts(&kept);
    let at = shown
        .iter()
        .position(|text| text.starts_with("… lines"))
        .unwrap();
    assert_eq!(shown[at + 1], "### 2.0.0 (1 lines, --from 21)");
    let listed = report.sections.listed;
    assert!(listed > 0 && listed < 200, "{listed}");
    assert_eq!(report.sections.count, 200);
    let left = 21 + 2 * listed;
    assert_eq!(
        *shown.last().unwrap(),
        format!(
            "… {} more headings; tabref find <text> or tabref snap --from {left}",
            200 - listed
        )
    );
    assert!(printed_cost(&kept) <= 1_800, "{}", printed_cost(&kept));
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
