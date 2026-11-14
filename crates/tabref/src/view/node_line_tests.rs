use tabref_core::{AccessibilityNode, NameSource};

use super::{node_line, quote_text};

fn node(role: &str, name: Option<&str>) -> AccessibilityNode {
    let mut node = AccessibilityNode {
        role: role.into(),
        ..AccessibilityNode::default()
    };
    node.identity.name = name.map(str::to_owned);
    node
}

#[test]
fn a_field_line_carries_ref_role_name_value_and_states() {
    let mut field = node("textfield", Some("Email"));
    field.ref_id = Some("@s1234567:e3".into());
    field.identity.value = Some("ada@example.test".into());
    field.presentation.states = vec!["focused".into(), "required".into()];
    assert_eq!(
        node_line(&field, 160),
        r#"@s1234567:e3 textfield "Email" value="ada@example.test" [focused, required]"#
    );
}

#[test]
fn a_derived_name_prints_and_static_text_reads_as_text() {
    let mut row = node("group", None);
    row.presentation.name_source = Some(NameSource::ContainedText);
    row.presentation.derived_name = Some("Open report".into());
    assert_eq!(node_line(&row, 160), r#"group "Open report""#);
    assert_eq!(
        node_line(&node("statictext", Some("Hi")), 160),
        r#"text "Hi""#
    );
}

#[test]
fn a_description_that_repeats_the_name_is_left_out_and_long_names_are_cut() {
    let mut link = node("link", Some("Docs"));
    link.identity.description = Some(" Docs ".into());
    assert_eq!(node_line(&link, 160), r#"link "Docs""#);
    assert_eq!(
        node_line(&node("heading", Some("abcdef")), 3),
        r#"heading "abc...""#
    );
}

#[test]
fn quoting_keeps_a_line_on_one_line() {
    assert_eq!(quote_text("a\"b\\c\nd\u{200E}"), r#""a\"b\\c\nd""#);
    assert_eq!(quote_text("\u{7}"), r#""\u0007""#);
}
