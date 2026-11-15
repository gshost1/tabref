use tabref_core::ErrorCode;
use tabref_page::Word;

use super::Condition;

#[test]
fn exactly_one_flag_names_the_condition() {
    assert_eq!(
        Condition::from_flags(Some("Done"), None, None, false).unwrap(),
        Condition::Text("Done".into())
    );
    assert_eq!(
        Condition::from_flags(None, None, None, true).unwrap(),
        Condition::Idle
    );
    for (text, url, idle) in [
        (None, None, false),
        (Some("a"), Some("/b"), false),
        (Some("a"), None, true),
    ] {
        let error = Condition::from_flags(text, None, url, idle).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }
}

#[test]
fn a_step_names_its_condition_in_words() {
    let words = |list: &[&str]| list.iter().map(|text| Word::bare(text)).collect::<Vec<_>>();
    assert_eq!(
        Condition::from_words(&words(&["url", "~saved=1"])).unwrap(),
        Condition::Url("saved=1".into())
    );
    assert_eq!(
        Condition::from_words(&words(&["url", "/saved/"])).unwrap(),
        Condition::Url("saved".into())
    );
    assert_eq!(
        Condition::from_words(&[Word::bare("text"), Word::quoted("You saved")]).unwrap(),
        Condition::Text("You saved".into())
    );
    assert_eq!(
        Condition::from_words(&[Word::quoted("Saved")]).unwrap(),
        Condition::Text("Saved".into())
    );
    assert_eq!(
        Condition::from_words(&words(&["idle"])).unwrap(),
        Condition::Idle
    );
    assert_eq!(
        Condition::from_words(&words(&["text"])).unwrap_err().code,
        ErrorCode::InvalidArgs
    );
}
