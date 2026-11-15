use std::sync::atomic::{AtomicI64, Ordering};

use tabref_core::{AccessibilityNode, ErrorCode};

use super::locate;
use crate::locate::locate_intent::LocateIntent;
use crate::locate::locator::Locator;
use crate::locate::word::Word;

static NEXT: AtomicI64 = AtomicI64::new(1);

fn node(role: &str, name: &str, children: Vec<AccessibilityNode>) -> AccessibilityNode {
    let mut node = AccessibilityNode {
        role: role.into(),
        children,
        ..AccessibilityNode::default()
    };
    if !name.is_empty() {
        node.identity.name = Some(name.into());
    }
    node
}

fn control(role: &str, name: &str) -> AccessibilityNode {
    let mut node = node(role, name, Vec::new());
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    node.identity.retained_object = Some(format!("cdp:T:F:L:{id}"));
    node
}

fn text(name: &str) -> AccessibilityNode {
    node("statictext", name, Vec::new())
}

fn row(name: &str, cells: Vec<AccessibilityNode>) -> AccessibilityNode {
    node("row", name, cells)
}

fn grid() -> AccessibilityNode {
    let product = |name: &str| {
        row(
            "",
            vec![
                node("group", "", vec![text(name)]),
                node("group", "", vec![text("$67.00")]),
                node("group", "", vec![control("link", "Edit")]),
            ],
        )
    };
    node(
        "webarea",
        "Products",
        vec![
            control("textfield", "Search by keyword"),
            control("button", "Search"),
            node(
                "table",
                "",
                vec![
                    row("", vec![control("cell", "Price")]),
                    product("Hoodie-XS-Blue"),
                    product("Hoodie-S-Blue"),
                    product("Hoodie-M-Blue"),
                ],
            ),
        ],
    )
}

fn find(
    tree: &AccessibilityNode,
    words: &[Word],
    intent: LocateIntent,
) -> Result<String, ErrorCode> {
    let locator = Locator::from_words(words).unwrap();
    locate(tree, &locator, intent)
        .map(|found| format!("{} {}", found.role, found.name.unwrap_or_default()))
        .map_err(|error| error.code)
}

#[test]
fn a_field_is_found_by_the_start_of_its_label() {
    let form = node(
        "webarea",
        "",
        vec![
            control("cell", "Price"),
            control("textfield", "Product Name"),
            control("textfield", "Price ($)"),
            control("button", "Save"),
        ],
    );
    assert_eq!(
        find(&form, &[Word::quoted("Price")], LocateIntent::Fill),
        Ok("textfield Price ($)".into())
    );
    assert_eq!(
        find(&form, &[Word::quoted("save")], LocateIntent::Click),
        Ok("button Save".into())
    );
}

#[test]
fn several_matches_are_ambiguous_until_a_row_or_nth_picks_one() {
    let tree = grid();
    assert_eq!(
        find(&tree, &[Word::quoted("Edit")], LocateIntent::Click),
        Err(ErrorCode::AmbiguousTarget)
    );
    let in_row = [
        Word::quoted("Edit"),
        Word::bare("in"),
        Word::quoted("Hoodie-S-Blue"),
    ];
    let locator = Locator::from_words(&in_row).unwrap();
    let found = locate(&tree, &locator, LocateIntent::Click).unwrap();
    let all = [Word::quoted("Edit"), Word::bare("nth"), Word::bare("2")];
    let second = locate(
        &tree,
        &Locator::from_words(&all).unwrap(),
        LocateIntent::Click,
    )
    .unwrap();
    assert_eq!(found.token, second.token);
}

#[test]
fn a_row_without_the_control_finds_nothing_rather_than_a_neighbour() {
    let tree = grid();
    let words = [
        Word::quoted("Search"),
        Word::bare("in"),
        Word::quoted("Hoodie-S-Blue"),
    ];
    assert_eq!(
        find(&tree, &words, LocateIntent::Click),
        Err(ErrorCode::ElementNotFound)
    );
    let missing = [
        Word::quoted("Edit"),
        Word::bare("in"),
        Word::quoted("No such row"),
    ];
    assert_eq!(
        find(&tree, &missing, LocateIntent::Click),
        Err(ErrorCode::ElementNotFound)
    );
}

#[test]
fn a_miss_names_the_closest_controls() {
    let tree = grid();
    let locator = Locator::from_words(&[Word::quoted("Serch")]).unwrap();
    let error = locate(&tree, &locator, LocateIntent::Click).unwrap_err();
    assert_eq!(error.code, ErrorCode::ElementNotFound);
    assert!(
        error.message.contains(r#"button "Search""#),
        "{}",
        error.message
    );
}

#[test]
fn a_button_is_found_by_its_description_near_a_title() {
    let post = |title: &str| {
        let mut up = control("button", "▲");
        up.identity.description = Some("Upvote".into());
        row(
            "",
            vec![
                node("group", "", vec![up, text("3")]),
                node("group", "", vec![control("link", title)]),
            ],
        )
    };
    let tree = node(
        "webarea",
        "",
        vec![node(
            "table",
            "",
            vec![post("Books set in winter"), post("Long series")],
        )],
    );
    let words = [
        Word::quoted("Upvote"),
        Word::bare("in"),
        Word::quoted("Books set in winter"),
    ];
    assert_eq!(
        find(&tree, &words, LocateIntent::Click),
        Ok("button ▲".into())
    );
}

#[test]
fn the_control_nearest_the_text_wins_inside_nested_comments() {
    let comment = |body: &str, replies: Vec<AccessibilityNode>| {
        let mut children = vec![
            node("paragraph", body, vec![text(body)]),
            control("button", "Reply"),
        ];
        children.extend(replies);
        node("group", "", children)
    };
    let tree = node(
        "webarea",
        "",
        vec![comment(
            "I run this site",
            vec![comment("Great news", Vec::new())],
        )],
    );
    let near_mod = [
        Word::quoted("Reply"),
        Word::bare("in"),
        Word::quoted("I run this site"),
    ];
    let near_reply = [
        Word::quoted("Reply"),
        Word::bare("in"),
        Word::quoted("Great news"),
    ];
    let first = locate(
        &tree,
        &Locator::from_words(&near_mod).unwrap(),
        LocateIntent::Click,
    )
    .unwrap();
    let second = locate(
        &tree,
        &Locator::from_words(&near_reply).unwrap(),
        LocateIntent::Click,
    )
    .unwrap();
    assert_ne!(first.token, second.token);
}

#[test]
fn text_the_page_shows_stands_for_the_control_around_it() {
    let tree = node(
        "webarea",
        "",
        vec![{
            let mut link = control("link", "");
            link.children = vec![text("Open the report")];
            link
        }],
    );
    assert_eq!(
        find(
            &tree,
            &[Word::quoted("Open the report")],
            LocateIntent::Click
        ),
        Ok("link ".into())
    );
    assert_eq!(
        find(
            &tree,
            &[Word::quoted("Open the report")],
            LocateIntent::Fill
        ),
        Err(ErrorCode::ElementNotFound)
    );
}

#[test]
fn a_role_alone_finds_the_only_control_of_that_role() {
    let tree = node(
        "webarea",
        "",
        vec![control("textfield", ""), control("button", "Post")],
    );
    assert_eq!(
        find(&tree, &[Word::bare("textbox")], LocateIntent::Fill),
        Ok("textfield ".into())
    );
}

#[test]
fn text_in_no_row_leads_to_the_first_match_after_it() {
    let tree = node(
        "webarea",
        "",
        vec![
            node("paragraph", "Does anyone know?", Vec::new()),
            control("button", "Reply"),
            node("paragraph", "I run this site", Vec::new()),
            control("button", "Reply"),
            node("paragraph", "Great news", Vec::new()),
            control("button", "Reply"),
        ],
    );
    let words = [Word::quoted("Reply"), Word::bare("in"), Word::quoted("I run this site")];
    let found = locate(&tree, &Locator::from_words(&words).unwrap(), LocateIntent::Click).unwrap();
    let nth = [Word::quoted("Reply"), Word::bare("nth"), Word::bare("2")];
    let second = locate(&tree, &Locator::from_words(&nth).unwrap(), LocateIntent::Click).unwrap();
    assert_eq!(found.token, second.token);
}
