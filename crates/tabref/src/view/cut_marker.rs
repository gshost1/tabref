use super::view_line::ViewLine;
use super::view_route::ViewRoute;

const FORM_ROLES: [&str; 11] = [
    "button",
    "textbox",
    "searchbox",
    "checkbox",
    "radio",
    "select",
    "combobox",
    "switch",
    "slider",
    "spinbutton",
    "clickable",
];

/// The line that marks a view's cut: `… lines 96-113 cut here to fit:
/// tabref snap --from 96`, and, when buttons or fields print after the
/// cut, `; 3 form controls follow, from line 111: tabref snap --from 111`,
/// so the end of a form is never left behind the cut without a pointer.
pub(crate) fn cut_marker(
    after: &[ViewLine],
    last: usize,
    route: &ViewRoute,
    full: bool,
) -> ViewLine {
    let first = after.first().map_or(last, |line| line.number);
    let mut text = format!(
        "… lines {first}-{last} cut here to fit: {}",
        route.starting_at(first, full)
    );
    let mut controls = 0usize;
    let mut first_control = None;
    for line in after {
        let count = form_controls(&line.text);
        if count > 0 && first_control.is_none() {
            first_control = Some(line.number);
        }
        controls += count;
    }
    if let Some(at) = first_control {
        let noun = if controls == 1 {
            "control follows"
        } else {
            "controls follow"
        };
        text.push_str(&format!(
            "; {controls} form {noun}, from line {at}: {}",
            route.starting_at(at, full)
        ));
    }
    ViewLine::new(0, text)
}

/// How many button or field tokens (`[e5 button Save]`, `[e6 textbox
/// Email]`) a printed line holds. Page text escapes its brackets, so an
/// unescaped `[` opens a token the view wrote.
fn form_controls(text: &str) -> usize {
    let mut count = 0;
    let mut escaped = false;
    for (at, character) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '[' => {
                let mut words = text[at + 1..].split([' ', ']']);
                let first = words.next().unwrap_or_default();
                let role = if is_ref(first) {
                    words.next().unwrap_or_default()
                } else {
                    first
                };
                if FORM_ROLES.contains(&role) {
                    count += 1;
                }
            }
            _ => {}
        }
    }
    count
}

/// Whether `word` is a printed ref: `e12`, or `@s8f3k2p9:e12` in long form.
fn is_ref(word: &str) -> bool {
    let element = word.rsplit(':').next().unwrap_or_default();
    element
        .strip_prefix('e')
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
}
