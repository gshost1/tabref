use tabref_page::{Block, BlockKind, Control, Item};

use super::BlockRenderer;
use crate::view::ref_style::RefStyle;
use crate::view::row_window::RowWindow;
use crate::view::view_options::ViewOptions;

fn options() -> ViewOptions {
    ViewOptions {
        refs: RefStyle::Short,
        ..ViewOptions::snap("http://site.test/", false, RowWindow::default())
    }
}

fn text(run: &str) -> Item {
    Item::Text(run.into())
}

fn link(number: u32, name: &str, href: &str) -> Item {
    Item::Control(Control {
        ref_text: Some(format!("@s1:e{number}")),
        role: "link".into(),
        name: name.into(),
        href: Some(format!("http://site.test{href}")),
        ..Control::default()
    })
}

fn block(kind: BlockKind, key: Option<&str>, items: Vec<Item>) -> Block {
    Block {
        kind,
        items,
        key: key.map(str::to_owned),
        label: None,
    }
}

fn render(root: &Block) -> Vec<String> {
    let options = options();
    let mut renderer = BlockRenderer::new(&options);
    renderer.render_children(root, 0);
    renderer.lines.iter().map(|line| line.printed()).collect()
}

fn sense(definition: &str, examples: &[&str]) -> Item {
    let mut items = vec![Item::Block(block(
        BlockKind::Generic,
        None,
        vec![text(definition)],
    ))];
    for example in examples {
        items.push(Item::Block(block(
            BlockKind::Generic,
            Some("DIV.ex"),
            vec![text(example)],
        )));
    }
    Item::Block(block(BlockKind::Generic, Some("DIV.sense"), items))
}

#[test]
fn repeated_blocks_are_numbered_so_a_reader_can_count_them() {
    let root = block(
        BlockKind::Generic,
        None,
        vec![
            sense(
                "A gleam of light is a small, bright light that shines briefly:",
                &["the gleam of a lighthouse", "a faint gleam"],
            ),
            sense(
                "a brief sign of a quality, seen in someone's eyes:",
                &["a hint of mischief"],
            ),
        ],
    );
    assert_eq!(
        render(&root),
        vec![
            "(1/2) A gleam of light is a small, bright light that shines briefly: · the gleam of a lighthouse · a faint gleam",
            "(2/2) a brief sign of a quality, seen in someone's eyes: · a hint of mischief",
        ]
    );
}

#[test]
fn a_long_table_prints_its_header_once_a_window_of_rows_and_how_to_see_more() {
    let header = block(
        BlockKind::Row,
        Some("TR"),
        ["ID", "Name"]
            .iter()
            .map(|cell| Item::Block(block(BlockKind::HeaderCell, None, vec![text(cell)])))
            .collect(),
    );
    let mut rows = vec![Item::Block(header)];
    for row in 1..=25 {
        rows.push(Item::Block(block(
            BlockKind::Row,
            Some("TR"),
            vec![
                Item::Block(block(BlockKind::Cell, None, vec![text(&row.to_string())])),
                Item::Block(block(
                    BlockKind::Cell,
                    None,
                    vec![link(row, "Edit", &format!("/item/{row}/edit"))],
                )),
            ],
        )));
    }
    let root = block(
        BlockKind::Generic,
        None,
        vec![Item::Block(block(BlockKind::Table, None, rows))],
    );
    let lines = render(&root);
    assert_eq!(lines[0], "table 25 rows: ID | Name");
    assert_eq!(lines[1], "  1 | [e1 Edit -> /item/1/edit]");
    assert_eq!(lines.len(), 22);
    assert_eq!(
        lines[21],
        "  … 5 more rows (25 in all): tabref snap --rows 21-"
    );
}

#[test]
fn prose_reads_short_links_as_words_but_keeps_titles_and_menus_as_links() {
    let paragraph = block(
        BlockKind::Paragraph,
        None,
        vec![
            text("He worked at "),
            link(1, "Bletchley Park", "/wiki/Bletchley"),
            text(" breaking German naval codes in the war."),
        ],
    );
    let menu = block(
        BlockKind::List,
        None,
        vec![
            Item::Block(block(
                BlockKind::ListItem,
                Some("LI"),
                vec![link(2, "Shop", "/shop/")],
            )),
            Item::Block(block(
                BlockKind::ListItem,
                Some("LI"),
                vec![link(3, "Forum", "/forum/")],
            )),
        ],
    );
    let root = block(
        BlockKind::Generic,
        None,
        vec![Item::Block(paragraph), Item::Block(menu)],
    );
    assert_eq!(
        render(&root),
        vec![
            "He worked at Bletchley Park breaking German naval codes in the war.",
            "[e2 Shop -> /shop/] · [e3 Forum -> /forum/]",
        ]
    );
}
