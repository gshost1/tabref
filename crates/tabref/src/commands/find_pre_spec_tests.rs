use super::{AccessibilityNode, OutlineInput, TestDom, input, merged_matches, options, reference};
use crate::view::find_view::find_lines_with_ax;

fn printed(input: &OutlineInput, query: &str, keep: usize) -> Vec<String> {
    let (rows, count, _) = merged_matches(input, &AccessibilityNode::default(), query, keep);
    find_lines_with_ax(query, &rows, &[], count, &options())
}

fn linked_pre(count: usize) -> OutlineInput {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    let mut links = Vec::new();
    for line in 1..=count {
        dom.text(pre, &format!("{line}. See "), true);
        let link = dom.element(pre, "a", &[("href", "/open.2")], Some("inline"));
        dom.text(link, "open(2)", true);
        dom.text(pre, " for details.\n", true);
        links.push(link);
    }
    let mut input = input(dom);
    for (line, link) in links.into_iter().enumerate() {
        reference(&mut input, link, line + 1, "link", "open(2)");
    }
    input
}

#[test]
fn plain_pre_reports_all_six_hits_without_a_footer() {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    dom.text(
        pre,
        "needle 1\nneedle 2\nneedle 3\nneedle 4\nneedle 5\nneedle 6",
        true,
    );
    let lines = printed(&input(dom), "needle", 20);
    assert_eq!(lines[0], "6 matches for \"needle\"");
    assert_eq!(lines.len(), 5);
    for (i, line) in lines[1..4].iter().enumerate() {
        assert!(line.starts_with(&format!("- needle {}", i + 1)));
    }
    assert_eq!(lines[4], "- … 3 more in this block");
}

#[test]
fn linked_pre_overflow_lists_each_folded_ref() {
    let lines = printed(&linked_pre(6), "open(2)", 20);
    assert_eq!(lines[0], "6 matches for \"open(2)\"");
    assert_eq!(lines[4], "- … 3 more in this block (@s1:e4 @s1:e5 @s1:e6)");
    assert_eq!(lines.len(), 5);
    for (i, line) in lines[1..4].iter().enumerate() {
        assert!(line.contains(&format!("[e{} ", i + 1)), "{line}");
    }
}

#[test]
fn two_row_budget_omits_hidden_refs_and_counts_four_unprinted_hits() {
    let lines = printed(&linked_pre(6), "open(2)", 2);
    assert_eq!(lines[0], "6 matches for \"open(2)\"");
    assert_eq!(lines.len(), 4);
    assert!(lines[3].starts_with("… 4 more;"), "{lines:?}");
    for i in 3..=6 {
        assert!(!lines.join("\n").contains(&format!("e{i}")), "{lines:?}");
    }
}

#[test]
fn linked_pre_overflow_lists_at_most_eight_refs_in_page_order() {
    let lines = printed(&linked_pre(15), "open(2)", 20);
    assert_eq!(lines[0], "15 matches for \"open(2)\"");
    assert_eq!(
        lines[4],
        "- … 12 more in this block (@s1:e4 @s1:e5 @s1:e6 @s1:e7 @s1:e8 @s1:e9 @s1:e10 @s1:e11 +4 refs)"
    );
    assert_eq!(lines.len(), 5);
}

#[test]
fn the_first_matching_inline_control_owns_the_line_and_the_second_owns_a_row() {
    for first in ["foo", "unrelated"] {
        let mut dom = TestDom::new();
        let pre = dom.element(2, "pre", &[], Some("block"));
        dom.text(pre, "See ", true);
        let a = dom.element(pre, "a", &[("href", "/first")], Some("inline"));
        dom.text(a, first, true);
        dom.text(pre, " or ", true);
        let b = dom.element(pre, "a", &[("href", "/second")], Some("inline"));
        dom.text(b, "foo", true);
        let mut input = input(dom);
        reference(&mut input, a, 1, "link", first);
        reference(&mut input, b, 2, "link", "foo");
        let (rows, count, _) = merged_matches(&input, &AccessibilityNode::default(), "foo", 20);
        let refs: Vec<_> = rows
            .iter()
            .map(|row| row.control.as_ref().unwrap().ref_text.as_deref().unwrap())
            .collect();
        if first == "foo" {
            assert_eq!(
                (count, refs.as_slice()),
                (2, ["@s1:e1", "@s1:e2"].as_slice())
            );
            assert_eq!(rows[1].line, "foo");
        } else {
            assert_eq!((count, refs.as_slice()), (1, ["@s1:e2"].as_slice()));
        }
    }
}

pub(super) fn assert_capped_long_pre(input: &OutlineInput) {
    let lines = printed(input, "let", 3);
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[0], "101 matches for \"let\"");
    assert!(lines[4].starts_with("… 98 more;"), "{lines:?}");
}
