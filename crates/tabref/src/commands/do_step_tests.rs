use super::split_value;
use crate::commands::step_words::split_steps;

fn split(line: &str) -> (Vec<String>, String) {
    let steps = split_steps(line).unwrap();
    let (aim, value) = split_value(&steps[0]).unwrap();
    (aim.iter().map(|word| word.text.clone()).collect(), value)
}

#[test]
fn an_unquoted_value_with_spaces_is_the_rest_of_the_step() {
    assert_eq!(
        split(r#"fill "Title" 401 bad gateway"#),
        (vec!["Title".to_owned()], "401 bad gateway".to_owned())
    );
    assert_eq!(
        split(r#"select "Assignee" Zofia Xu (@zxu)"#),
        (vec!["Assignee".to_owned()], "Zofia Xu (@zxu)".to_owned())
    );
}

#[test]
fn a_scoped_target_keeps_its_scope_and_a_quoted_value_stays_whole() {
    assert_eq!(
        split(r#"fill "Price" in "Hoodie-S" 62.00"#),
        (
            vec!["Price".to_owned(), "in".to_owned(), "Hoodie-S".to_owned()],
            "62.00".to_owned()
        )
    );
    assert_eq!(
        split(r#"fill e4 "Ada Lovelace""#),
        (vec!["e4".to_owned()], "Ada Lovelace".to_owned())
    );
}

#[test]
fn a_step_with_no_value_is_refused() {
    let steps = split_steps(r#"fill "Title""#).unwrap();
    assert!(split_value(&steps[0]).is_err());
}
