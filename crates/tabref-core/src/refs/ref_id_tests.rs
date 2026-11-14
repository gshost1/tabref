use super::RefId;
use crate::ErrorCode;

#[test]
fn a_qualified_ref_round_trips() {
    let parsed = RefId::parse("@s8f3k2p9:e4").unwrap();
    assert_eq!(parsed, RefId::new("s8f3k2p9", 4));
    assert_eq!(parsed.to_string(), "@s8f3k2p9:e4");
}

#[test]
fn the_at_sign_is_optional_and_whitespace_is_trimmed() {
    assert_eq!(
        RefId::parse(" s8f3k2p9:e12 ").unwrap(),
        RefId::new("s8f3k2p9", 12)
    );
}

#[test]
fn malformed_refs_are_invalid_args() {
    for text in [
        "@e4",
        "e4",
        "@s8f3k2p9:e0",
        "@s8f3k2p9:e",
        "@s8f3k2p9:ex",
        "@s8:e1",
        "@s8f/3k2p9:e1",
        "@s8f3k2p9:e99999999999",
        "hello",
    ] {
        let error = RefId::parse(text).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgs, "{text}");
        assert!(!RefId::looks_like_ref(text), "{text}");
    }
}
