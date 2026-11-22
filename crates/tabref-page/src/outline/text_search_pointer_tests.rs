use super::super::outline_input::OutlineInput;
use super::super::test_dom::TestDom;
use super::search_text;

#[test]
fn ambiguous_truncated_and_non_narrowing_pre_lines_have_no_pointer() {
    for (text, needle) in [
        ("repeat needle\nrepeat needle\n".to_owned(), "needle"),
        ("needle\n".to_owned(), "needle"),
        (format!("needle {}\n", "x".repeat(300)), "needle"),
        ("needle [bracketed]\n".to_owned(), "needle"),
    ] {
        let mut dom = TestDom::new();
        let pre = dom.element(2, "pre", &[], Some("block"));
        dom.text(pre, &text, true);
        let input = OutlineInput {
            trees: vec![dom.tree],
            ..OutlineInput::default()
        };
        let (hits, _) = search_text(&input, needle, 20);
        assert!(!hits.is_empty());
        assert!(hits.iter().all(|hit| !hit.line.contains(" (tabref ")));
        assert!(search_text(&input, "absent", 20).0.is_empty());
        assert!(search_text(&input, "", 20).0.is_empty());
    }
}

#[test]
fn a_pre_pointer_is_omitted_when_other_blocks_match_its_query() {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    dom.text(pre, "distinctive needle line\n", true);
    let paragraph = dom.element(2, "p", &[], Some("block"));
    dom.text(
        paragraph,
        "Also contains distinctive needle line here.",
        true,
    );
    let input = OutlineInput {
        trees: vec![dom.tree],
        ..OutlineInput::default()
    };
    let (hits, total) = search_text(&input, "needle", 20);
    assert_eq!(total, 2);
    assert_eq!(hits[0].line, "distinctive needle line");
}
