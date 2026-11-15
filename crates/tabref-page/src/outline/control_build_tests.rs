use super::control_of;
use crate::outline::ref_label::RefLabel;
use crate::outline::test_dom::TestDom;

fn label(role: &str, name: &str) -> RefLabel {
    RefLabel {
        ref_text: "@s1:e3".into(),
        role: role.into(),
        name: Some(name.into()),
        ..RefLabel::default()
    }
}

#[test]
fn a_select_lists_its_options_with_the_selected_one_as_value() {
    let mut dom = TestDom::new();
    let select = dom.element(2, "select", &[("name", "assignee")], Some("inline-block"));
    let none = dom.element(select, "option", &[("value", "")], None);
    dom.text(none, "Unassigned", false);
    let ada = dom.element(select, "option", &[("value", "asalazar")], None);
    dom.text(ada, "Ada Salazar", false);
    dom.form(ada).option_selected = true;
    let control = control_of(&dom.tree, select, Some(&label("combobox", "Assignee"))).unwrap();
    assert_eq!(control.name, "Assignee");
    assert_eq!(control.value.as_deref(), Some("Ada Salazar"));
    assert_eq!(control.options.len(), 2);
    assert_eq!(
        (
            control.options[1].value.as_str(),
            control.options[1].selected
        ),
        ("asalazar", true)
    );
}

#[test]
fn a_link_resolves_its_address_and_a_cell_ref_is_not_a_control() {
    let mut dom = TestDom::new();
    let cell = dom.element(2, "td", &[], Some("table-cell"));
    let link = dom.element(cell, "a", &[("href", "/item/4/edit")], Some("inline"));
    dom.text(link, "Edit", true);
    let control = control_of(&dom.tree, link, None).unwrap();
    assert_eq!(
        (control.role.as_str(), control.name.as_str()),
        ("link", "Edit")
    );
    assert_eq!(
        control.href.as_deref(),
        Some("http://site.test/item/4/edit")
    );
    assert!(control_of(&dom.tree, cell, Some(&label("cell", "Edit"))).is_none());
}

#[test]
fn states_come_from_the_dom_and_the_snapshot() {
    let mut dom = TestDom::new();
    let details = dom.element(2, "details", &[], Some("block"));
    let summary = dom.element(details, "summary", &[], Some("list-item"));
    dom.text(summary, "Reply", true);
    let date = dom.element(
        2,
        "input",
        &[("type", "date"), ("aria-label", "Due")],
        Some("inline-block"),
    );
    let checkbox = dom.element(
        2,
        "input",
        &[("type", "checkbox"), ("disabled", "")],
        Some("inline-block"),
    );
    dom.form(checkbox).checked = true;
    let toggle = dom.element(
        2,
        "button",
        &[("aria-expanded", "false")],
        Some("inline-block"),
    );
    dom.text(toggle, "Economics", true);
    assert_eq!(
        control_of(&dom.tree, summary, None).unwrap().states,
        vec!["collapsed"]
    );
    assert_eq!(
        control_of(&dom.tree, date, None).unwrap().states,
        vec!["type=date"]
    );
    assert_eq!(
        control_of(&dom.tree, checkbox, None).unwrap().states,
        vec!["checked", "disabled"]
    );
    let button = control_of(&dom.tree, toggle, None).unwrap();
    assert_eq!(
        (button.name.as_str(), button.states.as_slice()),
        ("Economics", ["collapsed".to_owned()].as_slice())
    );
}
