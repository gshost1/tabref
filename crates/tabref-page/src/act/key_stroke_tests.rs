use super::KeyStroke;

#[test]
fn a_named_key_keeps_its_code_and_text() {
    let enter = KeyStroke::parse("Enter").unwrap();
    assert_eq!(enter.key, "Enter");
    assert_eq!(enter.key_code, 13);
    assert_eq!(enter.text.as_deref(), Some("\r"));
    assert_eq!(enter.modifiers, 0);
}

#[test]
fn a_character_types_itself_and_names_its_physical_key() {
    let a = KeyStroke::parse("a").unwrap();
    assert_eq!(
        (a.key.as_str(), a.code.as_str(), a.key_code),
        ("a", "KeyA", 65)
    );
    assert_eq!(a.text.as_deref(), Some("a"));
    let seven = KeyStroke::parse("7").unwrap();
    assert_eq!((seven.code.as_str(), seven.key_code), ("Digit7", 55));
    assert_eq!(KeyStroke::parse("/").unwrap().text.as_deref(), Some("/"));
}

#[test]
fn modifiers_set_their_bits_and_a_command_chord_types_nothing() {
    let select_all = KeyStroke::parse("Control+a").unwrap();
    assert_eq!(select_all.modifiers, 2);
    assert_eq!(select_all.text, None);
    let back_tab = KeyStroke::parse("shift+Tab").unwrap();
    assert_eq!(back_tab.modifiers, 8);
    let shout = KeyStroke::parse("Shift+a").unwrap();
    assert_eq!(shout.text.as_deref(), Some("A"));
    assert_eq!(KeyStroke::parse("Meta+Shift+z").unwrap().modifiers, 12);
}

#[test]
fn the_plus_key_and_unknown_names() {
    assert_eq!(KeyStroke::parse("+").unwrap().text.as_deref(), Some("+"));
    assert_eq!(KeyStroke::parse("Control++").unwrap().key, "+");
    assert!(KeyStroke::parse("Hyper").is_none());
    assert!(KeyStroke::parse("Hyper+a").is_none());
    assert!(KeyStroke::parse("").is_none());
}
