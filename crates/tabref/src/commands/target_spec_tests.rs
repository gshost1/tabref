use tabref_page::Word;

use super::TargetSpec;

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
fn a_quoted_ref_lookalike_is_a_name() {
    assert!(matches!(
        TargetSpec::from_words(&[Word::quoted("e4")]).unwrap(),
        TargetSpec::Find(_)
    ));
}
