use tabref_cdp::PageTarget;

use super::{order_text, stable_order};

fn tab(id: &str) -> PageTarget {
    PageTarget {
        target_id: id.into(),
        url: String::new(),
        title: String::new(),
        attached: false,
        browser_context_id: None,
    }
}

fn ids(tabs: &[PageTarget]) -> Vec<&str> {
    tabs.iter().map(|tab| tab.target_id.as_str()).collect()
}

#[test]
fn a_new_tab_goes_last_even_when_the_browser_lists_it_first() {
    let listed = vec![tab("NEW"), tab("A"), tab("B")];
    let ordered = stable_order(listed, "A\nB\n");
    assert_eq!(ids(&ordered), ["A", "B", "NEW"]);
}

#[test]
fn closed_tabs_drop_out_and_the_rest_keep_their_order() {
    let ordered = stable_order(vec![tab("C"), tab("A")], "A\nB\nC");
    assert_eq!(ids(&ordered), ["A", "C"]);
    assert_eq!(order_text(&ordered), "A\nC");
}

#[test]
fn without_a_saved_order_the_browser_order_stands() {
    let ordered = stable_order(vec![tab("B"), tab("A")], "");
    assert_eq!(ids(&ordered), ["B", "A"]);
}
