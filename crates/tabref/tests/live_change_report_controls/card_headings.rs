use super::{live, new_ref, site};

#[test]
fn flat_cards_use_their_own_preceding_headings() {
    assert_card_refs("Reveal cards", &["Card A", "Card B"]);
}

#[test]
fn flat_button_first_layout_labels_two_cards() {
    assert_card_refs("Reveal two button-first cards", &["Card A", "Card B"]);
}

#[test]
fn flat_button_first_layout_labels_three_cards() {
    assert_card_refs(
        "Reveal three button-first cards",
        &["Card A", "Card B", "Card C"],
    );
}

#[test]
fn flat_cards_omit_labels_when_the_order_is_ambiguous() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/review")]);
    let text = tabref.ok(&["click", "Reveal ambiguous cards"]).stdout;
    let actions: Vec<_> = text
        .lines()
        .filter(|line| line.starts_with("new ") && line.contains("button \"Actions\""))
        .collect();
    assert_eq!(actions.len(), 3, "{text}");
    assert!(
        actions
            .iter()
            .all(|line| line.ends_with("button \"Actions\"")),
        "{text}"
    );
}

fn assert_card_refs(reveal: &str, headings: &[&str]) {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/review")]);
    let text = tabref.ok(&["click", reveal]).stdout;
    for heading in headings {
        let reference = new_ref(&text, &format!("button \"Actions\" in \"{heading}\""));
        tabref.ok(&["click", &reference]);
        let selected = tabref
            .ok(&[
                "--json",
                "js",
                "document.getElementById('status').textContent",
            ])
            .json();
        assert!(
            selected["data"]["value"] == format!("Selected {heading}"),
            "{text}\n{selected}"
        );
    }
}
