use serde_json::json;

use super::super::page_changes::ReplacedRows;
use super::{TextChange, compare_replies};

fn replacement(before: &serde_json::Value, after: &serde_json::Value) -> ReplacedRows {
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
        [(2, vec![(3, "New-A".into()), (4, "New-B".into())])]
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
        ("/tables/0/rows", json!(null)),
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

#[test]
fn refreshed_rows_with_matching_keys_keep_status_edits() {
    let (mut before, mut after) = pair();
    for (reply, offset, stamp) in [(&mut before, 0, "0s"), (&mut after, 100, "1s")] {
        let rows: Vec<_> = (0..10)
            .map(|n| {
                let status = if offset > 0 && n == 7 {
                    "revoked"
                } else {
                    "active"
                };
                json!([
                    offset + n + 1,
                    format!("TOK-{}\t{status}\tchecked {stamp} ago", 100 + n),
                    format!("TOK-{}", 100 + n),
                    ""
                ])
            })
            .collect();
        reply["blocks"][0][1] = json!(
            rows.iter()
                .map(|r| r[1].as_str().unwrap())
                .collect::<Vec<_>>()
                .join("\n")
        );
        reply["tables"][0]["rows"] = json!(rows);
    }
    assert!(replacement(&before, &after).is_empty());
    assert!(compare_replies(&before, &after).iter().any(|c| matches!(c, TextChange::Changed { before, after } if before.contains("TOK-107") && after.contains("revoked"))));
}

#[test]
fn row_ids_pair_edits_when_first_cells_change() {
    let (mut before, mut after) = pair();
    for (reply, start, label) in [(&mut before, 1, "Old"), (&mut after, 3, "New")] {
        for n in 0..2 {
            reply["tables"][0]["rows"][n] = json!([
                start + n,
                format!("{label}-{}", if n == 0 { "A" } else { "B" }),
                format!("{label}-{n}"),
                format!("key-{n}")
            ]);
        }
    }
    assert!(replacement(&before, &after).is_empty());
}

#[test]
fn a_minority_of_shared_keys_still_allows_a_new_page_summary() {
    let (mut before, mut after) = pair();
    before["tables"][0]["rows"][0]
        .as_array_mut()
        .unwrap()
        .push(json!("Shared key"));
    after["tables"][0]["rows"][0]
        .as_array_mut()
        .unwrap()
        .push(json!("Shared key"));
    assert_eq!(replacement(&before, &after).len(), 1);
}

#[test]
fn ambiguous_shared_keys_keep_the_normal_diff() {
    let (mut before, mut after) = pair();
    for reply in [&mut before, &mut after] {
        for n in 0..2 {
            reply["tables"][0]["rows"][n]
                .as_array_mut()
                .unwrap()
                .push(json!("Shared key"));
        }
    }
    assert!(replacement(&before, &after).is_empty());
}

#[test]
fn checkbox_first_cells_do_not_hide_the_first_text_key() {
    let (mut before, mut after) = pair();
    for (reply, offset, stamp) in [(&mut before, 0, "0s"), (&mut after, 100, "1s")] {
        let rows: Vec<_> = (0..10)
            .map(|n| {
                let status = if offset > 0 && n == 7 {
                    "revoked"
                } else {
                    "active"
                };
                json!([
                    offset + n + 1,
                    format!("\tTOK-{}\t{status}\tchecked {stamp} ago", 100 + n),
                    "",
                    ""
                ])
            })
            .collect();
        reply["blocks"][0][1] = json!(
            rows.iter()
                .map(|row| row[1].as_str().unwrap())
                .collect::<Vec<_>>()
                .join("\n")
        );
        reply["tables"][0]["rows"] = json!(rows);
    }
    assert!(replacement(&before, &after).is_empty());
    assert!(compare_replies(&before,&after).iter().any(|change| matches!(change,TextChange::Changed {before,after} if before.contains("TOK-107") && after.contains("revoked"))));
}
