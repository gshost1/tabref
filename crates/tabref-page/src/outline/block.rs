use super::block_kind::BlockKind;
use super::item::Item;

/// One block of the page outline: what it is, its content in reading
/// order, the key that marks siblings of the same shape (tag and stable
/// classes), and its label (a landmark's or dialog's accessible name, a
/// frame's origin, a hidden block's expander).
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub items: Vec<Item>,
    pub key: Option<String>,
    pub label: Option<String>,
}

impl Block {
    pub fn new(kind: BlockKind) -> Self {
        Self {
            kind,
            items: Vec::new(),
            key: None,
            label: None,
        }
    }

    /// The nested blocks, in order.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.items.iter().filter_map(|item| match item {
            Item::Block(block) => Some(block),
            _ => None,
        })
    }

    /// Whether any content is left: text that is not blank, a control, or a
    /// nested block that is not empty.
    pub fn is_empty(&self) -> bool {
        self.items.iter().all(|item| match item {
            Item::Text(text) => text.trim().is_empty(),
            Item::Break => true,
            Item::Control(_) => false,
            Item::Block(block) => block.is_empty(),
        })
    }
}
