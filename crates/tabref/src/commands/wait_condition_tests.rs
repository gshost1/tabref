use tabref_core::ErrorCode;
use tabref_page::Word;

use super::Condition;
use crate::commands::wait_flags::WaitFlags;

#[test]
fn exactly_one_flag_names_the_condition() {
    assert_eq!(
        Condition::from_flags(&WaitFlags {
            text: Some("Done"),
            ..WaitFlags::default()
        })
        .unwrap(),
        Condition::Text("Done".into())
    );
    assert_eq!(
        Condition::from_flags(&WaitFlags {
            idle: true,
            ..WaitFlags::default()
        })
        .unwrap(),
        Condition::Idle
    );
    assert_eq!(
        Condition::from_flags(&WaitFlags {
            download: true,
            ..WaitFlags::default()
        })
        .unwrap(),
        Condition::Download
    );
    for (text, url, idle) in [
        (None, None, false),
        (Some("a"), Some("/b"), false),
        (Some("a"), None, true),
    ] {
        let error = Condition::from_flags(&WaitFlags {
            text,
            url,
            idle,
            ..WaitFlags::default()
        })
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgs);
    }
}

#[test]
fn a_step_names_its_condition_in_words() {
    let words = |list: &[&str]| list.iter().map(|text| Word::bare(text)).collect::<Vec<_>>();
    assert_eq!(
        Condition::from_words(&words(&["url", "~saved=1"])).unwrap(),
        Condition::Url("saved=1".into())
    );
    assert_eq!(
        Condition::from_words(&words(&["url", "/saved/"])).unwrap(),
        Condition::Url("saved".into())
    );
    assert_eq!(
        Condition::from_words(&[Word::bare("text"), Word::quoted("You saved")]).unwrap(),
        Condition::Text("You saved".into())
    );
    assert_eq!(
        Condition::from_words(&[Word::quoted("Saved")]).unwrap(),
        Condition::Text("Saved".into())
    );
    assert_eq!(
        Condition::from_words(&words(&["idle"])).unwrap(),
        Condition::Idle
    );
    assert_eq!(
        Condition::from_words(&words(&["text"])).unwrap_err().code,
        ErrorCode::InvalidArgs
    );
}

#[test]
fn an_element_state_is_named_by_flags_or_by_is_in_a_step() {
    let words = |list: &[(&str, bool)]| {
        list.iter()
            .map(|(text, quoted)| {
                if *quoted {
                    Word::quoted(text)
                } else {
                    Word::bare(text)
                }
            })
            .collect::<Vec<_>>()
    };
    let step = Condition::from_words(&words(&[("Save", true), ("is", false), ("enabled", false)]))
        .unwrap();
    assert_eq!(step.describe(), "\"Save\" to be enabled");
    let flagged = Condition::from_flags(&WaitFlags {
        target: Some("e4"),
        state: Some("value=62.00"),
        ..WaitFlags::default()
    })
    .unwrap();
    assert_eq!(flagged.describe(), "e4 to be value \"62.00\"");
    let half = Condition::from_flags(&WaitFlags {
        target: Some("e4"),
        ..WaitFlags::default()
    })
    .unwrap_err();
    assert_eq!(half.code, ErrorCode::InvalidArgs);
    let bad = Condition::from_words(&words(&[("e4", false), ("is", false), ("shiny", false)]))
        .unwrap_err();
    assert_eq!(bad.code, ErrorCode::InvalidArgs);
}

fn mixed(list: &[(&str, bool)]) -> Vec<Word> {
    list.iter()
        .map(|(text, quoted)| {
            if *quoted {
                Word::quoted(text)
            } else {
                Word::bare(text)
            }
        })
        .collect()
}

#[test]
fn a_keyword_wins_over_is() {
    assert_eq!(
        Condition::from_words(&mixed(&[
            ("text", false),
            ("Notifications", false),
            ("is", false),
            ("enabled", false)
        ]))
        .unwrap(),
        Condition::Text("Notifications is enabled".into())
    );
    assert_eq!(
        Condition::from_words(&mixed(&[
            ("gone", false),
            ("Upload", true),
            ("is", false),
            ("hidden", false)
        ]))
        .unwrap(),
        Condition::Gone("Upload is hidden".into())
    );
}

#[test]
fn only_a_ref_or_one_quoted_name_is_an_element() {
    assert_eq!(
        Condition::from_words(&mixed(&[
            ("Autosave", false),
            ("is", false),
            ("enabled", false)
        ]))
        .unwrap(),
        Condition::Text("Autosave is enabled".into())
    );
    assert_eq!(
        Condition::from_words(&mixed(&[
            ("the", false),
            ("Save", true),
            ("is", false),
            ("enabled", false)
        ]))
        .unwrap(),
        Condition::Text("the Save is enabled".into())
    );
    let by_ref =
        Condition::from_words(&mixed(&[("e4", false), ("is", false), ("hidden", false)])).unwrap();
    assert_eq!(by_ref.describe(), "e4 to be hidden");
    let quoted = Condition::from_words(&mixed(&[
        ("Total", true),
        ("is", false),
        ("value=62 00", true),
    ]))
    .unwrap();
    assert_eq!(quoted.describe(), "\"Total\" to be value \"62 00\"");
}

#[test]
fn a_value_with_spaces_must_be_quoted() {
    let error = Condition::from_words(&mixed(&[
        ("Total", true),
        ("is", false),
        ("value=a", false),
        ("b", false),
    ]))
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArgs);
    assert!(
        error.message.contains("quote a value with spaces"),
        "{}",
        error.message
    );
}
