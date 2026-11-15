use tabref_page::{Block, BlockKind, Item, squash};

use super::escape::escape_text;
use super::node_line::quote_text;

/// Kinds that never fold into the line of the block around them.
pub(crate) fn folds_never(kind: &BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Heading(_)
            | BlockKind::Table
            | BlockKind::Landmark(_)
            | BlockKind::Dialog
            | BlockKind::Frame
            | BlockKind::Pre
    )
}

pub(crate) fn prefix(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::ListItem => "- ",
        BlockKind::Quote => "> ",
        _ => "",
    }
}

pub(crate) fn prefix_in_line(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::Quote => "> ",
        _ => "",
    }
}

/// How a landmark, dialog or frame is titled: `nav`, `nav "Primary"`,
/// `dialog "Cookies"`, `frame example.com`. A label is quoted on one line.
pub(crate) fn title_of(block: &Block) -> String {
    let base = match &block.kind {
        BlockKind::Landmark(name) => name.clone(),
        BlockKind::Dialog => "dialog".to_owned(),
        BlockKind::Frame => "frame".to_owned(),
        _ => String::new(),
    };
    match (&block.kind, block.label.as_deref()) {
        (BlockKind::Frame, Some(origin)) => format!("{base} {}", escape_text(origin)),
        (_, Some(label)) => format!("{base} {}", quote_text(&squash(label))),
        (_, None) => base,
    }
}

/// The block's text with controls as their names and no markup, on one
/// line: how a heading is matched by `read --section`.
pub(crate) fn plain_text(block: &Block) -> String {
    let mut out = String::new();
    gather_plain(block, &mut out);
    squash(&out)
}

fn gather_plain(block: &Block, out: &mut String) {
    for item in &block.items {
        match item {
            Item::Text(run) => out.push_str(run),
            Item::Break => out.push(' '),
            Item::Control(control) => {
                out.push(' ');
                out.push_str(&control.name);
                out.push(' ');
            }
            Item::Block(inner) => {
                out.push(' ');
                gather_plain(inner, out);
                out.push(' ');
            }
        }
    }
}
