use tabref_core::ErrorCode;

use super::Locator;
use crate::locate::word::Word;

fn parse(words: &[Word]) -> Locator {
    Locator::from_words(words).unwrap()
}

#[test]
fn a_quoted_name_alone_is_the_name() {
    let locator = parse(&[Word::quoted("Save")]);
    assert_eq!(locator.name.as_deref(), Some("Save"));
    assert_eq!(locator.role, None);
}

#[test]
fn an_unquoted_role_word_before_a_name_is_the_role() {
    let locator = parse(&[
        Word::bare("button"),
        Word::quoted("Save"),
        Word::bare("in"),
        Word::quoted("Row 3"),
        Word::bare("nth"),
        Word::bare("2"),
    ]);
    assert_eq!(locator.role.as_deref(), Some("button"));
    assert_eq!(locator.name.as_deref(), Some("Save"));
    assert_eq!(locator.scope.as_deref(), Some("Row 3"));
    assert_eq!(locator.nth, Some(2));
    assert_eq!(locator.describe(), r#"button "Save" nth 2 in "Row 3""#);
}

#[test]
fn a_quoted_role_word_is_a_name_and_a_bare_one_alone_is_a_role() {
    assert_eq!(parse(&[Word::quoted("link")]).name.as_deref(), Some("link"));
    let role_only = parse(&[Word::bare("textbox"), Word::bare("in"), Word::quoted("MOD")]);
    assert_eq!(role_only.role.as_deref(), Some("textbox"));
    assert_eq!(role_only.name, None);
}

#[test]
fn a_bad_nth_or_a_stray_word_is_refused() {
    let bad = Locator::from_words(&[Word::quoted("Edit"), Word::bare("nth"), Word::bare("x")]);
    assert_eq!(bad.unwrap_err().code, ErrorCode::InvalidArgs);
    let stray = Locator::from_words(&[Word::quoted("Edit"), Word::quoted("More")]);
    assert_eq!(stray.unwrap_err().code, ErrorCode::InvalidArgs);
}
