use super::block::Block;
use super::block_kind::BlockKind;
use super::item::Item;
use super::outline_builder::{OutlineBuilder, push_block};
use super::url_resolve::origin_of;
use super::walk_context::WalkContext;

const MAX_FRAME_DEPTH: usize = 4;
const LARGE_FRAME_SHARE: f64 = 0.25;
const FORM_ROLES: &[&str] = &["textfield", "combobox", "checkbox", "radiobutton", "listbox"];

impl OutlineBuilder<'_> {
    /// Reads the document an `<iframe>` shows into a frame block, or counts
    /// why it is left out. A frame of one pixel or less never shows. An
    /// in-process frame shows when it holds anything. A cross-origin frame
    /// shows only when it has a field to fill in or covers a quarter of the
    /// viewport, so ad slots and widgets cost one summary line.
    pub(crate) fn frame(&mut self, context: &WalkContext, index: usize, into: &mut Block) {
        let Some(tree) = self.tree(context) else {
            return;
        };
        let Some(node) = tree.node(index) else {
            return;
        };
        let Some(layout) = node.layout.as_ref().filter(|layout| layout.visible && !layout.is_tiny()) else {
            self.frames.tiny += 1;
            return;
        };
        if context.frame_depth >= MAX_FRAME_DEPTH {
            self.frames.unread += 1;
            return;
        }
        let area_share = {
            let [width, height] = self.input.viewport;
            if width > 0.0 && height > 0.0 { layout.area() / (width * height) } else { 0.0 }
        };
        let owner = (tree.target_id.clone(), node.backend_id);
        let (target, cross_target) = if let Some(document) = node.form.content_document {
            (Some((context.capture, document)), false)
        } else {
            let capture = self.frame_captures.get(&owner).map(|capture| (*capture, 0));
            (capture, true)
        };
        let Some((capture, document)) = target else {
            let page_origin = self.captures.first().and_then(|trees| trees.first()).and_then(|tree| origin_of(&tree.url));
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
        block.label = origin_of(&frame_url).map(|origin| origin.trim_start_matches("https://").to_owned());
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
        let page_origin = self.captures.first().and_then(|trees| trees.first()).and_then(|tree| origin_of(&tree.url));
        let cross_origin = cross_target || origin_of(&frame_url) != page_origin;
        if cross_origin && area_share < LARGE_FRAME_SHARE && !has_field(&block) {
            self.frames.cross_origin += 1;
            return;
        }
        self.frames.shown += 1;
        push_block(into, block);
    }
}

fn has_field(block: &Block) -> bool {
    block.items.iter().any(|item| match item {
        Item::Control(control) => FORM_ROLES.contains(&control.role.as_str()),
        Item::Block(inner) => has_field(inner),
        _ => false,
    })
}
