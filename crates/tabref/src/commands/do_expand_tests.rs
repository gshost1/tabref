use tabref_core::ErrorCode;

use super::expand;
use crate::commands::step_words::split_steps;

fn sources(text: &str) -> Vec<String> {
    expand(&split_steps(text).unwrap())
        .unwrap()
        .into_iter()
        .map(|step| step.source)
        .collect()
}

#[test]
fn a_loop_writes_out_its_body_once_per_round_of_values() {
    assert_eq!(
        sources(
            "for id price in 4499 62.00 5916 64.00\n  open /p/{id}/edit\n  fill \"Price\" {price}\nend\nclick Done"
        ),
        [
            "open /p/4499/edit",
            "fill \"Price\" 62.00",
            "open /p/5916/edit",
            "fill \"Price\" 64.00",
            "click Done",
        ]
    );
}

#[test]
fn values_are_put_in_once_and_never_become_keywords_or_refs() {
    assert_eq!(
        sources("for a b in \"{b}\" X\nread {a}|{b}\nend"),
        ["read {b}|X"]
    );
    let steps = expand(&split_steps("for t in e4\nclick {t}\nend").unwrap()).unwrap();
    assert!(steps[0].words[1].quoted);
}

#[test]
fn loops_nest() {
    assert_eq!(
        sources("for a in 1 2\nfor b in x\nread {a}{b}\nend\nend"),
        ["read 1x", "read 2x"]
    );
}

#[test]
fn a_loop_without_an_end_or_with_odd_values_is_refused() {
    for text in [
        "for id in 1 2\nopen /{id}",
        "for a b in 1 2 3\nend",
        "end",
        "for in 1\nend",
    ] {
        let error = expand(&split_steps(text).unwrap()).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgs, "{text}");
    }
}
