use super::{OutlineInput, RefLabel, TestDom, input, search_text};

fn reference(input: &mut OutlineInput, index: usize, role: &str, name: &str) {
    input.refs.insert(
        ("T1".into(), i64::try_from(index).unwrap() + 1),
        RefLabel {
            ref_text: "@s1:e1".into(),
            role: role.into(),
            name: Some(name.into()),
            ..RefLabel::default()
        },
    );
}

#[test]
fn a_referenced_code_wrapper_keeps_capped_pre_lines_and_its_ref() {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    let code = dom.element(pre, "code", &[], Some("inline"));
    dom.text(
        code,
        "needle 1\nneedle 2\nneedle 3\nneedle 4\nneedle 5",
        true,
    );
    let mut input = input(dom);
    reference(&mut input, code, "button", "Copy");
    let (hits, total) = search_text(&input, "needle", 20);
    assert_eq!(total, 4);
    for (index, hit) in hits[..3].iter().enumerate() {
        assert!(hit.line.starts_with(&format!("needle {}", index + 1)));
        assert_eq!(
            hit.control.as_ref().unwrap().ref_text.as_deref(),
            Some("@s1:e1")
        );
    }
    assert_eq!(hits[3].line, "… 2 more in this block");
}

#[test]
fn a_referenced_pre_does_not_repeat_its_matched_lines_as_a_control() {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    dom.text(pre, "needle 1\nneedle 2", true);
    let mut input = input(dom);
    reference(&mut input, pre, "button", "Copy");
    let (hits, total) = search_text(&input, "needle", 20);
    assert_eq!(total, 2);
    assert_eq!(
        hits[0].control.as_ref().unwrap().ref_text.as_deref(),
        Some("@s1:e1")
    );
}

#[test]
fn a_pre_line_keeps_inline_link_text_and_its_ref() {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    dom.text(pre, "see ", true);
    let link = dom.element(pre, "a", &[("href", "/x")], Some("inline"));
    dom.text(link, "docs", true);
    dom.text(pre, " for needle more", true);
    let mut input = input(dom);
    reference(&mut input, link, "link", "docs");
    for query in ["see docs", "needle"] {
        let (hits, total) = search_text(&input, query, 20);
        assert_eq!(total, 1, "{query}");
        assert!(hits[0].line.starts_with("see docs for needle more"));
        assert_eq!(
            hits[0].control.as_ref().unwrap().ref_text.as_deref(),
            Some("@s1:e1")
        );
    }
}
