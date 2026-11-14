use super::TextChange;

fn changed(before: &str, after: &str) -> String {
    TextChange::Changed {
        before: before.into(),
        after: after.into(),
    }
    .to_string()
}

#[test]
fn a_rewritten_line_shows_the_changed_words_in_their_context() {
    assert_eq!(
        changed("▲ 3 ▼ Books set in winter", "▲ 4 ▼ Books set in winter"),
        "▲ {3 -> 4} ▼ Books set in winter"
    );
    assert_eq!(
        changed("Total: 42 EUR", "Total: 47 EUR"),
        "Total: {42 -> 47} EUR"
    );
    assert_eq!(changed("Waiting", "Sent ada"), "{Waiting -> Sent ada}");
}

#[test]
fn a_line_that_only_grew_shows_an_empty_side() {
    assert_eq!(changed("Items", "Items 3"), "Items {-> 3}");
}

#[test]
fn a_long_rewrite_shows_both_lines_cut() {
    let before = "a ".repeat(80);
    let after = "b ".repeat(80);
    let shown = changed(before.trim(), after.trim());
    assert!(shown.contains(" -> "), "{shown}");
    assert!(shown.chars().count() < 120, "{shown}");
}

#[test]
fn added_and_removed_lines_are_marked() {
    assert_eq!(TextChange::Added("Post".into()).to_string(), "+ Post");
    assert_eq!(TextChange::Removed("Old".into()).to_string(), "- Old");
    let long = TextChange::Added("x".repeat(300)).to_string();
    assert!(long.ends_with('…'));
}
