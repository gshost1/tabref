use super::ElementState;

#[test]
fn each_state_name_parses_and_anything_else_is_refused() {
    assert_eq!(ElementState::parse("enabled"), Some(ElementState::Enabled));
    assert_eq!(ElementState::parse("Visible"), Some(ElementState::Visible));
    assert_eq!(ElementState::parse("hidden"), Some(ElementState::Hidden));
    assert_eq!(ElementState::parse("gone"), Some(ElementState::Gone));
    assert_eq!(
        ElementState::parse("value=62.00"),
        Some(ElementState::Value("62.00".into()))
    );
    assert_eq!(
        ElementState::parse("value="),
        Some(ElementState::Value(String::new()))
    );
    assert_eq!(ElementState::parse("shiny"), None);
}

#[test]
fn a_value_state_describes_its_value() {
    assert_eq!(
        ElementState::Value("a b".into()).describe(),
        "value \"a b\""
    );
}
