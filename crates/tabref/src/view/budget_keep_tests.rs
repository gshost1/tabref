use tabref_page::{Block, BlockKind, Item};

use super::fit;
use crate::view::block_renderer::BlockRenderer;
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

fn long_page() -> Block {
    let mut items: Vec<Item> = (1..=40)
        .map(|n| text(BlockKind::Paragraph, &format!("{n} {}", "word ".repeat(40))))
        .collect();
    let mut rows = vec![
        row(
            BlockKind::HeaderCell,
            &["Rank".into(), "Mountain".into(), "Notes".into()],
        ),
        row(
            BlockKind::HeaderCell,
            &[String::new(), String::new(), "m".into()],
        ),
    ];
    for rank in 1..=70 {
        rows.push(row(
            BlockKind::Cell,
            &[
                rank.to_string(),
                format!("Peak {rank}"),
                format!("first climbed {}", "by a long party ".repeat(17)),
            ],
        ));
    }
    items.push(block(BlockKind::Table, rows));
    items.extend(
        (41..=60).map(|n| text(BlockKind::Paragraph, &format!("{n} {}", "word ".repeat(40)))),
    );
    Block {
        kind: BlockKind::Generic,
        items,
        key: None,
        label: None,
    }
}

fn fitted(rows: RowWindow, budget: usize) -> (Vec<ViewLine>, Vec<ViewLine>) {
    let options = ViewOptions {
        refs: RefStyle::Short,
        budget,
        ..ViewOptions::snap("http://site.test/", false, rows)
    };
    let mut renderer = BlockRenderer::new(&options);
    renderer.render_children(&long_page(), 0);
    let mut lines = renderer.lines;
    for (position, line) in lines.iter_mut().enumerate() {
        line.number = position + 1;
    }
    let (kept, _) = fit(lines.clone(), &options, false);
    (lines, kept)
}

fn cost(lines: &[ViewLine]) -> usize {
    lines.iter().map(ViewLine::cost).sum()
}

#[test]
fn a_three_row_window_of_long_cells_past_the_cut_prints_all_three_rows() {
    let (lines, kept) = fitted(RowWindow::parse("62-64").unwrap(), 3_000);
    assert!(cost(&lines) > 3_000);
    let printed: Vec<String> = kept.iter().map(ViewLine::printed).collect();
    for rank in 62..=64 {
        assert!(
            printed
                .iter()
                .any(|line| line.contains(&format!("{rank} | Peak {rank} | first climbed"))),
            "row {rank} missing: {printed:#?}"
        );
    }
    assert!(cost(&kept) <= 3_000, "{} chars", cost(&kept));
}

#[test]
fn kept_rows_are_cut_to_a_short_line_with_the_rest_marked() {
    let (_, kept) = fitted(RowWindow::parse("62-64").unwrap(), 3_000);
    let rows: Vec<&ViewLine> = kept
        .iter()
        .filter(|line| line.text.contains("| Peak 6"))
        .collect();
    assert_eq!(rows.len(), 3, "{rows:#?}");
    for line in rows {
        assert!(line.keep && line.text.starts_with('#'), "{line:#?}");
        assert!(line.text.chars().count() <= 300, "{line:#?}");
        assert!(
            line.cut
                .as_deref()
                .is_some_and(|cut| cut.ends_with(" chars"))
        );
    }
}

#[test]
fn a_small_budget_keeps_only_the_rows_that_fit_and_stays_within_it() {
    let (_, kept) = fitted(RowWindow::parse("62-64").unwrap(), 700);
    assert!(cost(&kept) <= 700, "{} chars", cost(&kept));
}

#[test]
fn a_window_of_more_than_ten_rows_is_not_kept_past_the_cut() {
    let (lines, kept) = fitted(RowWindow::parse("55-65").unwrap(), 3_000);
    assert!(lines.iter().all(|line| !line.keep));
    assert!(!kept.iter().any(|line| line.text.contains("| Peak 6")));
}
