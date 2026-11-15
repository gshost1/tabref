use super::{is_block_display, is_dialog_markup, is_skipped, semantic_kind};
use crate::outline::block_kind::BlockKind;
use crate::outline::test_dom::TestDom;

#[test]
fn tags_and_roles_open_the_blocks_they_mean() {
    let mut dom = TestDom::new();
    let heading = dom.element(2, "h3", &[], Some("block"));
    let aria = dom.element(
        2,
        "div",
        &[("role", "heading"), ("aria-level", "4")],
        Some("block"),
    );
    let nav = dom.element(2, "div", &[("role", "navigation main")], Some("block"));
    let header = dom.element(2, "header", &[], Some("block"));
    let section = dom.element(2, "section", &[("aria-label", "News")], Some("block"));
    let layout = dom.element(2, "table", &[("role", "presentation")], Some("table"));
    let nodes = &dom.tree.nodes;
    assert_eq!(
        semantic_kind(&nodes[heading], false),
        Some(BlockKind::Heading(3))
    );
    assert_eq!(
        semantic_kind(&nodes[aria], false),
        Some(BlockKind::Heading(4))
    );
    assert_eq!(
        semantic_kind(&nodes[nav], false),
        Some(BlockKind::Landmark("nav".into()))
    );
    assert_eq!(
        semantic_kind(&nodes[header], false),
        Some(BlockKind::Landmark("header".into()))
    );
    assert_eq!(
        semantic_kind(&nodes[header], true),
        None,
        "a header inside an article is not the page's"
    );
    assert_eq!(
        semantic_kind(&nodes[section], false),
        Some(BlockKind::Landmark("region".into()))
    );
    assert_eq!(semantic_kind(&nodes[layout], false), None);
}

#[test]
fn display_decides_block_or_inline() {
    for display in [
        "block",
        "flex",
        "grid",
        "list-item",
        "table-row",
        "table-cell",
    ] {
        assert!(is_block_display(display), "{display}");
    }
    for display in [
        "inline",
        "inline-block",
        "inline-flex",
        "contents",
        "none",
        "",
    ] {
        assert!(!is_block_display(display), "{display}");
    }
}

#[test]
fn hidden_inputs_scripts_and_aria_hidden_are_skipped_and_dialogs_are_known() {
    let mut dom = TestDom::new();
    let hidden = dom.element(2, "input", &[("type", "HIDDEN")], Some("inline"));
    let script = dom.element(2, "script", &[], None);
    let decor = dom.element(2, "span", &[("aria-hidden", "true")], Some("inline"));
    let modal = dom.element(2, "div", &[("aria-modal", "true")], Some("block"));
    let nodes = &dom.tree.nodes;
    assert!(is_skipped(&nodes[hidden]) && is_skipped(&nodes[script]) && is_skipped(&nodes[decor]));
    assert!(is_dialog_markup(&nodes[modal]));
}
