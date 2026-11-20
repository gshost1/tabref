use crate::{cut_chars, squash};

const LISTED: usize = 30;
pub(super) const LISTED_CHARS: usize = 60;
const DISABLED_MARK: &str = " (disabled)";

/// One `<option>` of a select as the page has it: its label, its own text,
/// its value, and whether it can be picked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct OptionEntry {
    pub label: String,
    pub text: String,
    pub value: String,
    pub disabled: bool,
}

/// What a choice names among a select's options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OptionPick {
    Chosen(usize),
    Ambiguous(Vec<usize>),
    Disabled(usize),
    Missing,
}

/// The option `choice` names, in any form tabref prints one: its label, its
/// text, its value, or the view's `label (value)`, escaped or cut with `…`
/// as the view cuts it. The first rank with a match decides: exact, then
/// the same ignoring case and spacing, then the view's cut form, then a
/// unique start. At the two exact ranks an option whose printed form is
/// the choice beats one whose label only is, an enabled option beats a
/// disabled twin, and a label and another option's value alike are
/// ambiguous. At the cut and start ranks every match counts, disabled
/// ones too, so a start is never silently taken past a disabled option.
/// A trailing ` (disabled)`, as error listings mark an option, is read
/// only when the choice names nothing with it.
pub(crate) fn pick_option(options: &[OptionEntry], choice: &str) -> OptionPick {
    let wanted = normal(choice);
    match pick_exactly(options, &wanted) {
        OptionPick::Missing => {}
        pick => return pick,
    }
    match wanted.strip_suffix(DISABLED_MARK) {
        Some(bare) => pick_exactly(options, bare),
        None => OptionPick::Missing,
    }
}

fn pick_exactly(options: &[OptionEntry], wanted: &str) -> OptionPick {
    let low = wanted.to_lowercase();
    let pair = split_printed(&low);
    let same = |text: &str, folded: bool| {
        if folded {
            text.to_lowercase() == low
        } else {
            text == wanted
        }
    };
    for folded in [false, true] {
        let printed_as: Vec<usize> = matching(options, |option| same(&printed(option), folded));
        let named: Vec<usize> = matching(options, |option| {
            same(&squash(&option.label), folded) || same(&squash(&option.text), folded)
        });
        let valued: Vec<usize> = matching(options, |option| {
            same(&option.value, folded) || same(&squash(&option.value), folded)
        });
        let mut found = if printed_as.is_empty() {
            named
        } else {
            printed_as
        };
        for index in valued {
            if !found.contains(&index) {
                found.push(index);
            }
        }
        found.sort_unstable();
        if !found.is_empty() {
            return exact_pick(options, &found);
        }
    }
    let loose: [&dyn Fn(&OptionEntry) -> bool; 2] =
        [&|option| cut_form(option, &low, pair.as_ref()), &|option| {
            !low.is_empty()
                && names(option)
                    .iter()
                    .any(|name| name.to_lowercase().starts_with(&low))
        }];
    for test in loose {
        match matching(options, test).as_slice() {
            [] => {}
            [only] if options[*only].disabled => return OptionPick::Disabled(*only),
            [only] => return OptionPick::Chosen(*only),
            several => return OptionPick::Ambiguous(several.to_vec()),
        }
    }
    OptionPick::Missing
}

fn matching(options: &[OptionEntry], test: impl Fn(&OptionEntry) -> bool) -> Vec<usize> {
    (0..options.len())
        .filter(|index| test(&options[*index]))
        .collect()
}

fn exact_pick(options: &[OptionEntry], found: &[usize]) -> OptionPick {
    let enabled: Vec<usize> = found
        .iter()
        .copied()
        .filter(|index| !options[*index].disabled)
        .collect();
    match enabled.as_slice() {
        [] => OptionPick::Disabled(found[0]),
        [only] => OptionPick::Chosen(*only),
        _ => OptionPick::Ambiguous(enabled),
    }
}

/// An option as the view prints it, uncut: its label, with its value in
/// parentheses when the label does not already hold it.
pub(crate) fn printed(option: &OptionEntry) -> String {
    let label = squash(&option.label);
    let value = option.value.trim();
    if value.is_empty() || option.label.to_lowercase().contains(&value.to_lowercase()) {
        label
    } else {
        format!("{label} ({value})")
    }
}

/// The options at `shown` in their escaped printed form, ` | ` between them, a
/// disabled one marked, at most thirty and then `… and N more`.
pub(crate) fn listing(options: &[OptionEntry], shown: &[usize]) -> String {
    let parts: Vec<String> = shown
        .iter()
        .filter_map(|index| options.get(*index))
        .take(LISTED)
        .map(|option| {
            let mut part = escaped(&listed_name(option));
            if option.disabled {
                part.push_str(DISABLED_MARK);
            }
            part
        })
        .collect();
    let joined = parts.join(" | ");
    if shown.len() <= LISTED {
        return joined;
    }
    format!("{joined} … and {} more", shown.len() - LISTED)
}

/// An option's printed form as an error names it: `[`, `]` and `|`
/// escaped as the view escapes them, control characters as `\uXXXX`.
pub(crate) fn shown(option: &OptionEntry) -> String {
    escaped(&listed_name(option))
}

pub(super) fn listed_name(option: &OptionEntry) -> String {
    cut_chars(&printed(option), LISTED_CHARS)
}

fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '[' | ']' | '|' => {
                out.push('\\');
                out.push(character);
            }
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out
}

fn names(option: &OptionEntry) -> [String; 3] {
    [squash(&option.label), squash(&option.text), printed(option)]
}

fn cut_form(option: &OptionEntry, low: &str, pair: Option<&(String, String)>) -> bool {
    let label = squash(&option.label).to_lowercase();
    let value = squash(&option.value).to_lowercase();
    if let Some((label_part, value_part)) = pair
        && fits(&label, label_part)
        && fits(&value, value_part)
        && (label_part.ends_with('…') || value_part.ends_with('…'))
    {
        return true;
    }
    low.ends_with('…') && fits(&label, low)
}

fn fits(full: &str, shown: &str) -> bool {
    match shown.strip_suffix('…') {
        Some(start) => !start.is_empty() && full.starts_with(start.trim_end()),
        None => full == shown,
    }
}

fn split_printed(low: &str) -> Option<(String, String)> {
    let inner = low.strip_suffix(')')?;
    let (label, value) = inner.rsplit_once(" (")?;
    Some((label.to_owned(), value.to_owned()))
}

fn normal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\\'
            && let Some(next) = characters.peek().copied()
            && matches!(next, '[' | ']' | '|' | '\\')
        {
            out.push(next);
            characters.next();
            continue;
        }
        out.push(character);
    }
    squash(&out)
}

#[cfg(test)]
#[path = "option_match_tests.rs"]
mod tests;
