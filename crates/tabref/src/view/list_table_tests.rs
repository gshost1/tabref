use tabref_page::{Block, BlockKind, Item};

use super::BlockRenderer;
use crate::view::ref_style::RefStyle;
use crate::view::row_window::RowWindow;
use crate::view::view_options::ViewOptions;

fn cell(kind: BlockKind, text: &str) -> Item {
    Item::Block(Block {
        kind,
        items: vec![Item::Text(text.into())],
        key: None,
        label: None,
    })
}

fn row(kind: BlockKind, cells: &[&str]) -> Item {
    Item::Block(Block {
        kind: BlockKind::Row,
        items: cells.iter().map(|text| cell(kind.clone(), text)).collect(),
        key: Some("TR".into()),
        label: None,
    })
}

fn peaks(ranks: &[&str]) -> Block {
    let mut rows = vec![
        row(BlockKind::HeaderCell, &["Rank", "Mountain", "Height"]),
        row(BlockKind::HeaderCell, &["", "", "m"]),
    ];
    for rank in ranks {
        rows.push(row(
            BlockKind::Cell,
            &[rank, &format!("Peak {rank}"), "7,400"],
        ));
    }
    Block {
        kind: BlockKind::Generic,
        items: vec![Item::Block(Block {
            kind: BlockKind::Table,
            items: rows,
            key: None,
            label: None,
        })],
        key: None,
        label: None,
    }
}

fn render(root: &Block, rows: RowWindow) -> Vec<String> {
    let options = ViewOptions {
        refs: RefStyle::Short,
        ..ViewOptions::snap("http://site.test/", false, rows)
    };
    let mut renderer = BlockRenderer::new(&options);
    renderer.render_children(root, 0);
    renderer.lines.iter().map(|line| line.printed()).collect()
}

fn ranked(count: usize) -> Vec<String> {
    (1..=count).map(|rank| rank.to_string()).collect()
}

#[test]
fn a_rows_window_counts_data_rows_below_a_two_row_header_and_labels_them() {
    let ranks = ranked(70);
    let ranks: Vec<&str> = ranks.iter().map(String::as_str).collect();
    let lines = render(&peaks(&ranks), RowWindow::parse("62-64").unwrap());
    assert_eq!(
        lines[0], "table 70 rows: Rank | Mountain | Height",
        "{lines:#?}"
    );
    let shown: Vec<&String> = lines.iter().filter(|line| line.contains("Peak")).collect();
    assert_eq!(
        shown,
        [
            "  #62 62 | Peak 62 | 7,400",
            "  #63 63 | Peak 63 | 7,400",
            "  #64 64 | Peak 64 | 7,400",
        ],
        "{lines:#?}"
    );
    assert!(
        lines.last().unwrap().contains("#N is the Nth data row"),
        "{lines:#?}"
    );
}

#[test]
fn a_rows_window_label_shows_the_position_when_unranked_rows_shift_the_ranks() {
    let mut ranks = ranked(70);
    ranks.insert(10, "S".into());
    let ranks: Vec<&str> = ranks.iter().map(String::as_str).collect();
    let lines = render(&peaks(&ranks), RowWindow::parse("62-64").unwrap());
    assert!(
        lines.contains(&"  #62 61 | Peak 61 | 7,400".to_owned()),
        "{lines:#?}"
    );
}

#[test]
fn the_default_window_keeps_rows_unlabelled_and_the_second_header_row_unnumbered() {
    let ranks = ranked(25);
    let ranks: Vec<&str> = ranks.iter().map(String::as_str).collect();
    let lines = render(&peaks(&ranks), RowWindow::default());
    assert_eq!(lines[0], "table 25 rows: Rank | Mountain | Height");
    assert_eq!(lines[1], "   |  | m");
    assert_eq!(lines[2], "  1 | Peak 1 | 7,400");
    assert_eq!(lines[21], "  20 | Peak 20 | 7,400");
    assert_eq!(
        lines[22],
        "  … 5 more rows (25 in all): tabref snap --rows 21-"
    );
}
