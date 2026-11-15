use tabref_page::{Block, BlockKind, Item};

/// Whether the outline holds a `main` landmark anywhere.
pub(crate) fn has_main(body: &Block) -> bool {
    is_main(body) || body.blocks().any(has_main)
}

/// The page's main content: of its `main` landmarks the one with the most
/// text, since a page may mark an ad slot as `main` too; with none, the
/// `<article>`-like region with the most text; else the whole body.
pub(crate) fn main_block(body: &Block) -> &Block {
    let mut found = Vec::new();
    collect(body, &mut found);
    found
        .into_iter()
        .max_by_key(|block| text_size(block))
        .filter(|block| text_size(block) > 0)
        .unwrap_or(body)
}

fn is_main(block: &Block) -> bool {
    block.kind == BlockKind::Landmark("main".to_owned())
}

fn collect<'a>(block: &'a Block, found: &mut Vec<&'a Block>) {
    for inner in block.blocks() {
        if is_main(inner) {
            found.push(inner);
            continue;
        }
        collect(inner, found);
    }
}

/// The characters of text and control names inside `block`.
pub(crate) fn text_size(block: &Block) -> usize {
    block
        .items
        .iter()
        .map(|item| match item {
            Item::Text(run) => run.trim().chars().count(),
            Item::Control(control) => control.name.chars().count(),
            Item::Break => 0,
            Item::Block(inner) => text_size(inner),
        })
        .sum()
}
