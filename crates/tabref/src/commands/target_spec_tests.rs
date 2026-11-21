use tabref_page::Word;

use super::{TargetSpec, looks_like_css};

#[test]
fn refs_stay_refs_and_other_text_is_a_name() {
    assert_eq!(
        TargetSpec::from_arg("@s8f3k2p9:e4").unwrap(),
        TargetSpec::Ref("@s8f3k2p9:e4".into())
    );
    assert_eq!(
        TargetSpec::from_arg("e12").unwrap(),
        TargetSpec::Ref("e12".into())
    );
    let TargetSpec::Find(locator) = TargetSpec::from_arg("Add to cart").unwrap() else {
        panic!("not a locator");
    };
    assert_eq!(locator.name.as_deref(), Some("Add to cart"));
    assert_eq!(locator.role, None);
}

#[test]
fn quoted_text_in_one_argument_reads_as_words() {
    let TargetSpec::Find(locator) = TargetSpec::from_arg(r#"button "Save" in "Row 3""#).unwrap()
    else {
        panic!("not a locator");
    };
    assert_eq!(locator.role.as_deref(), Some("button"));
    assert_eq!(locator.scope.as_deref(), Some("Row 3"));
}

#[test]
fn apostrophes_inside_words_do_not_start_target_grammar() {
    for text in ["Don't save", "O'Reilly", "James' account"] {
        let TargetSpec::Find(locator) = TargetSpec::from_arg(text).unwrap() else {
            panic!("not a locator")
        };
        assert_eq!(locator.name.as_deref(), Some(text));
    }
    let TargetSpec::Find(locator) = TargetSpec::from_arg("button \"Don't save\"").unwrap() else {
        panic!("not a locator")
    };
    assert_eq!(locator.name.as_deref(), Some("Don't save"));
}

#[test]
fn a_quoted_ref_lookalike_is_a_name() {
    assert!(matches!(
        TargetSpec::from_words(&[Word::quoted("e4")]).unwrap(),
        TargetSpec::Find(_)
    ));
}

#[test]
fn css_selectors_are_told_apart_from_names() {
    for css in [
        "input[type=file]",
        "input[type=\"file\"]",
        "#file",
        ".dz-hidden-input",
        "form > input",
        "a.button",
        "li:nth-child(2)",
    ] {
        assert!(looks_like_css(css), "{css}");
    }
    for name in [
        "Add to cart",
        "Save",
        "select more files…",
        "e4",
        "# of items",
        "Q3 report",
    ] {
        assert!(!looks_like_css(name), "{name}");
    }
}
