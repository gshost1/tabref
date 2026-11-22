use tabref_page::{Block, BlockKind, Item};

use super::fit;
use crate::view::block_renderer::BlockRenderer;
use crate::view::cut_report::CutReport;
use crate::view::ref_style::RefStyle;
use crate::view::row_window::RowWindow;
use crate::view::view_line::ViewLine;
use crate::view::view_options::ViewOptions;

fn block(kind: BlockKind, items: Vec<Item>) -> Item {
    Item::Block(Block {
        kind,
        items,
        key: None,
        label: None,
    })
}

fn text(kind: BlockKind, text: &str) -> Item {
    block(kind, vec![Item::Text(text.into())])
}

fn row(kind: BlockKind, cells: &[String]) -> Item {
    block(
        BlockKind::Row,
        cells.iter().map(|cell| text(kind.clone(), cell)).collect(),
    )
}

fn paragraphs(numbers: std::ops::RangeInclusive<usize>) -> Vec<Item> {
    numbers
        .map(|n| text(BlockKind::Paragraph, &format!("{n} {}", "word ".repeat(40))))
        .collect()
}

fn table(name: &str, count: usize, notes: &str) -> Item {
    let mut rows = vec![row(
        BlockKind::HeaderCell,
        &["Rank".into(), "Mountain".into(), "Notes".into()],
    )];
    for rank in 1..=count {
        rows.push(row(
            BlockKind::Cell,
            &[rank.to_string(), format!("{name} {rank}"), notes.to_owned()],
        ));
    }
    block(BlockKind::Table, rows)
}

fn page(items: Vec<Item>) -> Block {
    Block {
        kind: BlockKind::Generic,
        items,
        key: None,
        label: None,
    }
}

fn long_cells() -> String {
    format!("first climbed {}", "by a long party ".repeat(17))
}

fn long_page() -> Block {
    let mut items = paragraphs(1..=40);
    items.push(table("Peak", 70, &long_cells()));
    items.extend(paragraphs(41..=60));
    page(items)
}

fn many_tables_page() -> Block {
    let mut items = paragraphs(1..=70);
    items.extend((1..=40).map(|n| table(&format!("Hill{n}"), 25, "short")));
    items.push(table("Peak", 80, "short"));
    page(items)
}

struct Fitted {
    lines: Vec<ViewLine>,
    kept: Vec<ViewLine>,
    report: CutReport,
}

fn fitted(root: &Block, rows: RowWindow, budget: usize) -> Fitted {
    let options = ViewOptions {
        refs: RefStyle::Short,
        budget,
        ..ViewOptions::snap("http://site.test/", false, rows)
    };
    let mut renderer = BlockRenderer::new(&options);
    renderer.render_children(root, 0);
    let mut lines = renderer.lines;
    for (position, line) in lines.iter_mut().enumerate() {
        line.number = position + 1;
    }
    let (kept, report) = fit(lines.clone(), &options, false);
    Fitted {
        lines,
        kept,
        report,
    }
}

fn cost(lines: &[ViewLine]) -> usize {
    lines.iter().map(ViewLine::cost).sum()
}

fn within_frame_room(lines: &[ViewLine], budget: usize) {
    let room = budget - 400.min(budget / 10);
    assert!(cost(lines) <= room, "{} chars of {room}", cost(lines));
}

fn printed(lines: &[ViewLine]) -> Vec<String> {
    lines.iter().map(ViewLine::printed).collect()
}

fn shows_rows(lines: &[ViewLine], name: &str, ranks: std::ops::RangeInclusive<usize>) {
    let printed = printed(lines);
    for rank in ranks {
        let wanted = format!("{rank} | {name} {rank} |");
        assert!(
            printed.iter().any(|line| line.contains(&wanted)),
            "{wanted} missing: {printed:#?}"
        );
    }
}

#[test]
fn a_three_row_window_of_long_cells_past_the_cut_prints_all_three_rows() {
    let view = fitted(&long_page(), RowWindow::parse("62-64").unwrap(), 3_000);
    assert!(cost(&view.lines) > 3_000);
    shows_rows(&view.kept, "Peak", 62..=64);
    let from = view.report.budget_from.unwrap();
    assert!(from < 42, "cut at {from}");
    assert_eq!(view.report.lines_shown, from - 1 + 5);
    within_frame_room(&view.kept, 3_000);
}

#[test]
fn kept_rows_past_a_cut_are_cut_to_a_short_line_that_says_how_to_see_the_rest() {
    let view = fitted(&long_page(), RowWindow::parse("62-64").unwrap(), 3_000);
    let rows: Vec<&ViewLine> = view
        .kept
        .iter()
        .filter(|line| line.text.contains("| Peak 6"))
        .collect();
    assert_eq!(rows.len(), 3, "{rows:#?}");
    for line in rows {
        let (short, pointer) = line.text.split_once(" …(").unwrap();
        assert!(
            short.starts_with('#') && short.chars().count() <= 300,
            "{line:#?}"
        );
        let from = format!(" chars: tabref snap --full --from {})", line.number);
        assert!(
            pointer.starts_with('+') && pointer.ends_with(&from),
            "{line:#?}"
        );
        assert!(
            line.cut
                .as_deref()
                .is_some_and(|cut| cut.ends_with(" chars"))
        );
    }
}

#[test]
fn a_window_row_in_a_view_that_fits_keeps_the_usual_long_line() {
    let mut items = paragraphs(1..=2);
    items.push(table("Peak", 30, &long_cells()));
    let view = fitted(&page(items), RowWindow::parse("2-3").unwrap(), 12_000);
    assert!(view.report.is_complete());
    let row = view
        .kept
        .iter()
        .find(|line| line.text.contains("| Peak 2 |"))
        .unwrap();
    assert_eq!(
        row.text.chars().count(),
        long_cells().trim_end().chars().count() + 16
    );
    assert_eq!(row.cut, None);
}

#[test]
fn tables_the_window_picks_no_rows_of_keep_nothing_past_the_cut() {
    let root = many_tables_page();
    let view = fitted(&root, RowWindow::parse("62-64").unwrap(), 12_000);
    let mut unkept = view.lines.clone();
    for line in &mut unkept {
        line.keep = false;
    }
    let window = RowWindow::parse("62-64").unwrap();
    let (_, main) = fit(
        unkept,
        &ViewOptions::snap("http://site.test/", false, window),
        false,
    );
    shows_rows(&view.kept, "Peak", 62..=64);
    let from = view.report.budget_from.unwrap();
    let main_from = main.budget_from.unwrap();
    assert!(
        from + 2 >= main_from,
        "cut at {from}, main cut at {main_from}"
    );
    assert_eq!(view.report.lines_shown, from - 1 + 5);
    assert_eq!(view.lines.iter().filter(|line| line.keep).count(), 5);
    within_frame_room(&view.kept, 12_000);
}

#[test]
fn a_small_budget_keeps_only_the_rows_that_fit_and_stays_within_it() {
    let view = fitted(&long_page(), RowWindow::parse("62-64").unwrap(), 700);
    within_frame_room(&view.kept, 700);
}

#[test]
fn a_window_of_more_than_ten_rows_is_not_kept_past_the_cut() {
    let view = fitted(&long_page(), RowWindow::parse("55-65").unwrap(), 3_000);
    assert!(view.lines.iter().all(|line| !line.keep));
    assert!(!view.kept.iter().any(|line| line.text.contains("| Peak 6")));
}
