use super::RefStyle;

#[test]
fn the_short_form_keeps_only_the_element_number() {
    assert_eq!(RefStyle::Long.print("@s8f3k2p9:e4"), "@s8f3k2p9:e4");
    assert_eq!(RefStyle::Short.print("@s8f3k2p9:e4"), "e4");
    assert_eq!(RefStyle::Short.print("e4"), "e4");
}
