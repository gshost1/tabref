use super::section_lines;
use crate::view::view_line::ViewLine;

fn line(depth: usize, text: &str, heading: Option<u8>) -> ViewLine {
    ViewLine {
        depth,
        text: text.into(),
        heading,
        chrome: None,
    }
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
