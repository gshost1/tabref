use tabref_core::ErrorCode;
use tabref_page::Word;

use super::{split_steps, split_words};

#[test]
fn lines_and_semicolons_end_steps_and_quotes_keep_spaces() {
    let steps = split_steps(
        "open /admin/\n  fill \"Price ($)\" 62.00 ; click 'Save'\n\n# a comment\nexpect url ~saved=1",
    )
    .unwrap();
    let sources: Vec<&str> = steps.iter().map(|step| step.source.as_str()).collect();
    assert_eq!(
        sources,
        [
            "open /admin/",
            "fill \"Price ($)\" 62.00",
            "click 'Save'",
            "expect url ~saved=1"
        ]
    );
    assert_eq!(
        steps[1].words,
        vec![
            Word::bare("fill"),
            Word::quoted("Price ($)"),
            Word::bare("62.00")
        ]
    );
    assert_eq!(steps[2].words[1], Word::quoted("Save"));
    assert_eq!(
        steps.iter().map(|step| step.line).collect::<Vec<_>>(),
        [1, 2, 2, 5]
    );
}

#[test]
fn double_quotes_take_escapes_and_single_quotes_take_text_as_it_is() {
    let words = split_words(r#"fill "say \"hi\"\n" 'a\b' "" "x;y" "C:\d""#).unwrap();
    assert_eq!(
        words,
        vec![
            Word::bare("fill"),
            Word::quoted("say \"hi\"\n"),
            Word::quoted("a\\b"),
            Word::quoted(""),
            Word::quoted("x;y"),
            Word::quoted("C:\\d"),
        ]
    );
}

#[test]
fn an_open_quote_is_refused_with_its_line() {
    let error = split_steps("click \"Save\nfill x").unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArgs);
    assert!(error.message.starts_with("line 1"), "{}", error.message);
}
