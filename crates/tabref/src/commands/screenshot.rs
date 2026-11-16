use std::path::Path;

use serde_json::json;
use tabref_core::{AppError, resolve_state_root, write_atomic};
use tabref_page::{ActionNeed, LocateIntent, capture_screenshot, png_size};

use super::act_target::resolve_target;
use super::screenshot_path::{default_out, screenshot_path};
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Saves a PNG of the current tab's visible area, or of the element
/// `target` names once it shows, to `out` (default: a new file in the
/// working directory, never in the state directory), and says where it is,
/// how large and how many pixels.
pub(crate) fn run(
    session: &mut Session,
    target: Option<&str>,
    out: Option<&Path>,
) -> Result<Outcome, AppError> {
    let out = match out {
        Some(out) => out.to_path_buf(),
        None => default_out(&std::env::current_dir()?),
    };
    let path = screenshot_path(&out, &resolve_state_root()?)?;
    let deadline = session.deadline();
    let (png, of) = match target {
        Some(text) => {
            let spec = TargetSpec::from_arg(text)?;
            let element = resolve_target(session, &spec, LocateIntent::Read, deadline)?;
            element.ensure_ready(session, ActionNeed::Shown, deadline)?;
            let png = capture_screenshot(
                session.connection(),
                &element.session,
                Some(&element.token),
                deadline,
            )?;
            (png, Some(element.label))
        }
        None => {
            let page = session.page()?;
            (
                capture_screenshot(session.connection(), &page, None, deadline)?,
                None,
            )
        }
    };
    let (width, height) = png_size(&png)
        .ok_or_else(|| AppError::internal("The browser sent an image that is not a PNG"))?;
    write_atomic(&path, &png)?;
    let what = of
        .as_deref()
        .map_or_else(|| "the page".to_owned(), |label| label.to_owned());
    Ok(Outcome::new(
        format!(
            "saved a screenshot of {what}: {} ({width}x{height} px, {})",
            path.display(),
            shown_size(png.len())
        ),
        json!({
            "path": path,
            "bytes": png.len(),
            "width": width,
            "height": height,
            "target": of,
        }),
    ))
}

fn shown_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    }
}
