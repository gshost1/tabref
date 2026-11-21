use super::block::Block;
use super::block_kind::BlockKind;
use super::outline_builder::OutlineBuilder;
use super::outline_input::OutlineInput;
use super::page_outline::PageOutline;
use super::walk_context::WalkContext;

/// Reads the page's main document into an outline: the body in reading
/// order with dialogs and fixed overlays lifted out, frames shown or
/// counted.
pub fn build_outline(input: &OutlineInput) -> PageOutline {
    let mut builder = OutlineBuilder::new(input, true);
    let mut body = Block::new(BlockKind::Generic);
    if let Some(tree) = input.trees.first() {
        builder.visit(&WalkContext::default(), tree.body(), &mut body);
    }
    flatten_root(&mut body);
    PageOutline {
        body,
        dialogs: builder.dialogs,
        frames: builder.frames,
    }
}

/// Reads one element, found by its target and backend node id in any
/// captured document, into an outline whose body is that element. Dialogs
/// inside it stay where they are. `None` when no document holds it.
pub fn build_outline_at(
    input: &OutlineInput,
    target_id: &str,
    backend_id: i64,
) -> Option<PageOutline> {
    let mut builder = OutlineBuilder::new(input, false);
    let (capture, document, index) =
        builder
            .captures
            .iter()
            .enumerate()
            .find_map(|(capture, trees)| {
                trees.iter().enumerate().find_map(|(document, tree)| {
                    (tree.target_id == target_id)
                        .then(|| tree.find_backend(backend_id))
                        .flatten()
                        .map(|index| (capture, document, index))
                })
            })?;
    let context = WalkContext {
        capture,
        document,
        ..WalkContext::default()
    };
    let mut body = Block::new(BlockKind::Generic);
    builder.visit(&context, index, &mut body);
    flatten_root(&mut body);
    Some(PageOutline {
        body,
        dialogs: builder.dialogs,
        frames: builder.frames,
    })
}

fn flatten_root(body: &mut Block) {
    while let [super::item::Item::Block(inner)] = body.items.as_mut_slice() {
        if inner.kind != BlockKind::Generic {
            break;
        }
        body.items = std::mem::take(&mut inner.items);
    }
}
