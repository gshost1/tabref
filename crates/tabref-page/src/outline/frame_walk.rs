use super::block::Block;
use super::block_kind::BlockKind;
use super::item::Item;
use super::outline_builder::{OutlineBuilder, push_block};
use super::overlay::{is_dialog_candidate, keeps_as_dialog};
use super::url_resolve::origin_of;
use super::walk_context::WalkContext;

const MAX_FRAME_DEPTH: usize = 4;
const LARGE_FRAME_SHARE: f64 = 0.25;
const FORM_ROLES: &[&str] = &[
    "textfield",
    "combobox",
    "checkbox",
    "radiobutton",
    "listbox",
];

impl OutlineBuilder<'_> {
    /// Reads the document an `<iframe>` shows into a frame block, or counts
    /// why it is left out. A frame of one pixel or less never shows. An
    /// in-process frame shows when it holds anything. A cross-origin frame
    /// shows only when it has a field or a button or covers a quarter of the
    /// viewport, so ad slots cost one summary line. A frame laid over the
    /// page like a dialog (a consent wall) is lifted to the top.
    pub(crate) fn frame(&mut self, context: &WalkContext, index: usize, into: &mut Block) {
        let Some(tree) = self.tree(context) else {
            return;
        };
        let Some(node) = tree.node(index) else {
            return;
        };
        let Some(layout) = node
            .layout
            .as_ref()
            .filter(|layout| layout.visible && !layout.is_tiny())
        else {
            self.frames.tiny += 1;
            return;
        };
        if context.frame_depth >= MAX_FRAME_DEPTH {
            self.frames.unread += 1;
            return;
        }
        let area_share = {
            let [width, height] = self.input.viewport;
            if width > 0.0 && height > 0.0 {
                layout.area() / (width * height)
            } else {
                0.0
            }
        };
        let owner = (tree.target_id.clone(), node.backend_id);
        let (target, cross_target) = if let Some(document) = node.form.content_document {
            (Some((context.capture, document)), false)
        } else {
            let capture = self.frame_captures.get(&owner).map(|capture| (*capture, 0));
            (capture, true)
        };
        let Some((capture, document)) = target else {
            let page_origin = self
                .captures
                .first()
                .and_then(|trees| trees.first())
                .and_then(|tree| origin_of(&tree.url));
            let src = node.attr("src").unwrap_or_default();
            if origin_of(src).is_some() && origin_of(src) != page_origin {
                self.frames.cross_origin += 1;
            } else {
                self.frames.unread += 1;
            }
            return;
        };
        let inner = WalkContext {
            capture,
            document,
            depth: context.depth + 1,
            in_section: false,
            in_pre: false,
            in_main: false,
            frame_depth: context.frame_depth + 1,
        };
        let mut block = Block::new(BlockKind::Frame);
        let frame_url = self
            .captures
            .get(capture)
            .and_then(|trees| trees.get(document))
            .map(|tree| tree.url.clone())
            .unwrap_or_default();
        block.label = origin_of(&frame_url).map(|origin| {
            origin
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .to_owned()
        });
        let body = self
            .captures
            .get(capture)
            .and_then(|trees| trees.get(document))
            .map_or(0, |tree| tree.body());
        self.visit(&inner, body, &mut block);
        if block.is_empty() {
            self.frames.empty += 1;
            return;
        }
        let page_origin = self
            .captures
            .first()
            .and_then(|trees| trees.first())
            .and_then(|tree| origin_of(&tree.url));
        let frame_origin = origin_of(&frame_url);
        let cross_origin = cross_target || (frame_origin.is_some() && frame_origin != page_origin);
        if cross_origin && area_share < LARGE_FRAME_SHARE && !holds_input(&block) {
            self.frames.cross_origin += 1;
            return;
        }
        self.frames.shown += 1;
        if self.lift_dialogs
            && is_dialog_candidate(node, self.input.viewport)
            && keeps_as_dialog(node, &block)
        {
            block.kind = BlockKind::Dialog;
            if let Some(heading) = first_heading_text(&block) {
                block.label = Some(heading);
            }
            self.dialogs.push(block);
            return;
        }
        push_block(into, block);
    }
}

/// Whether a frame asks for something: a field to fill in or a button to
/// press (a consent wall, a payment form).
fn holds_input(block: &Block) -> bool {
    block.items.iter().any(|item| match item {
        Item::Control(control) => FORM_ROLES.contains(&control.role.as_str()),
        Item::Block(inner) => holds_input(inner),
        _ => false,
    })
}

/// The text of the first heading in `block`, for naming a frame lifted as a
/// dialog.
fn first_heading_text(block: &Block) -> Option<String> {
    block.items.iter().find_map(|item| match item {
        Item::Block(inner) if matches!(inner.kind, BlockKind::Heading(_)) => {
            let text: String = inner
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Text(run) => Some(run.as_str()),
                    _ => None,
                })
                .collect();
            let text = super::text_runs::squash(&text);
            (!text.is_empty()).then_some(text)
        }
        Item::Block(inner) => first_heading_text(inner),
        _ => None,
    })
}
