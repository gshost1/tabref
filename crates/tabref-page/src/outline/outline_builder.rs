use std::collections::HashMap;

use super::block::Block;
use super::block_kind::BlockKind;
use super::control_build::control_of;
use super::dom_tree::DomTree;
use super::element_kind::{is_block_display, is_sectioning, is_skipped, semantic_kind};
use super::frame_stats::FrameStats;
use super::item::Item;
use super::node_kind::NodeKind;
use super::outline_input::OutlineInput;
use super::overlay::{dialog_label, is_dialog_candidate, keeps_as_dialog};
use super::repeat_key::repeat_key;
use super::shown_map::ShownMap;
use super::text_runs::collapse_run;
use super::walk_context::WalkContext;
use super::container::is_container;

const MAX_DEPTH: usize = 256;

/// Walks the captured documents into blocks. Every capture (the page's own
/// target first, then each out-of-process iframe) is a slice of documents;
/// a node is addressed by capture, document and node index.
pub(crate) struct OutlineBuilder<'a> {
    pub(crate) input: &'a OutlineInput,
    pub(crate) captures: Vec<&'a [DomTree]>,
    pub(crate) shown: ShownMap,
    pub(crate) dialogs: Vec<Block>,
    pub(crate) frames: FrameStats,
    pub(crate) lift_dialogs: bool,
    pub(crate) frame_captures: HashMap<(String, i64), usize>,
}

impl<'a> OutlineBuilder<'a> {
    pub(crate) fn new(input: &'a OutlineInput, lift_dialogs: bool) -> Self {
        let mut captures: Vec<&'a [DomTree]> = vec![input.trees.as_slice()];
        let mut owners: Vec<(&(String, i64), &'a Vec<DomTree>)> = input.frames.iter().collect();
        owners.sort_by(|left, right| left.0.cmp(right.0));
        let mut frame_captures = HashMap::new();
        for (owner, trees) in owners {
            frame_captures.insert(owner.clone(), captures.len());
            captures.push(trees.as_slice());
        }
        let shown = ShownMap::new(&captures);
        Self {
            input,
            captures,
            shown,
            dialogs: Vec::new(),
            frames: FrameStats::default(),
            lift_dialogs,
            frame_captures,
        }
    }

    pub(crate) fn tree(&self, context: &WalkContext) -> Option<&'a DomTree> {
        self.captures.get(context.capture)?.get(context.document)
    }

    /// The blocks for the children of `index`, built into `into`.
    pub(crate) fn visit_children(&mut self, context: &WalkContext, index: usize, into: &mut Block) {
        let Some(tree) = self.tree(context) else {
            return;
        };
        let Some(node) = tree.node(index) else {
            return;
        };
        for child in &node.children {
            self.visit(context, *child, into);
        }
    }

    pub(crate) fn visit(&mut self, context: &WalkContext, index: usize, into: &mut Block) {
        if context.depth > MAX_DEPTH {
            return;
        }
        let Some(tree) = self.tree(context) else {
            return;
        };
        let Some(node) = tree.node(index) else {
            return;
        };
        match &node.kind {
            NodeKind::Text(text) => {
                if node.is_shown() {
                    let run = if context.in_pre { text.clone() } else { collapse_run(text) };
                    push_text(into, &run);
                }
            }
            NodeKind::Document => self.visit_children(&context.deeper(), index, into),
            NodeKind::Element(tag) => {
                if is_skipped(node) || !self.shown.has_shown(context, index) {
                    return;
                }
                match tag.as_str() {
                    "BR" => into.items.push(Item::Break),
                    "IFRAME" | "FRAME" => self.frame(context, index, into),
                    _ => self.element(context, index, into),
                }
            }
            NodeKind::Other => {}
        }
    }

    fn element(&mut self, context: &WalkContext, index: usize, into: &mut Block) {
        let Some(tree) = self.tree(context) else {
            return;
        };
        let Some(node) = tree.node(index) else {
            return;
        };
        let key = (tree.target_id.clone(), node.backend_id);
        let label = self.input.refs.get(&key);
        if self.lift_dialogs && is_dialog_candidate(node, self.input.viewport) {
            let mut dialog = Block::new(BlockKind::Dialog);
            dialog.label = dialog_label(tree, index);
            self.visit_children(&context.deeper(), index, &mut dialog);
            if keeps_as_dialog(node, &dialog) {
                self.dialogs.push(dialog);
                return;
            }
            into.items.push(Item::Block(dialog));
            return;
        }
        let mut control = control_of(tree, index, label);
        let container = control
            .as_ref()
            .is_some_and(|_| is_container(tree, index, &self.input.refs));
        if let Some(found) = control.take() {
            if !container {
                into.items.push(Item::Control(found));
                return;
            }
            control = Some(found);
        }
        let kind = semantic_kind(node, context.in_section).or_else(|| {
            (node.layout.is_some() && is_block_display(node.display())).then_some(BlockKind::Generic)
        });
        let inner = WalkContext {
            in_section: context.in_section || is_sectioning(node),
            in_pre: context.in_pre || node.tag() == "PRE",
            ..context.deeper()
        };
        let Some(kind) = kind else {
            if let Some(mut found) = control {
                found.name.clear();
                into.items.push(Item::Control(found));
            }
            self.visit_children(&inner, index, into);
            return;
        };
        let mut block = Block::new(kind);
        block.key = repeat_key(node);
        if matches!(block.kind, BlockKind::Landmark(_) | BlockKind::Dialog) {
            block.label = dialog_label(tree, index);
        }
        if let Some(mut found) = control {
            found.name.clear();
            block.items.push(Item::Control(found));
        }
        self.visit_children(&inner, index, &mut block);
        if node.tag() == "DETAILS" && node.attr("open").is_none() {
            block.label = Some("collapsed".to_owned());
        }
        push_block(into, block);
    }
}

/// Appends a text run, joined to the run before it when there is one.
pub(crate) fn push_text(into: &mut Block, run: &str) {
    if run.is_empty() {
        return;
    }
    if let Some(Item::Text(last)) = into.items.last_mut() {
        if last.ends_with(' ') && run.starts_with(' ') {
            last.push_str(&run[1..]);
        } else {
            last.push_str(run);
        }
        return;
    }
    into.items.push(Item::Text(run.to_owned()));
}

/// Appends `block` unless it is empty. A wrapper holding a single block and
/// nothing else gives way to that block, and a block whose only content is
/// one plain wrapper takes the wrapper's content, so nesting that carries
/// no meaning costs no line.
pub(crate) fn push_block(into: &mut Block, mut block: Block) {
    if block.is_empty() {
        return;
    }
    block.items.retain(|item| match item {
        Item::Block(inner) => !inner.is_empty(),
        _ => true,
    });
    let only_block = match block.items.as_slice() {
        [Item::Block(inner)] => Some(inner.clone()),
        _ => None,
    };
    if let Some(mut inner) = only_block {
        if block.kind == BlockKind::Generic && block.label.is_none() {
            if inner.key.is_none() {
                inner.key = block.key;
            }
            into.items.push(Item::Block(inner));
            return;
        }
        if inner.kind == BlockKind::Generic && inner.label.is_none() {
            block.items = inner.items;
        }
    }
    into.items.push(Item::Block(block));
}
