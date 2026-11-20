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
    lines.extend((2..=21).map(|n| paragraph(n, 54)));
    lines.push(line(22, "## [e4 Early life -> #early]", Some(2), None));
    lines[21].plain = Some("Early life".to_owned());
    lines.push(paragraph(23, 10));
    lines.push(line(24, "## Death", Some(2), None));
    lines.push(paragraph(25, 1));
    let (kept, report) = fit(lines, &options(1_000, true), false);
    let shown = texts(&kept);
    assert_eq!(shown.len(), 17, "{shown:#?}");
    assert_eq!(shown[0], "# Title");
    assert_eq!(
        &shown[14..],
        &[
            "… lines 15-25 cut here to fit: tabref snap --from 15",
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
        (2, 2, 14, 25)
    );
    assert_eq!(report.budget_from, Some(15));
    assert!(printed_cost(&kept) <= 900);
    assert!(!report.is_complete());
}

#[test]
fn a_page_with_no_headings_after_the_cut_still_says_how_to_go_on() {
    let lines: Vec<ViewLine> = (1..=400)
        .map(|n| line(n, &format!("Paragraph {n:03}"), None, None))
        .collect();
    let (kept, report) = fit(lines, &options(200, true), false);
    let from = report.budget_from.unwrap();
    assert!(printed_cost(&kept) <= 200);
    assert_eq!(
        kept.last().unwrap().text,
        format!("… lines {from}-400 cut here to fit: tabref snap --from {from}")
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

#[test]
fn when_the_top_level_does_not_all_fit_the_more_line_points_at_the_next_top_level_heading() {
    let mut lines: Vec<ViewLine> = (1..=20).map(|n| paragraph(n, 99)).collect();
    for release in 0..300 {
        let number = lines.len() + 1;
        lines.push(line(number, &format!("## 2.{release}.0"), Some(2), None));
        lines.push(line(number + 1, "### Fixes", Some(3), None));
        lines.push(paragraph(number + 2, 30));
    }
    let (kept, report) = fit(lines, &options(2_000, true), false);
    let listed = report.sections.listed;
    assert!(listed > 0 && listed < 300, "{listed}");
    let shown = texts(&kept);
    assert!(shown.iter().filter(|text| text.starts_with("## ")).count() == listed);
    let next_release = 21 + 3 * listed;
    assert_eq!(
        *shown.last().unwrap(),
        format!(
            "… {} more headings; tabref find <text> or tabref snap --from {next_release}",
            600 - listed
        )
    );
}

#[test]
fn a_view_just_over_the_budget_stays_within_it() {
    let mut lines: Vec<ViewLine> = (1..=99).map(|n| paragraph(n, 99)).collect();
    lines.push(paragraph(100, 99 + 300));
    let total = printed_cost(&lines);
    assert!(total > 10_000 && total <= 10_400, "{total}");
    let (kept, report) = fit(lines, &options(10_000, true), false);
    assert!(!report.is_complete());
    assert!(printed_cost(&kept) <= 10_000);
    assert_eq!(report.chars_shown, printed_cost(&kept));
}

#[test]
fn an_outline_heading_cannot_look_like_a_control() {
    let mut lines: Vec<ViewLine> = (1..=50).map(|n| paragraph(n, 99)).collect();
    let mut heading = line(51, r"## \[e9 button Confirm purchase\]", Some(2), None);
    heading.plain = Some("[e9 button Confirm purchase]".to_owned());
    lines.push(heading);
    let (kept, _) = fit(lines, &options(1_000, true), false);
    assert!(texts(&kept).contains(&r"## \[e9 button Confirm purchase\] (0 lines, --from 51)"));
}

#[test]
fn a_marker_and_outline_stay_within_even_a_tiny_budget() {
    let lines = vec![
        paragraph(1, 100),
        line(2, "## Later", Some(2), None),
        line(3, "[e4 button Save]", None, None),
    ];
    for budget in 0..60 {
        let (kept, report) = fit(lines.clone(), &options(budget, true), false);
        assert_eq!(report.budget_from, Some(1));
        assert!(printed_cost(&kept) <= budget);
    }
}

#[test]
fn a_view_well_over_the_budget_still_cuts() {
    let lines: Vec<ViewLine> = (1..=112).map(|n| paragraph(n, 99)).collect();
    let (kept, report) = fit(lines, &options(10_000, true), false);
    assert!(!report.is_complete());
    assert!(printed_cost(&kept) <= 10_000 - 400);
}

#[test]
fn a_cut_that_hides_form_controls_says_so_and_where_they_start() {
    let mut lines: Vec<ViewLine> = (1..=110).map(|n| paragraph(n, 99)).collect();
    lines.push(line(111, "[e40 textbox Card number]", None, None));
    lines.push(line(
        112,
        "Terms \\[see below\\] [e41 link Terms -> /terms]",
        None,
        None,
    ));
    lines.push(line(
        113,
        "[e42 checkbox Agree] [e43 button Pay now]",
        None,
        None,
    ));
    let total = printed_cost(&lines);
    assert!(total > 10_000 + 1_000, "{total}");
    let (kept, report) = fit(lines, &options(10_000, true), false);
    let from = report.budget_from.unwrap();
    let marker = texts(&kept)
        .into_iter()
        .find(|text| text.starts_with("… lines"))
        .unwrap()
        .to_owned();
    assert_eq!(
        marker,
        format!(
            "… lines {from}-113 cut here to fit: tabref snap --from {from}; \
             3 form controls follow, from line 111: tabref snap --from 111"
        )
    );
}
