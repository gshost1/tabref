use serde_json::json;
use tabref_core::AppError;
use tabref_page::read_element;

use crate::output::outcome::Outcome;
use crate::session::Session;
use crate::session::ref_target::RefTarget;
use crate::view::snapshot_view::reading_view;
use crate::view::view_options::ViewOptions;
use crate::view::view_scope::ViewScope;

const FIELD_ROLES: &[&str] = &[
    "textfield", "combobox", "checkbox", "radiobutton", "switch", "slider", "incrementor", "listbox",
];

/// Prints text in reading order: the page's main content (the `main`
/// landmark with the most text, across same-origin frames), the section
/// under a heading, or one element. A form field reads as its text, value
/// and checked state.
pub(crate) fn run(
    session: &mut Session,
    target: Option<&str>,
    section: Option<&str>,
) -> Result<Outcome, AppError> {
    let options = ViewOptions::read("");
    if let Some(text) = target {
        let target = session.resolve_ref(text)?;
        if FIELD_ROLES.contains(&target.entry.role.as_str()) {
            return read_field(session, &target);
        }
        let scope = ViewScope::Element {
            target_id: target.token.target_id.clone(),
            backend_id: target.token.backend_node_id,
        };
        return reading_view(session, &options, &scope);
    }
    let scope = match section {
        Some(heading) => ViewScope::Section(heading.to_owned()),
        None => ViewScope::Main,
    };
    reading_view(session, &options, &scope)
}

fn read_field(session: &mut Session, target: &RefTarget) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    let reading = read_element(session.connection(), &target.session, &target.token, deadline)?;
    let mut out = format!("{}", target.label());
    if let Some(value) = reading.value.as_deref() {
        out.push_str(&format!(" = \"{value}\""));
    }
    if let Some(checked) = reading.checked {
        out.push_str(if checked { " (checked)" } else { " (not checked)" });
    }
    if let Some(text) = reading.text.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
        out.push_str(&format!("\n{text}"));
    }
    Ok(Outcome::new(
        out,
        json!({
            "ref": target.ref_id.to_string(),
            "text": reading.text,
            "value": reading.value,
            "checked": reading.checked,
        }),
    ))
}
