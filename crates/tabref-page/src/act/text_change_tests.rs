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
    assert_eq!(changed("▲3 Books", "▲4 Books"), "▲{3 -> 4} Books");
    assert_eq!(
        changed("$9.99 total", "$19.99 total"),
        "${9 -> 19}.99 total"
    );
    assert_eq!(changed("Open", "Shut"), "{Open -> Shut}");
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
    let long = TextChange::Added("x".repeat(500)).to_string();
    assert!(long.ends_with('…'));
}

#[test]
fn an_added_message_of_a_few_hundred_characters_is_shown_whole() {
    let times: Vec<String> = (8..20)
        .flat_map(|hour| [format!("{hour}:15"), format!("{hour}:45")])
        .collect();
    let message = format!(
        "Dr. Mora is already booked at 9:30 AM on Dec 29, 2026. Free times that day: {}.",
        times.join(", ")
    );
    assert!(message.chars().count() > 200, "{message}");
    assert_eq!(
        TextChange::Added(message.clone()).to_string(),
        format!("+ {message}")
    );
}

#[test]
fn a_cut_line_ends_after_a_whole_word() {
    let line = "alpha beta gamma delta ".repeat(40);
    let shown = TextChange::Added(line.trim().to_owned()).to_string();
    assert!(shown.ends_with(" …"), "{shown}");
    let kept = shown.trim_start_matches("+ ").trim_end_matches(" …");
    assert!(line.starts_with(kept), "{shown}");
    assert!(
        ["alpha", "beta", "gamma", "delta"]
            .iter()
            .any(|word| kept.ends_with(word)),
        "{shown}"
    );
    let removed = TextChange::Removed(line.trim().to_owned()).to_string();
    assert!(removed.ends_with(" …"), "{removed}");
}
