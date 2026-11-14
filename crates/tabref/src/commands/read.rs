use serde_json::json;
use tabref_core::AppError;
use tabref_page::{PageInfo, page_text, read_element};

use crate::output::outcome::Outcome;
use crate::session::Session;

const PAGE_TEXT_LIMIT: usize = 20_000;

/// Prints the page's rendered text, or one element's text and value.
pub(crate) fn run(session: &mut Session, target: Option<&str>) -> Result<Outcome, AppError> {
    let deadline = session.deadline();
    if let Some(text) = target {
        let target = session.resolve_ref(text)?;
        let reading = read_element(
            session.connection(),
            &target.session,
            &target.token,
            deadline,
        )?;
        let mut out = reading.text.clone().unwrap_or_default().trim().to_owned();
        if let Some(value) = reading.value.as_deref().filter(|value| !value.is_empty()) {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("value: {value}"));
        }
        if let Some(checked) = reading.checked {
            out.push_str(&format!("\nchecked: {checked}"));
        }
        return Ok(Outcome::new(
            out,
            json!({
                "ref": target.ref_id.to_string(),
                "text": reading.text,
                "value": reading.value,
                "checked": reading.checked,
            }),
        ));
    }
    let page = session.page()?;
    let info = PageInfo::read(session.connection(), &page.target_id, deadline)?;
    let (text, cut) = page_text(session.connection(), &page, PAGE_TEXT_LIMIT, deadline)?;
    let mut out = format!("page \"{}\" {}\n{text}", info.title, info.url);
    if cut {
        out.push_str(&format!(
            "\n... cut at {PAGE_TEXT_LIMIT} characters; use 'tabref find' for the rest"
        ));
    }
    Ok(Outcome::new(
        out,
        json!({ "url": info.url, "title": info.title, "text": text, "truncated": cut }),
    ))
}
