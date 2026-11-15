use super::search_text;
use crate::outline::outline_input::OutlineInput;
use crate::outline::ref_label::RefLabel;
use crate::outline::test_dom::TestDom;

fn input(dom: TestDom) -> OutlineInput {
    OutlineInput {
        trees: vec![dom.tree],
        ..OutlineInput::default()
    }
}

#[test]
fn hidden_text_is_found_with_its_section_and_the_control_that_shows_it() {
    let mut dom = TestDom::new();
    let heading = dom.element(2, "h2", &[], Some("block"));
    let button = dom.element(
        heading,
        "button",
        &[("aria-controls", "panel")],
        Some("inline-block"),
    );
    dom.text(button, "Economics", true);
    let panel = dom.element(2, "div", &[("id", "panel")], None);
    let item = dom.element(panel, "h3", &[], None);
    dom.text(item, "econ.EM (Econometrics)", false);
    let mut search = input(dom);
    let button_backend = i64::try_from(button).unwrap() + 1;
    search.refs.insert(
        ("T1".into(), button_backend),
        RefLabel {
            ref_text: "@s1:e8".into(),
            role: "button".into(),
            name: Some("Economics".into()),
            ..RefLabel::default()
        },
    );
    let (found, total) = search_text(&search, "ECON.", 20);
    assert_eq!(total, 1);
    assert!(found[0].hidden);
    assert_eq!(found[0].section, vec!["Economics".to_owned()]);
    assert_eq!(found[0].line, "econ.EM (Econometrics)");
    assert_eq!(
        found[0]
            .expander
            .as_ref()
            .and_then(|control| control.ref_text.as_deref()),
        Some("@s1:e8")
    );
}

#[test]
fn a_match_in_a_cell_reads_as_its_row_with_the_row_control() {
    let mut dom = TestDom::new();
    let table = dom.element(2, "table", &[], Some("table"));
    let row = dom.element(table, "tr", &[], Some("table-row"));
    for text in ["000165", "Complete"] {
        let cell = dom.element(row, "td", &[], Some("table-cell"));
        dom.text(cell, text, true);
    }
    let cell = dom.element(row, "td", &[], Some("table-cell"));
    let link = dom.element(cell, "a", &[("href", "/order/165")], Some("inline"));
    dom.text(link, "View", true);
    let (found, total) = search_text(&input(dom), "complete", 20);
    assert_eq!(total, 1);
    assert_eq!(found[0].line, "000165 Complete View");
    assert!(!found[0].hidden);
    assert_eq!(
        found[0]
            .control
            .as_ref()
            .map(|control| control.name.as_str()),
        None
    );
}

#[test]
fn plain_text_with_no_match_is_an_empty_result() {
    let mut dom = TestDom::new();
    dom.text(2, "nothing here", true);
    assert_eq!(search_text(&input(dom), "[v3]", 20), (Vec::new(), 0));
    assert_eq!(
        search_text(&OutlineInput::default(), " ", 20),
        (Vec::new(), 0)
    );
}
