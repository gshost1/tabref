use std::fmt::Write as _;

use tabref_page::{Control, cut_chars, short_href, squash};

use super::escape::{escape_href, escape_in_token};
use super::node_line::quote_text;
use super::view_options::ViewOptions;

const NAME_LIMIT: usize = 100;
const VALUE_LIMIT: usize = 80;
const HREF_LIMIT: usize = 120;
const CROSS_SITE_LIMIT: usize = 80;
const OPTION_LIMIT: usize = 20;
const OPTION_TEXT_LIMIT: usize = 40;

/// A control as one bracketed token: `[e4 Edit -> /admin/product/4/edit]`
/// for a link, `[e5 button Save]`, `[e6 textbox Email = "a@b.c"]`,
/// `[e7 select Size = "M" options: S | M | L]`, `[e8 checkbox Terms
/// (checked)]`. `show_href` is false when the line already gave the same
/// address.
pub(crate) fn control_text(control: &Control, options: &ViewOptions, show_href: bool) -> String {
    control_token(control, options, show_href).0
}

/// [`control_text`] and whether anything in the token was cut: a long
/// name, value or address, or options past the first twenty. Names,
/// options and addresses are escaped so page text cannot close the token;
/// a value is quoted on one line. With whole lines nothing is cut.
pub(crate) fn control_token(
    control: &Control,
    options: &ViewOptions,
    show_href: bool,
) -> (String, bool) {
    let whole = options.whole_lines();
    let mut cut = false;
    let mut limited = |text: String, limit: usize| {
        if whole || text.chars().count() <= limit {
            return text;
        }
        cut = true;
        cut_chars(&text, limit)
    };
    let mut out = String::from("[");
    if let Some(ref_text) = &control.ref_text {
        out.push_str(&options.refs.print(ref_text));
        out.push(' ');
    }
    let name = limited(escape_in_token(&control.name), NAME_LIMIT);
    let href = control
        .href
        .as_deref()
        .filter(|_| show_href)
        .map(|href| escape_href(&trim_query(short_href(&options.page_url, href), whole)));
    let href = href.map(|href| limited(href, HREF_LIMIT));
    let value = control
        .value
        .as_deref()
        .filter(|_| shows_value(control))
        .map(|value| limited(value.to_owned(), VALUE_LIMIT));
    let role = role_word(control);
    if !(control.is_link() && control.href.is_some()) {
        out.push_str(role);
        if !name.is_empty() {
            out.push(' ');
        }
    }
    out.push_str(&name);
    if let Some(value) = value {
        let _ = write!(out, " = {}", quote_text(&value));
    }
    if !control.options.is_empty() {
        out.push_str(" options: ");
        let (list, options_cut) = option_list(control, whole);
        cut |= options_cut;
        out.push_str(&list);
    }
    for state in &control.states {
        let _ = write!(out, " ({state})");
    }
    if let Some(href) = href {
        let _ = write!(out, " -> {href}");
    }
    out = out.trim_end().to_owned();
    out.push(']');
    (out, cut)
}

/// A control as reading text: its name, or its value for a field, and
/// every option of a select.
pub(crate) fn control_reading(control: &Control) -> String {
    if control.role == "button" || control.is_link() {
        return control.name.clone();
    }
    let mut out = match (&control.name, &control.value) {
        (name, Some(value)) if !name.is_empty() => format!("{name}: {}", squash(value)),
        (name, _) => name.clone(),
    };
    if !control.options.is_empty() {
        let labels: Vec<String> = control
            .options
            .iter()
            .map(|option| squash(&option.label))
            .collect();
        out.push_str(&format!(" (options: {})", labels.join(" | ")));
    }
    out
}

/// A long address on another site loses its query: share and tracking
/// links carry the page in their query, which costs tokens and says nothing.
fn trim_query(href: String, whole: bool) -> String {
    if whole
        || href.starts_with('/')
        || href.starts_with('#')
        || href.chars().count() <= CROSS_SITE_LIMIT
    {
        return href;
    }
    match href.split_once('?') {
        Some((path, _)) => format!("{path}?…"),
        None => href,
    }
}

fn role_word(control: &Control) -> &str {
    match control.role.as_str() {
        "textfield" => "textbox",
        "radiobutton" => "radio",
        "combobox" if !control.options.is_empty() => "select",
        "group" | "generic" | "" => "clickable",
        "statictext" => "text",
        other => other,
    }
}

fn shows_value(control: &Control) -> bool {
    !matches!(
        control.role.as_str(),
        "link" | "button" | "checkbox" | "radiobutton" | "switch"
    ) && control.value.as_deref() != Some(control.name.as_str())
}

fn option_list(control: &Control, whole: bool) -> (String, bool) {
    let limit = if whole { usize::MAX } else { OPTION_LIMIT };
    let mut parts: Vec<String> = control
        .options
        .iter()
        .take(limit)
        .map(|option| {
            let label = if whole {
                escape_in_token(&option.label)
            } else {
                escape_in_token(&cut_chars(&option.label, OPTION_TEXT_LIMIT))
            };
            let value = option.value.trim();
            if value.is_empty() || option.label.to_lowercase().contains(&value.to_lowercase()) {
                label
            } else {
                format!(
                    "{label} ({})",
                    escape_in_token(&cut_chars(value, OPTION_TEXT_LIMIT))
                )
            }
        })
        .collect();
    let cut = control.options.len() > limit;
    if cut {
        parts.push(format!("+{} more", control.options.len() - limit));
    }
    (parts.join(" | "), cut)
}

#[cfg(test)]
#[path = "control_text_tests.rs"]
mod tests;
