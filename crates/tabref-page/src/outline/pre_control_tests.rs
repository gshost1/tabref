use std::time::{Duration, Instant};

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
    for query in ["see docs", "needle", "docs"] {
        let (hits, total) = search_text(&input, query, 20);
        assert_eq!(total, 1, "{query}");
        assert!(hits[0].line.starts_with("see docs for needle more"));
        assert_eq!(
            hits[0].control.as_ref().unwrap().ref_text.as_deref(),
            Some("@s1:e1")
        );
    }
}

#[test]
fn man_page_links_count_toward_the_pre_line_cap_without_duplicate_hits() {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    let mut links = Vec::new();
    for line in 1..=6 {
        dom.text(pre, &format!("{line}. See "), true);
        let link = dom.element(pre, "a", &[("href", "/open.2")], Some("inline"));
        dom.text(link, "open(2)", true);
        dom.text(pre, " for details.\n", true);
        links.push(link);
    }
    let mut input = input(dom);
    for link in links {
        reference(&mut input, link, "link", "open(2)");
    }
    let (hits, total) = search_text(&input, "open(2)", 20);
    assert_eq!(total, 6);
    assert_eq!(hits.len(), 4);
    for (line, hit) in hits[..3].iter().enumerate() {
        assert!(
            hit.line
                .starts_with(&format!("{}. See open(2) for details.", line + 1))
        );
        assert_eq!(
            hit.control.as_ref().unwrap().ref_text.as_deref(),
            Some("@s1:e1")
        );
    }
    assert_eq!(hits[3].line, "… 3 more in this block");
}

#[test]
fn a_button_in_a_matching_pre_line_is_not_a_second_hit() {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    let button = dom.element(
        pre,
        "button",
        &[("title", "Tooltip hint")],
        Some("inline-block"),
    );
    dom.text(button, "Retry", true);
    dom.text(pre, " the operation.\n", true);
    let mut input = input(dom);
    reference(&mut input, button, "button", "Try again");
    for (query, expected) in [
        ("Retry", "Retry the operation."),
        ("Tooltip hint", "Tooltip hint"),
    ] {
        let (hits, total) = search_text(&input, query, 20);
        assert_eq!(total, 1, "{query}");
        assert!(hits[0].line.starts_with(expected));
        assert_eq!(
            hits[0].control.as_ref().unwrap().ref_text.as_deref(),
            Some("@s1:e1")
        );
    }
}

#[test]
#[cfg_attr(debug_assertions, ignore = "release timing guard")]
fn large_referenced_code_wrappers_scale_with_the_number_of_lines() {
    let mut elapsed = Vec::new();
    for lines in [20_000, 40_000] {
        let mut dom = TestDom::new();
        let pre = dom.element(2, "pre", &[], Some("block"));
        let code = dom.element(pre, "code", &[], Some("inline"));
        for _ in 0..lines {
            for text in ["let ", "needle", " = 1;", "\n"] {
                dom.text(code, text, true);
            }
        }
        let mut input = input(dom);
        reference(&mut input, code, "button", "Copy");
        let start = Instant::now();
        let (hits, total) = search_text(&input, "needle", 20);
        let duration = start.elapsed();
        eprintln!("{lines} lines: {:.3} ms", duration.as_secs_f64() * 1000.0);
        assert_eq!(total, 4);
        assert_eq!(hits[3].line, format!("… {} more in this block", lines - 3));
        elapsed.push(duration);
    }
    assert!(elapsed[0] < Duration::from_millis(200), "{elapsed:?}");
    assert!(elapsed[1] < Duration::from_millis(400), "{elapsed:?}");
}
