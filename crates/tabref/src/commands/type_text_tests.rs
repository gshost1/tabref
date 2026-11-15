use super::tail;

#[test]
fn a_short_text_is_shown_whole() {
    assert_eq!(tail("Hello world"), "Hello world");
}

#[test]
fn a_long_text_shows_its_last_forty_characters() {
    let long = format!("{}the end", "x".repeat(3_000));
    let shown = tail(&long);
    assert!(shown.starts_with('…'));
    assert!(shown.ends_with("the end"));
    assert_eq!(shown.chars().count(), 41);
}
