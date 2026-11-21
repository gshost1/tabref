use serde_json::json;

use super::{TextChange, compare_replies};

fn replacement(before: &serde_json::Value, after: &serde_json::Value) -> Vec<(usize, Vec<String>)> {
    super::compare_watched(
        &super::text_blocks(before),
        &[],
        before["tables"].as_array().unwrap(),
        after,
    )
    .2
}

fn pair() -> (serde_json::Value, serde_json::Value) {
    let before = json!({"blocks":[[0,"Old-A\nOld-B"]],"tables":[{"watch":"epoch","block":0,"table":10,"body":0,"body_id":20,"rows":[[1,"Old-A"],[2,"Old-B"]]}]});
    let after = json!({"blocks":[[0,"New-A\nNew-B"]],"tables":[{"watch":"epoch","block":0,"table":10,"body":0,"body_id":21,"rows":[[3,"New-A"],[4,"New-B"]]}]});
    (before, after)
}

#[test]
fn replacing_the_body_element_or_emptying_it_is_detected() {
    let (before, mut after) = pair();
    assert_eq!(
        replacement(&before, &after),
        [(2, vec!["New-A".into(), "New-B".into()])]
    );
    after["tables"][0]["rows"] = json!([]);
    after["blocks"][0][1] = json!("");
    assert_eq!(replacement(&before, &after), [(2, vec![])]);
}

#[test]
fn retained_rows_text_and_identity_keep_the_existing_diff_path() {
    let (before, after) = pair();
    for (key, value) in [(0, json!(1)), (1, json!("Old-A"))] {
        let mut after = after.clone();
        after["tables"][0]["rows"][0][key] = value;
        assert!(replacement(&before, &after).is_empty());
    }
}

#[test]
fn reset_unreadable_malformed_and_ambiguous_metadata_are_conservative() {
    let (before, after) = pair();
    for (path, value) in [
        ("/tables/0/watch", json!(null)),
        ("/tables/0/block", json!(2)),
        ("/tables/0/rows/0/1", json!(null)),
        ("/blocks/0/1", json!(null)),
    ] {
        let mut after = after.clone();
        *after.pointer_mut(path).unwrap() = value;
        assert!(replacement(&before, &after).is_empty(), "{after}");
    }
    let mut before = before.clone();
    before["blocks"][0][1] = json!("Old-A\nOld-B\nOld-A\nOld-B");
    assert!(replacement(&before, &after).is_empty());
}

#[test]
fn an_existing_body_shifting_position_is_not_a_replacement() {
    let (mut before, mut after) = pair();
    let mut other = after["tables"][0].clone();
    other["body"] = json!(1);
    before["tables"].as_array_mut().unwrap().push(other);
    before["blocks"][0][1] = json!("Old-A\nOld-B\nNew-A\nNew-B");
    after["tables"][0]["body"] = json!(0);
    assert!(replacement(&before, &after).is_empty());
}

#[test]
fn whole_body_replacements_do_not_spend_changes_on_old_rows() {
    let before = json!({"blocks":[[0,"Old-A\nOld-B\nStatus idle"]],"tables":[{
        "watch":"epoch","block":0,"table":10,"body":0,"body_id":20,"rows":[[1,"Old-A"],[2,"Old-B"]]
    }]});
    let after = json!({"blocks":[[0,"New-A\nNew-B\nStatus saved"]],"tables":[{
        "watch":"epoch","block":0,"table":10,"body":0,"body_id":20,"rows":[[3,"New-A"],[4,"New-B"]]
    }]});
    let changes = compare_replies(&before, &after);
    assert!(
        !changes
            .iter()
            .any(|c| matches!(c,TextChange::Removed(row) if row.starts_with("Old-"))),
        "{changes:?}"
    );
    assert!(
        changes.contains(&TextChange::Added("New-A".into())),
        "{changes:?}"
    );
    assert!(
        changes.contains(&TextChange::Added("New-B".into())),
        "{changes:?}"
    );
    assert!(
        changes
            .iter()
            .any(|c| matches!(c,TextChange::Changed {after,..} if after=="Status saved")),
        "{changes:?}"
    );
}

#[test]
fn inserting_a_body_before_surviving_rows_is_not_a_replacement() {
    let (before, mut after) = pair();
    let mut surviving = before["tables"][0].clone();
    surviving["body"] = json!(1);
    after["tables"].as_array_mut().unwrap().push(surviving);
    after["blocks"][0][1] = json!("New-A\nNew-B\nOld-A\nOld-B");
    assert!(replacement(&before, &after).is_empty());
}
