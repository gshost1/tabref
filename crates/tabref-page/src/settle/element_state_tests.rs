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

#[test]
fn one_pair_of_matching_quotes_around_a_value_is_stripped() {
    assert_eq!(
        ElementState::parse("value=\"a b\""),
        Some(ElementState::Value("a b".into()))
    );
    assert_eq!(
        ElementState::parse("value='62.00'"),
        Some(ElementState::Value("62.00".into()))
    );
    assert_eq!(
        ElementState::parse("value=\"a'"),
        Some(ElementState::Value("\"a'".into()))
    );
    assert_eq!(
        ElementState::parse("value=\"\"x\"\""),
        Some(ElementState::Value("\"x\"".into()))
    );
    assert_eq!(ElementState::parse("shown"), None);
}
