use crate::cut_chars;

const LISTED: usize = 30;
const LISTED_CHARS: usize = 60;
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
/// as the view cuts it. The first rank with a match decides: exact text,
/// exact value, the same ignoring case and spacing, the view's cut form,
/// then a unique start. At that rank an enabled option beats a disabled
/// twin; several enabled ones are ambiguous, only disabled ones refused.
pub(crate) fn pick_option(options: &[OptionEntry], choice: &str) -> OptionPick {
    let wanted = normal(choice);
    let wanted = wanted
        .strip_suffix(DISABLED_MARK)
        .map_or(wanted.clone(), str::to_owned);
    let low = wanted.to_lowercase();
    let pair = split_printed(&low);
    let tiers: [&dyn Fn(&OptionEntry) -> bool; 6] = [
        &|option| names(option).contains(&wanted),
        &|option| option.value == wanted || normal(&option.value) == wanted,
        &|option| names(option).iter().any(|name| name.to_lowercase() == low),
        &|option| normal(&option.value).to_lowercase() == low,
        &|option| cut_form(option, &low, pair.as_ref()),
        &|option| {
            !low.is_empty()
                && names(option)
                    .iter()
                    .any(|name| name.to_lowercase().starts_with(&low))
        },
    ];
    for test in tiers {
        let found: Vec<usize> = (0..options.len())
            .filter(|index| test(&options[*index]))
            .collect();
        let Some(first) = found.first().copied() else {
            continue;
        };
        let enabled: Vec<usize> = found
            .into_iter()
            .filter(|index| !options[*index].disabled)
            .collect();
        return match enabled.as_slice() {
            [] => OptionPick::Disabled(first),
            [only] => OptionPick::Chosen(*only),
            _ => OptionPick::Ambiguous(enabled),
        };
    }
    OptionPick::Missing
}

/// An option as the view prints it, uncut: its label, with its value in
/// parentheses when the label does not already hold it.
pub(crate) fn printed(option: &OptionEntry) -> String {
    let label = normal(&option.label);
    let value = option.value.trim();
    if value.is_empty() || label.to_lowercase().contains(&value.to_lowercase()) {
        label
    } else {
        format!("{label} ({value})")
    }
}

/// The options at `shown` in their printed form, ` | ` between them, a
/// disabled one marked, at most thirty and then `… and N more`.
pub(crate) fn listing(options: &[OptionEntry], shown: &[usize]) -> String {
    let parts: Vec<String> = shown
        .iter()
        .filter_map(|index| options.get(*index))
        .take(LISTED)
        .map(|option| {
            let mut part = cut_chars(&printed(option), LISTED_CHARS);
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

fn names(option: &OptionEntry) -> [String; 3] {
    [normal(&option.label), normal(&option.text), printed(option)]
}

fn cut_form(option: &OptionEntry, low: &str, pair: Option<&(String, String)>) -> bool {
    let label = normal(&option.label).to_lowercase();
    let value = normal(&option.value).to_lowercase();
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
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
#[path = "option_match_tests.rs"]
mod tests;
