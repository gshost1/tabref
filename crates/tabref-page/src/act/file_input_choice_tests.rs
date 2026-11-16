use super::{FileInputCandidate, FileInputChoice, FileInputVia, choose};

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
    assert_eq!(chosen(choose(&candidates, Some(4))), (1, FileInputVia::Itself));
}

#[test]
fn a_labelled_input_wins_over_the_nearest_one() {
    let mut labelled = input(1, "input#cv", Some(5));
    labelled.labelled = true;
    let candidates = [input(0, "input#near", Some(1)), labelled];
    assert_eq!(chosen(choose(&candidates, Some(5))), (1, FileInputVia::Label));
}

#[test]
fn the_nearest_ancestor_with_one_input_decides() {
    let candidates = [
        input(0, "input.dz-hidden-input", Some(2)),
        input(1, "input#avatar", Some(4)),
    ];
    assert_eq!(chosen(choose(&candidates, Some(4))), (0, FileInputVia::Nearest));
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
