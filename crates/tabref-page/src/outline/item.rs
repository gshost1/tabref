use super::block::Block;
use super::control::Control;

/// One piece of a block's content in reading order: a run of text with
/// its edge spaces kept, a control, a line break, or a nested block.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Text(String),
    Control(Control),
    Break,
    Block(Block),
}
