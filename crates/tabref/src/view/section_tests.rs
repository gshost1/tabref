use super::section_lines;
use crate::view::view_line::ViewLine;

fn line(depth: usize, text: &str, heading: Option<u8>) -> ViewLine {
    let mut line = ViewLine::new(depth, text);
    line.heading = heading;
    line
}

#[test]
fn a_section_runs_to_the_next_heading_of_its_level() {
    let lines = vec![
        line(1, "## Early life", Some(2)),
        line(1, "born", None),
        line(1, "## Career", Some(2)),
        line(2, "### Bombe", Some(3)),
        line(2, "machine", None),
        line(1, "## Death", Some(2)),
    ];
    let section = section_lines(lines.clone(), "career").unwrap();
    let texts: Vec<String> = section.iter().map(ViewLine::printed).collect();
    assert_eq!(texts, vec!["## Career", "  ### Bombe", "  machine"]);
    assert!(section_lines(lines, "nothing").is_none());
}

#[test]
fn an_exact_heading_wins_over_one_that_only_contains_the_text_and_links_match_by_their_words() {
    let mut linked = line(0, "## [@s1:e146 Post title 46 -> /post/46]", Some(2));
    linked.plain = Some("Post title 46".into());
    let lines = vec![
        line(0, "## History of Rome", Some(2)),
        line(0, "early", None),
        line(0, "## History", Some(2)),
        line(0, "late", None),
        linked,
        line(0, "teaser", None),
    ];
    let history = section_lines(lines.clone(), "history").unwrap();
    assert_eq!(history[1].text, "late");
    let post = section_lines(lines, "Post title 46").unwrap();
    assert_eq!(post[1].text, "teaser");
}
