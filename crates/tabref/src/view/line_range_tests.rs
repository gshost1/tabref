use std::collections::HashMap;

use tabref_page::PageInfo;

use super::page_view;
use super::tests::{Dom, input, snap};
use crate::view::view_options::ViewOptions;
use crate::view::view_request::ViewRequest;
use crate::view::view_route::ViewRoute;
use crate::view::view_scope::ViewScope;
use crate::view::view_subject::ViewSubject;

fn render(
    dom: Dom,
    refs: HashMap<(String, i64), tabref_page::RefLabel>,
    options: &ViewOptions,
) -> String {
    let info = PageInfo {
        url: "http://site.test/page".into(),
        title: "T".into(),
    };
    let subject = ViewSubject {
        info: &info,
        snapshot_id: Some("s1"),
        ref_count: 0,
        skipped_frames: &[],
    };
    page_view(&subject, &input(dom, refs), options, &ViewScope::Page)
        .unwrap()
        .text
}

fn paragraphs(count: usize) -> Dom {
    let mut dom = Dom::new();
    for i in 1..=count {
        let p = dom.el(2, "p", &[], Some("block"));
        dom.text(p, &format!("Paragraph number {i:03}."), true);
    }
    dom
}

fn range(from: usize, to: usize) -> ViewOptions {
    ViewOptions {
        route: ViewRoute::new("tabref snap", from).ending_at(Some(to)),
        ..snap()
    }
}

#[test]
fn to_stops_the_view_after_that_line_and_says_how_to_go_on() {
    let out = render(paragraphs(30), HashMap::new(), &range(1, 5));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 7, "{out}");
    assert_eq!(lines[1], "Paragraph number 001.");
    assert_eq!(lines[5], "Paragraph number 005.");
    let footer = lines[6];
    assert!(footer.contains("lines 6- after --to"), "{footer}");
    assert!(footer.contains("tabref snap --from 6"), "{footer}");
    assert!(!footer.contains("all shown"), "{footer}");
}

#[test]
fn from_and_to_print_the_lines_between_them_inclusive() {
    let out = render(paragraphs(30), HashMap::new(), &range(11, 13));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines[1..4],
        [
            "Paragraph number 011.",
            "Paragraph number 012.",
            "Paragraph number 013."
        ],
        "{out}"
    );
    assert!(lines[4].contains("tabref snap --from 14"), "{out}");
}

#[test]
fn to_at_or_past_the_last_line_leaves_nothing_out() {
    for to in [30, 500] {
        let out = render(paragraphs(30), HashMap::new(), &range(1, to));
        assert_eq!(out.lines().count(), 32, "{out}");
        assert!(out.ends_with("all shown"), "{out}");
    }
}

#[test]
fn to_before_from_is_refused_with_the_form_to_use() {
    let request = ViewRequest {
        from: 20,
        to: Some(10),
        ..ViewRequest::default()
    };
    let error = request.snap_options("http://site.test/").unwrap_err();
    assert!(error.message.contains("--to 10 is before --from 20"));
    assert!(request.read_options("http://site.test/").is_err());
    let same = ViewRequest {
        from: 7,
        to: Some(7),
        ..ViewRequest::default()
    };
    assert_eq!(
        same.snap_options("http://site.test/").unwrap().route.to,
        Some(7)
    );
}
