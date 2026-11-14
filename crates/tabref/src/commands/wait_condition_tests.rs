use tabref_core::ErrorCode;

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
