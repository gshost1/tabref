use crate::outline::build::build_outline;
use crate::outline::item::Item;
use crate::outline::outline_input::OutlineInput;
use crate::outline::test_dom::TestDom;

fn outline_of(dom: TestDom) -> crate::outline::page_outline::PageOutline {
    build_outline(&OutlineInput {
        trees: vec![dom.tree],
        ..OutlineInput::default()
    })
}

fn holds_button(items: &[Item]) -> bool {
    items.iter().any(|item| match item {
        Item::Control(control) => control.role == "button",
        Item::Block(block) => holds_button(&block.items),
        _ => false,
    })
}

#[test]
fn a_dialog_with_a_hidden_box_and_a_visible_pane_is_lifted() {
    let mut dom = TestDom::new();
    let paragraph = dom.element(2, "p", &[], Some("block"));
    dom.text(paragraph, "Inbox", true);
    let layer = dom.element(
        2,
        "div",
        &[("role", "dialog"), ("aria-label", "New Message")],
        Some("block"),
    );
    dom.hide(layer);
    let pane = dom.element(layer, "div", &[], Some("block"));
    let send = dom.element(pane, "button", &[], Some("inline-block"));
    dom.text(send, "Send", true);
    let outline = outline_of(dom);
    assert_eq!(outline.dialogs.len(), 1);
    assert_eq!(outline.dialogs[0].label.as_deref(), Some("New Message"));
    assert!(holds_button(&outline.dialogs[0].items));
    assert!(!holds_button(&outline.body.items));
}

#[test]
fn a_dialog_with_nothing_visible_inside_is_left_out() {
    let mut dom = TestDom::new();
    let paragraph = dom.element(2, "p", &[], Some("block"));
    dom.text(paragraph, "Inbox", true);
    let layer = dom.element(2, "div", &[("role", "dialog")], Some("block"));
    dom.hide(layer);
    let send = dom.element(layer, "button", &[], Some("inline-block"));
    dom.hide(send);
    let text = dom.text(send, "Send", true);
    dom.hide(text);
    let outline = outline_of(dom);
    assert!(outline.dialogs.is_empty());
    assert!(!holds_button(&outline.body.items));
}
