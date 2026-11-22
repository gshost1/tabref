use tabref_core::AccessibilityNode;
use tabref_page::{OutlineInput, RefLabel, build_outline, search_text};

use super::{ax, merged_matches, test_dom::TestDom};
use crate::view::block_renderer::BlockRenderer;
use crate::view::find_view::find_lines_with_ax;
use crate::view::ref_style::RefStyle;
use crate::view::row_window::RowWindow;
use crate::view::section::section_lines;
use crate::view::view_options::ViewOptions;

fn input(dom: TestDom) -> OutlineInput {
    OutlineInput {
        trees: vec![dom.tree],
        ..OutlineInput::default()
    }
}

fn options() -> ViewOptions {
    let mut options = ViewOptions::snap("http://site.test/page", false, RowWindow::default());
    options.refs = RefStyle::Short;
    options
}

fn reference(input: &mut OutlineInput, index: usize, number: usize, role: &str, name: &str) {
    input.refs.insert(
        ("T1".into(), i64::try_from(index).unwrap() + 1),
        RefLabel {
            ref_text: format!("@s1:e{number}"),
            role: role.into(),
            name: Some(name.into()),
            ..RefLabel::default()
        },
    );
}

#[test]
fn a_long_pre_leaves_room_for_later_sections_golden() {
    let mut dom = TestDom::new();
    let heading = dom.element(2, "h2", &[], Some("block"));
    dom.text(heading, "Example 0", true);
    let pre = dom.element(2, "pre", &[], Some("block"));
    dom.text(pre, &"let value = 1;\n".repeat(100), true);
    let heading = dom.element(2, "h2", &[], Some("block"));
    dom.text(heading, "Summary", true);
    let prose = dom.element(2, "p", &[], Some("block"));
    dom.text(prose, "We let the later prose stay visible.", true);
    let (hits, total) = search_text(&input(dom), "let", 20);
    assert_eq!(total, 5);
    assert_eq!(
        find_lines_with_ax("let", &hits, &[], total, &options()).join("\n"),
        "5 matches for \"let\"\n- Example 0 | let value = 1; (tabref read --section \"Example 0\")\n- Example 0 | let value = 1; (tabref read --section \"Example 0\")\n- Example 0 | let value = 1; (tabref read --section \"Example 0\")\n- Example 0 | … 97 more in this block (tabref read --section \"Example 0\")\n- Summary | We let the later prose stay visible."
    );
}

#[test]
fn pre_lines_use_their_own_visibility_and_do_not_borrow_links() {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    let link = dom.element(pre, "a", &[("href", "/unrelated")], Some("inline"));
    dom.text(link, "unrelated\n", true);
    let hidden = dom.element(pre, "span", &[], None);
    dom.text(hidden, "install hidden\n", false);
    dom.text(pre, "install visible\n", true);
    let split = dom.element(pre, "b", &[], Some("inline"));
    dom.text(split, "in", true);
    dom.text(pre, "stall split\n", true);
    let (hits, total) = search_text(&input(dom), "install", 20);
    assert_eq!(total, 3);
    assert!(hits[0].hidden);
    assert!(!hits[1].hidden && !hits[2].hidden);
    assert!(hits.iter().all(|hit| hit.control.is_none()));
    assert_eq!(hits[1].line, "install visible (tabref snap --full)");
    assert_eq!(hits[2].line, "install split (tabref snap --full)");
}

#[test]
fn svg_icon_matches_keep_the_enclosing_link_reference() {
    let mut dom = TestDom::new();
    let link = dom.element(2, "a", &[("href", "/search")], Some("inline"));
    let svg = dom.element(link, "svg", &[], Some("inline"));
    let title = dom.element(svg, "title", &[], None);
    dom.text(title, "search icon", false);
    let mut input = input(dom);
    reference(&mut input, link, 1, "link", "search icon");
    let tree = ax("search icon", Some("@s1:e1"));
    let (hits, total, _) = merged_matches(&input, &tree, "search", 20);
    assert_eq!(total, 1);
    assert_eq!(
        hits[0].control.as_ref().unwrap().ref_text.as_deref(),
        Some("@s1:e1")
    );
    assert_eq!(
        hits[0].control.as_ref().unwrap().href.as_deref(),
        Some("http://site.test/search")
    );
}

#[test]
fn interactive_ax_hits_inside_matched_blocks_and_labels_are_not_recounted() {
    for label_attribute in ["for", "aria-labelledby"] {
        let mut dom = TestDom::new();
        let p = dom.element(2, "p", &[], Some("block"));
        dom.text(p, "Read ", true);
        let link = dom.element(p, "a", &[("href", "/i")], Some("inline"));
        dom.text(link, "the ", true);
        let bold = dom.element(link, "b", &[], Some("inline"));
        dom.text(bold, "install", true);
        dom.text(link, " guide", true);
        let label_attrs = if label_attribute == "for" {
            vec![("for", "x")]
        } else {
            vec![("id", "path-label")]
        };
        let label = dom.element(2, "label", &label_attrs, Some("inline"));
        dom.text(label, "install path", true);
        let attrs = if label_attribute == "for" {
            vec![("id", "x"), ("value", "install")]
        } else {
            vec![
                ("id", "x"),
                ("value", "install"),
                ("aria-labelledby", "path-label"),
            ]
        };
        let field = dom.element(2, "input", &attrs, Some("inline-block"));
        let mut input = input(dom);
        reference(&mut input, link, 1, "link", "the install guide");
        reference(&mut input, field, 2, "textfield", "install path");
        let tree = AccessibilityNode {
            children: vec![
                ax("the install guide", Some("@s1:e1")),
                ax("install path", Some("@s1:e2")),
            ],
            ..AccessibilityNode::default()
        };
        let (hits, total, _) = merged_matches(&input, &tree, "install", 20);
        assert_eq!(total, 2, "{label_attribute}: {hits:?}");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].line, "Read the install guide");
        assert_eq!(hits[1].line, "install path");
    }
}

#[test]
fn a_matched_label_block_covers_its_separately_positioned_ax_control() {
    for aria in [false, true] {
        let mut dom = TestDom::new();
        let attrs = if aria {
            vec![("id", "name")]
        } else {
            vec![("for", "x")]
        };
        let label = dom.element(2, "label", &attrs, Some("block"));
        dom.text(label, "install path", true);
        let spacer = dom.element(2, "p", &[], Some("block"));
        dom.text(spacer, "Not part of the label", true);
        let attrs = if aria {
            vec![("id", "x"), ("aria-labelledby", "name")]
        } else {
            vec![("id", "x")]
        };
        let field = dom.element(2, "input", &attrs, Some("inline-block"));
        let mut input = input(dom);
        reference(&mut input, field, 1, "textfield", "install path");
        let tree = ax("install path", Some("@s1:e1"));
        let (hits, total, _) = merged_matches(&input, &tree, "install", 20);
        assert_eq!(total, 1, "{aria}: {hits:?}");
        assert_eq!(hits.len(), 1);
    }
}

#[test]
fn referenced_ax_only_hit_is_interleaved_between_dom_hits() {
    let mut dom = TestDom::new();
    let first = dom.element(2, "p", &[], Some("block"));
    dom.text(first, "install first", true);
    let button = dom.element(2, "button", &[], Some("inline-block"));
    dom.text(button, "Continue", true);
    let last = dom.element(2, "p", &[], Some("block"));
    dom.text(last, "install last", true);
    let mut input = input(dom);
    reference(&mut input, button, 1, "button", "Continue");
    let tree = AccessibilityNode {
        ref_id: Some("@s1:e1".into()),
        children: vec![ax("install middle", None)],
        ..AccessibilityNode::default()
    };
    let (hits, total, _) = merged_matches(&input, &tree, "install", 20);
    assert_eq!(total, 3);
    assert_eq!(
        find_lines_with_ax("install", &hits, &[], total, &options()),
        vec![
            "3 matches for \"install\"",
            "- install first",
            "- install middle [e1 button Continue]",
            "- install last",
        ]
    );
    assert_eq!(
        hits.iter().map(|hit| hit.line.as_str()).collect::<Vec<_>>(),
        vec!["install first", "install middle", "install last"]
    );
    assert_eq!(
        hits[1].control.as_ref().unwrap().ref_text.as_deref(),
        Some("@s1:e1")
    );
    let (hits, total, ax_only) = merged_matches(&input, &tree, "middle", 20);
    assert_eq!(total, 1);
    assert!(ax_only);
    assert!(hits[0].line.contains("image \"install middle\" in @s1:e1"));
}

#[test]
fn ax_description_outside_a_matched_inline_source_is_retained() {
    let mut dom = TestDom::new();
    let caption = dom.element(2, "p", &[], Some("block"));
    dom.text(caption, "Electricity use in kWh", true);
    let units = dom.element(2, "span", &[("id", "units")], None);
    dom.text(units, "Monthly totals in kWh", false);
    let button = dom.element(2, "button", &[], Some("inline-block"));
    dom.text(button, "Totals", true);
    let mut input = input(dom);
    reference(&mut input, button, 1, "button", "Totals");
    let mut tree = ax("Totals", Some("@s1:e1"));
    tree.role = "button".into();
    tree.identity.description = Some("Yearly total in kWh".into());
    let (hits, total, _) = merged_matches(&input, &tree, "kWh", 20);
    assert_eq!(total, 3, "{hits:?}");
    assert!(hits.iter().any(|hit| hit.line == "Yearly total in kWh"));
}

#[test]
fn svg_reference_cache_distinguishes_documents_on_the_same_target() {
    let mut input = OutlineInput::default();
    let mut children = Vec::new();
    for (number, name) in [(1, "install first"), (2, "install second")] {
        let mut dom = TestDom::new();
        let link = dom.element(2, "a", &[("href", "/chart")], Some("inline"));
        let svg = dom.element(link, "svg", &[], Some("inline"));
        dom.element(
            svg,
            "rect",
            &[("role", "img"), ("aria-label", name)],
            Some("inline"),
        );
        for node in &mut dom.tree.nodes {
            node.backend_id += number * 100;
        }
        let reference = format!("@s1:e{number}");
        input.refs.insert(
            ("T1".into(), dom.tree.nodes[link].backend_id),
            RefLabel {
                ref_text: reference.clone(),
                ..RefLabel::default()
            },
        );
        input.trees.push(dom.tree);
        children.push(ax(name, Some(&reference)));
    }
    let tree = AccessibilityNode {
        children,
        ..AccessibilityNode::default()
    };
    let (hits, total, ax_only) = merged_matches(&input, &tree, "install", 20);
    assert!(!ax_only);
    assert_eq!(total, 2, "{hits:?}");
    assert_eq!(hits.len(), 2);
}

#[test]
fn heading_only_summary_and_rustdoc_summary_have_distinct_golden_views() {
    let mut dom = TestDom::new();
    let details = dom.element(2, "details", &[], Some("block"));
    let summary = dom.element(details, "summary", &[], Some("list-item"));
    let heading = dom.element(summary, "h3", &[], Some("block"));
    dom.text(heading, "How do I configure option 0?", true);
    let mut input = input(dom);
    let options = options();
    let render = |input: &OutlineInput| {
        let outline = build_outline(input);
        let mut renderer = BlockRenderer::new(&options);
        renderer.render_children(&outline.body, 0);
        renderer.lines
    };
    assert_eq!(
        render(&input)
            .iter()
            .map(|line| line.printed())
            .collect::<Vec<_>>(),
        vec!["[disclosure How do I configure option 0? (collapsed)]"]
    );
    let mut dom = TestDom::new();
    let details = dom.element(2, "details", &[], Some("block"));
    let summary = dom.element(details, "summary", &[], Some("list-item"));
    let section = dom.element(summary, "section", &[], Some("block"));
    let badge = dom.element(section, "span", &[("class", "since")], Some("inline"));
    dom.text(badge, "1.66.0", true);
    let heading = dom.element(section, "h4", &[], Some("block"));
    dom.text(heading, "pub fn unzip(self)", true);
    input.trees[0] = dom.tree;
    let lines = render(&input);
    assert_eq!(
        lines.iter().map(|line| line.printed()).collect::<Vec<_>>(),
        vec![
            "[disclosure (collapsed)]",
            "1.66.0",
            "#### pub fn unzip(self)"
        ]
    );
    let section = section_lines(lines, "pub fn unzip(self)").unwrap();
    assert_eq!(section[0].heading_text(), "pub fn unzip(self)");
}

#[test]
fn deeply_nested_zero_hit_search_has_a_linear_scan_budget() {
    let mut dom = TestDom::new();
    let mut parent = 2;
    for _ in 0..1800 {
        parent = dom.element(parent, "div", &[], Some("block"));
    }
    dom.text(parent, "nothing matches", true);
    let input = input(dom);
    let start = std::time::Instant::now();
    let (hits, total) = search_text(&input, "absent", 20);
    assert!(hits.is_empty());
    assert_eq!(total, 0);
    assert!(start.elapsed() < std::time::Duration::from_secs(2));
}
