use super::search_text;
use crate::outline::outline_input::OutlineInput;
use crate::outline::ref_label::RefLabel;
use crate::outline::test_dom::TestDom;

#[test]
fn pre_pointers_narrow_to_the_line_not_a_repeated_or_oversized_section() {
    let mut dom = TestDom::new();
    for number in 0..30 {
        let heading = dom.element(2, "h2", &[], Some("block"));
        dom.text(heading, "§ Examples", true);
        let pre = dom.element(2, "pre", &[], Some("block"));
        dom.text(
            pre,
            &format!("let mut vec = Vec::new();\nvec.push({number});\n"),
            true,
        );
    }
    let heading = dom.element(2, "h2", &[], Some("block"));
    dom.text(heading, "DESCRIPTION top", true);
    let pre = dom.element(2, "pre", &[], Some("block"));
    dom.text(
        pre,
        &format!(
            "{}\nSIGTARGET distinctive final line\n",
            "ordinary signal prose\n".repeat(2_000)
        ),
        true,
    );
    let input = input(dom);
    for (needle, target) in [
        ("push(11)", "vec.push(11);"),
        ("SIGTARGET", "SIGTARGET distinctive final line"),
    ] {
        let (hits, total) = search_text(&input, needle, 20);
        assert_eq!(total, 1);
        let suffix = format!(" (tabref find '{target}')");
        assert!(hits[0].line.ends_with(&suffix), "{}", hits[0].line);
        let (reached, total) = search_text(&input, target, 20);
        assert_eq!(total, 1);
        assert!(reached[0].line.contains(target));
    }
}

#[test]
fn pre_controls_keep_their_reference_beyond_the_block_line_cap() {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    dom.text(pre, &"widget btn in ordinary code\n".repeat(8), true);
    let button = dom.element(pre, "button", &[], Some("inline-block"));
    dom.text(button, "widget btn", true);
    let mut input = input(dom);
    input.refs.insert(
        ("T1".into(), i64::try_from(button).unwrap() + 1),
        RefLabel {
            ref_text: "@s1:e1".into(),
            role: "button".into(),
            name: Some("widget btn".into()),
            ..RefLabel::default()
        },
    );
    let (hits, total) = search_text(&input, "widget btn", 20);
    assert_eq!(total, 5);
    let control = hits
        .iter()
        .filter_map(|hit| hit.control.as_ref())
        .next()
        .expect("matching pre button must survive the block cap");
    assert_eq!(control.ref_text.as_deref(), Some("@s1:e1"));
    assert_eq!(control.name, "widget btn");
    assert!(
        hits.iter()
            .any(|hit| hit.line.starts_with("… 5 more in this block"))
    );
}

#[test]
fn pre_controls_keep_visible_text_and_attributes_when_the_accessible_name_differs() {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    let button = dom.element(
        pre,
        "button",
        &[("title", "Tooltip hint")],
        Some("inline-block"),
    );
    dom.text(button, "widget btn", true);
    let mut input = input(dom);
    input.refs.insert(
        ("T1".into(), i64::try_from(button).unwrap() + 1),
        RefLabel {
            ref_text: "@s1:e1".into(),
            role: "button".into(),
            name: Some("Accessible name".into()),
            ..RefLabel::default()
        },
    );
    for needle in ["widget", "Accessible", "Tooltip"] {
        let (hits, total) = search_text(&input, needle, 20);
        assert_eq!(total, 1, "{needle}");
        assert!(hits[0].line.contains(needle));
        assert_eq!(
            hits[0].control.as_ref().unwrap().ref_text.as_deref(),
            Some("@s1:e1")
        );
    }
}

fn input(dom: TestDom) -> OutlineInput {
    OutlineInput {
        trees: vec![dom.tree],
        ..OutlineInput::default()
    }
}

#[test]
fn hidden_text_is_found_with_its_section_and_the_control_that_shows_it() {
    let mut dom = TestDom::new();
    let heading = dom.element(2, "h2", &[], Some("block"));
    let button = dom.element(
        heading,
        "button",
        &[("aria-controls", "panel")],
        Some("inline-block"),
    );
    dom.text(button, "Economics", true);
    let panel = dom.element(2, "div", &[("id", "panel")], None);
    let item = dom.element(panel, "h3", &[], None);
    dom.text(item, "econ.EM (Econometrics)", false);
    let mut search = input(dom);
    let button_backend = i64::try_from(button).unwrap() + 1;
    search.refs.insert(
        ("T1".into(), button_backend),
        RefLabel {
            ref_text: "@s1:e8".into(),
            role: "button".into(),
            name: Some("Economics".into()),
            ..RefLabel::default()
        },
    );
    let (found, total) = search_text(&search, "ECON.", 20);
    assert_eq!(total, 1);
    assert!(found[0].hidden);
    assert_eq!(found[0].section, vec!["Economics".to_owned()]);
    assert_eq!(found[0].line, "econ.EM (Econometrics)");
    assert_eq!(
        found[0]
            .expander
            .as_ref()
            .and_then(|control| control.ref_text.as_deref()),
        Some("@s1:e8")
    );
}

#[test]
fn a_match_in_a_cell_reads_as_its_row_with_the_row_control() {
    let mut dom = TestDom::new();
    let table = dom.element(2, "table", &[], Some("table"));
    let row = dom.element(table, "tr", &[], Some("table-row"));
    for text in ["000165", "Complete"] {
        let cell = dom.element(row, "td", &[], Some("table-cell"));
        dom.text(cell, text, true);
    }
    let cell = dom.element(row, "td", &[], Some("table-cell"));
    let link = dom.element(cell, "a", &[("href", "/order/165")], Some("inline"));
    dom.text(link, "View", true);
    let (found, total) = search_text(&input(dom), "complete", 20);
    assert_eq!(total, 1);
    assert_eq!(found[0].line, "000165 Complete View");
    assert!(!found[0].hidden);
    assert_eq!(
        found[0]
            .control
            .as_ref()
            .map(|control| control.name.as_str()),
        None
    );
}

#[test]
fn plain_text_with_no_match_is_an_empty_result() {
    let mut dom = TestDom::new();
    dom.text(2, "nothing here", true);
    assert_eq!(search_text(&input(dom), "[v3]", 20), (Vec::new(), 0));
    assert_eq!(
        search_text(&OutlineInput::default(), " ", 20),
        (Vec::new(), 0)
    );
}

#[test]
fn a_visible_pane_inside_a_hidden_layer_is_not_hidden() {
    let mut dom = TestDom::new();
    let layer = dom.element(2, "div", &[], Some("block"));
    dom.hide(layer);
    let pane = dom.element(layer, "div", &[], Some("block"));
    dom.text(pane, "Subject of the draft", true);
    let (found, total) = search_text(&input(dom), "subject of", 20);
    assert_eq!(total, 1);
    assert!(!found[0].hidden);
    assert_eq!(found[0].expander, None);
}

#[test]
fn text_whose_own_element_is_not_visible_is_hidden() {
    let mut dom = TestDom::new();
    let layer = dom.element(2, "div", &[], Some("block"));
    let pane = dom.element(layer, "div", &[], Some("block"));
    dom.hide(pane);
    let text = dom.text(pane, "Subject of the draft", true);
    dom.hide(text);
    let (found, total) = search_text(&input(dom), "subject of", 20);
    assert_eq!(total, 1);
    assert!(found[0].hidden);
}

#[test]
fn man_page_pre_has_one_match_per_line_with_a_cap() {
    let mut dom = TestDom::new();
    let pre = dom.element(2, "pre", &[], Some("block"));
    dom.text(pre, "Architectures other than SPARC and MIPS\n", true);
    let signal = dom.element(pre, "b", &[], Some("inline"));
    dom.text(signal, "SIGWINCH", true);
    dom.text(pre, "   P2024 Ign Window resize signal\n", true);
    dom.text(pre, &"other signals\n".repeat(117), true);
    dom.text(pre, "Signal    x86  Alpha   MIPS  PARISC\n", true);
    dom.text(pre, "SIGWINCH    28     28     20      23\n", true);
    let search = input(dom);

    let (found, total) = search_text(&search, "SIGWINCH", 20);
    assert_eq!(total, 2);
    assert_eq!(
        found[0].line,
        "SIGWINCH P2024 Ign Window resize signal (tabref find 'SIGWINCH P2024 Ign Window resize signal')"
    );
    assert_eq!(
        found[1].line,
        "SIGWINCH 28 28 20 23 (tabref find 'SIGWINCH 28 28 20 23')"
    );

    let (found, total) = search_text(&search, "MIPS", 1);
    assert_eq!(total, 2);
    assert_eq!(found.len(), 1);
    assert_eq!(search_text(&search, "signal Signal", 20).1, 0);
}

fn rustdoc_method() -> OutlineInput {
    let mut dom = TestDom::new();
    let details = dom.element(2, "details", &[], Some("block"));
    let summary = dom.element(details, "summary", &[], Some("list-item"));
    let section = dom.element(summary, "section", &[("id", "method.unzip")], Some("block"));
    let rightside = dom.element(section, "span", &[("class", "rightside")], Some("block"));
    dom.text(rightside, "Other metadata", true);
    let since = dom.element(
        rightside,
        "span",
        &[
            ("class", "badge since"),
            ("title", "Stable since Rust version 1.66.0"),
        ],
        Some("inline"),
    );
    dom.text(since, "1.66.0", true);
    let heading = dom.element(section, "h4", &[], Some("block"));
    dom.text(heading, "pub fn unzip(self)", true);
    let closed = dom.element(details, "p", &[], None);
    dom.text(closed, "Hidden until expanded", false);
    input(dom)
}

#[test]
fn rustdoc_summary_preserves_the_heading_and_its_siblings() {
    use crate::outline::block::Block;
    use crate::outline::block_kind::BlockKind;
    use crate::outline::item::Item;

    fn contains(block: &Block, predicate: &impl Fn(&BlockKind, &str) -> bool) -> bool {
        block.items.iter().any(|item| match item {
            Item::Block(inner) => contains(inner, predicate),
            Item::Text(text) => predicate(&block.kind, text),
            _ => false,
        })
    }

    let outline = crate::build_outline(&rustdoc_method());
    assert!(contains(&outline.body, &|kind, text| {
        *kind == BlockKind::Heading(4) && text == "pub fn unzip(self)"
    }));
    assert!(contains(&outline.body, &|_, text| {
        text.contains("Other metadata") && text.contains("1.66.0")
    }));
    assert!(!contains(&outline.body, &|_, text| text
        .contains("Hidden until expanded")));
}

#[test]
fn rustdoc_signature_find_includes_its_version_badge() {
    let (found, total) = search_text(&rustdoc_method(), "fn unzip", 20);
    assert_eq!(total, 1);
    assert_eq!(found[0].line, "pub fn unzip(self) 1.66.0");
}

#[test]
fn heading_outside_a_summary_does_not_gain_sibling_badges() {
    let mut dom = TestDom::new();
    let heading = dom.element(2, "h4", &[], Some("block"));
    dom.text(heading, "pub fn unzip(self)", true);
    let since = dom.element(2, "span", &[("class", "since")], Some("inline"));
    dom.text(since, "1.66.0", true);
    let (found, total) = search_text(&input(dom), "fn unzip", 20);
    assert_eq!(total, 1);
    assert_eq!(found[0].line, "pub fn unzip(self)");
}

#[test]
fn chart_find_keeps_accessible_svg_labels_and_metadata_once_each() {
    let mut dom = TestDom::new();
    let caption = dom.element(2, "p", &[], Some("block"));
    dom.text(caption, "Electricity use (kWh) by month", true);
    let chart = dom.element(
        2,
        "svg",
        &[("role", "img"), ("aria-label", "Bar chart")],
        Some("inline"),
    );
    let bar = dom.element(
        chart,
        "rect",
        &[("role", "img"), ("aria-label", "March 2024: 45,371 kWh")],
        Some("inline"),
    );
    let title = dom.element(bar, "title", &[], None);
    dom.text(title, "March 2024: 45,371 kWh", false);
    let titled = dom.element(2, "svg", &[], Some("inline"));
    let title = dom.element(titled, "title", &[], None);
    dom.text(title, "April 2024: 46,123 kWh", false);
    let desc = dom.element(titled, "desc", &[], None);
    dom.text(desc, "April 2024 kWh are estimates", false);
    let decorative = dom.element(2, "svg", &[], Some("inline"));
    let text = dom.element(decorative, "text", &[], Some("inline"));
    dom.text(text, "Decorative kWh", true);
    let hidden = dom.element(
        2,
        "svg",
        &[
            ("role", "img"),
            ("aria-hidden", "true"),
            ("title", "Hidden kWh"),
        ],
        Some("inline"),
    );
    dom.element(hidden, "path", &[], Some("inline"));

    let (found, total) = search_text(&input(dom), "kWh", 20);
    assert_eq!(total, 3);
    assert_eq!(found[0].line, "Electricity use (kWh) by month");
    assert_eq!(found[1].line, "March 2024: 45,371 kWh");
    assert_eq!(
        found[2].line,
        "April 2024: 46,123 kWh April 2024 kWh are estimates"
    );
    assert!(found.iter().all(|item| !item.hidden));
}
