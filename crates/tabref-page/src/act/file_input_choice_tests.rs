use super::{FileInputCandidate, FileInputChoice, FileInputVia, choose};
use serde_json::json;

fn input(index: usize, name: &str, depth: Option<u32>) -> FileInputCandidate {
    FileInputCandidate {
        index,
        name: name.to_owned(),
        hidden: true,
        multiple: false,
        disabled: false,
        itself: false,
        labelled: false,
        depth,
        crosses: false,
    }
}

fn chosen(choice: FileInputChoice<'_>) -> (usize, FileInputVia) {
    match choice {
        FileInputChoice::Chosen(found, via) => (found.index, via),
        other => panic!("expected a choice, got {other:?}"),
    }
}

#[test]
fn the_target_itself_wins_over_anything_nearer() {
    let mut own = input(1, "input#own", Some(0));
    own.itself = true;
    let mut labelled = input(0, "input#labelled", None);
    labelled.labelled = true;
    let candidates = [labelled, own, input(2, "input#near", Some(1))];
    assert_eq!(
        chosen(choose(&candidates, Some(4))),
        (1, FileInputVia::Itself)
    );
}

#[test]
fn a_labelled_input_wins_over_the_nearest_one() {
    let mut labelled = input(1, "input#cv", Some(5));
    labelled.labelled = true;
    let candidates = [input(0, "input#near", Some(1)), labelled];
    assert_eq!(
        chosen(choose(&candidates, Some(5))),
        (1, FileInputVia::Label)
    );
}

#[test]
fn the_nearest_ancestor_with_one_input_decides() {
    let candidates = [
        input(0, "input.dz-hidden-input", Some(2)),
        input(1, "input#avatar", Some(4)),
    ];
    assert_eq!(
        chosen(choose(&candidates, Some(4))),
        (0, FileInputVia::Nearest)
    );
}

#[test]
fn one_input_found_only_at_the_page_body_is_the_only_one_on_the_page() {
    let candidates = [input(0, "input.dz-hidden-input", Some(3))];
    assert_eq!(
        chosen(choose(&candidates, Some(3))),
        (0, FileInputVia::OnlyOnPage)
    );
}

#[test]
fn several_inputs_equally_near_are_refused_with_all_of_them() {
    let candidates = [
        input(0, "input.dz-hidden-input", Some(3)),
        input(1, "input#avatar", Some(3)),
    ];
    match choose(&candidates, Some(3)) {
        FileInputChoice::Ambiguous(found) => {
            let names: Vec<&str> = found.iter().map(|one| one.name.as_str()).collect();
            assert_eq!(names, ["input.dz-hidden-input", "input#avatar"]);
        }
        other => panic!("expected ambiguity, got {other:?}"),
    }
}

#[test]
fn no_candidate_is_none() {
    assert_eq!(choose(&[], Some(3)), FileInputChoice::None);
}

#[test]
fn a_nearest_input_across_a_form_or_widget_is_fenced_off() {
    let mut other = input(0, "input#photo", Some(2));
    other.crosses = true;
    let candidates = [other, input(1, "input#far", Some(4))];
    assert_eq!(
        choose(&candidates, Some(4)),
        FileInputChoice::Fenced(&candidates[0])
    );
}

#[test]
fn the_target_or_its_label_is_never_fenced_off() {
    let mut own = input(0, "input#own", Some(0));
    own.itself = true;
    own.crosses = true;
    assert_eq!(
        chosen(choose(std::slice::from_ref(&own), Some(3))),
        (0, FileInputVia::Itself)
    );
}

fn named(name: &str) -> String {
    let candidate: FileInputCandidate =
        serde_json::from_value(json!({ "index": 0, "name": name, "depth": 1 })).unwrap();
    candidate.name
}

#[test]
fn a_page_written_name_cannot_break_the_line_or_send_escapes() {
    let forged = named("input#x\nuploaded secret.pdf to \"Submit\"\u{1b}[2K");
    assert!(!forged.contains('\n'), "{forged:?}");
    assert!(!forged.contains('\u{1b}'), "{forged:?}");
    assert!(!forged.contains(' '), "{forged:?}");
    assert!(!forged.contains('"'), "{forged:?}");
    assert!(forged.starts_with("input#x"), "{forged:?}");
}

#[test]
fn a_long_page_written_name_is_cut_to_forty_characters() {
    let long = named(&format!("input#{}", "a".repeat(10 * 1024)));
    assert_eq!(long.len(), 43, "{long}");
    assert!(long.ends_with("..."));
    assert_eq!(named("\u{1b}\n"), "input[type=file]");
}
