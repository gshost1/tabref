use std::fmt::Write as _;

use tabref_page::{Control, cut_chars, short_href};

use super::view_options::ViewOptions;

const NAME_LIMIT: usize = 100;
const VALUE_LIMIT: usize = 80;
const HREF_LIMIT: usize = 120;
const OPTION_LIMIT: usize = 20;
const OPTION_TEXT_LIMIT: usize = 40;

/// A control as one bracketed token: `[e4 Edit -> /admin/product/4/edit]`
/// for a link, `[e5 button Save]`, `[e6 textbox Email = "a@b.c"]`,
/// `[e7 select Size = "M" options: S | M | L]`, `[e8 checkbox Terms
/// (checked)]`. `show_href` is false when the line already gave the same
/// address.
pub(crate) fn control_text(control: &Control, options: &ViewOptions, show_href: bool) -> String {
    let mut out = String::from("[");
    if let Some(ref_text) = &control.ref_text {
        out.push_str(&options.refs.print(ref_text));
        out.push(' ');
    }
    let name = cut_chars(&control.name, NAME_LIMIT);
    let href = control
        .href
        .as_deref()
        .filter(|_| show_href)
        .map(|href| cut_chars(&short_href(&options.page_url, href), HREF_LIMIT));
    let role = role_word(control);
    if !(control.is_link() && control.href.is_some()) {
        out.push_str(role);
        if !name.is_empty() {
            out.push(' ');
        }
    }
    out.push_str(&name);
    if let Some(value) = control.value.as_deref().filter(|_| shows_value(control)) {
        let _ = write!(out, " = \"{}\"", cut_chars(value, VALUE_LIMIT));
    }
    if !control.options.is_empty() {
        out.push_str(" options: ");
        out.push_str(&option_list(control));
    }
    for state in &control.states {
        let _ = write!(out, " ({state})");
    }
    if let Some(href) = href {
        let _ = write!(out, " -> {href}");
    }
    out = out.trim_end().to_owned();
    out.push(']');
    out
}

/// A control as reading text: its name, or its value for a field.
pub(crate) fn control_reading(control: &Control) -> String {
    if control.role == "button" || control.is_link() {
        return control.name.clone();
    }
    match (&control.name, &control.value) {
        (name, Some(value)) if !name.is_empty() => format!("{name}: {value}"),
        (name, _) => name.clone(),
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
    !matches!(control.role.as_str(), "link" | "button" | "checkbox" | "radiobutton" | "switch")
        && control.value.as_deref() != Some(control.name.as_str())
}

fn option_list(control: &Control) -> String {
    let mut parts: Vec<String> = control
        .options
        .iter()
        .take(OPTION_LIMIT)
        .map(|option| {
            let label = cut_chars(&option.label, OPTION_TEXT_LIMIT);
            let value = option.value.trim();
            if value.is_empty() || value.eq_ignore_ascii_case(option.label.trim()) {
                label
            } else {
                format!("{label} ({})", cut_chars(value, OPTION_TEXT_LIMIT))
            }
        })
        .collect();
    if control.options.len() > OPTION_LIMIT {
        parts.push(format!("+{} more", control.options.len() - OPTION_LIMIT));
    }
    parts.join(" | ")
}

#[cfg(test)]
#[path = "control_text_tests.rs"]
mod tests;
