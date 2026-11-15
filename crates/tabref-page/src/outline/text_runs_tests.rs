use super::{collapse_run, cut_chars, squash, subtree_text};
use crate::outline::test_dom::TestDom;

#[test]
fn runs_keep_one_space_at_their_ends() {
    assert_eq!(collapse_run("  For six\n years "), " For six years ");
    assert_eq!(collapse_run("word"), "word");
    assert_eq!(collapse_run("\n\t"), " ");
    assert_eq!(collapse_run(""), "");
    assert_eq!(squash("  a \n b "), "a b");
}

#[test]
fn cutting_counts_characters_and_marks_the_cut() {
    assert_eq!(cut_chars("ʌnˈblem", 10), "ʌnˈblem");
    assert_eq!(cut_chars("abcdefgh", 5), "abcd…");
}

#[test]
fn subtree_text_joins_inline_runs_and_spaces_boxes() {
    let mut dom = TestDom::new();
    let paragraph = dom.element(2, "p", &[], Some("block"));
    dom.text(paragraph, "(of ", true);
    let link = dom.element(paragraph, "a", &[("href", "/skin")], Some("inline"));
    dom.text(link, "skin", true);
    dom.text(paragraph, ") marks", true);
    let tag = dom.element(paragraph, "span", &[], Some("inline-block"));
    dom.text(tag, "approving", true);
    assert_eq!(
        subtree_text(&dom.tree, paragraph),
        "(of skin) marks approving"
    );
}
