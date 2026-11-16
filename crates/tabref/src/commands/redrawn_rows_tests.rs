use tabref_page::TextChange;

use super::redrawn;

fn found(names: &[&str]) -> Vec<(String, Option<String>)> {
    names
        .iter()
        .map(|name| (format!("[e1 button {name}]"), Some((*name).to_owned())))
        .collect()
}

#[test]
fn without_a_reorder_every_new_control_is_listed() {
    let (note, outside) = redrawn(&[TextChange::Added("Saved".into())], &found(&["Undo"]));
    assert_eq!(note, None);
    assert_eq!(outside, vec!["[e1 button Undo]".to_owned()]);
}

#[test]
fn row_controls_made_again_are_one_line_and_others_stay_listed() {
    let sorted = TextChange::Reordered {
        above: None,
        run: vec!["P-2 44 kg".into(), "P-1 12 kg".into()],
        table: true,
    };
    let (note, outside) = redrawn(&[sorted], &found(&["P-2", "P-1", "Undo sort"]));
    assert_eq!(
        note.as_deref(),
        Some("rows drawn again: 2 new refs, earlier refs in them are stale: tabref snap")
    );
    assert_eq!(outside, vec!["[e1 button Undo sort]".to_owned()]);
}
