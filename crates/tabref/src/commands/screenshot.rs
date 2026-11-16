use std::path::Path;

use serde_json::json;
use tabref_core::{AppError, resolve_state_root, write_atomic};
use tabref_page::{ActionNeed, CapturedImage, LocateIntent, capture_screenshot, png_size};

use super::act_target::resolve_target;
use super::screenshot_path::{default_out, screenshot_path};
use super::target_spec::TargetSpec;
use crate::output::outcome::Outcome;
use crate::session::Session;

/// Saves a PNG of the current tab's visible area, or of the element
/// `target` names once it shows, to `out` (default: a new file in the
/// working directory, never in the state directory; an existing PNG there is
/// replaced), and says where it is, how large and how many pixels.
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
    let (image, of) = match target {
        Some(text) => {
            let spec = TargetSpec::from_arg(text)?;
            let element = resolve_target(session, &spec, LocateIntent::Read, deadline)?;
            element.ensure_ready(session, ActionNeed::Shown, deadline)?;
            let image = capture_screenshot(
                session.connection(),
                &element.session,
                Some(&element.token),
                deadline,
            )
            .map_err(|error| hidden_hint(session, error))?;
            (image, Some(element.label))
        }
        None => {
            let page = session.page()?;
            let image = capture_screenshot(session.connection(), &page, None, deadline)
                .map_err(|error| hidden_hint(session, error))?;
            (image, None)
        }
    };
    let replaced = path.exists();
    write_atomic(&path, &image.png)?;
    saved(&path, replaced, &image, of.as_deref())
}

/// What the command reports about the PNG it wrote to `path`: whether it
/// replaced a PNG there, its pixels and, when it is not 1, the device scale
/// it was taken at. A path that is not UTF-8 is shown with replacement
/// characters rather than failing the report of a file already written.
pub(crate) fn saved(
    path: &Path,
    replaced: bool,
    image: &CapturedImage,
    of: Option<&str>,
) -> Result<Outcome, AppError> {
    let (width, height) = png_size(&image.png)
        .ok_or_else(|| AppError::internal("The browser sent an image that is not a PNG"))?;
    let what = of.unwrap_or("the page");
    let shown = path.display().to_string();
    let scale = if (image.device_scale - 1.0).abs() > 0.01 {
        format!(" at {}x", image.device_scale)
    } else {
        String::new()
    };
    let facts = format!(
        "({width}x{height} px{scale}, {})",
        shown_size(image.png.len())
    );
    let text = if replaced {
        format!("replaced {shown} with a screenshot of {what} {facts}")
    } else {
        format!("saved a screenshot of {what}: {shown} {facts}")
    };
    Ok(Outcome::new(
        text,
        json!({
            "path": shown,
            "replaced": replaced,
            "bytes": image.png.len(),
            "width": width,
            "height": height,
            "device_scale": image.device_scale,
            "target": of,
        }),
    ))
}

pub(crate) fn shown_size(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    let size = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else if size < KB * KB {
        format!("{:.1} KB", size / KB)
    } else {
        format!("{:.1} MB", size / (KB * KB))
    }
}

/// Names the tab to bring to the front when the tab is not showing; tabref
/// never brings it there itself.
fn hidden_hint(session: &mut Session, error: AppError) -> AppError {
    if error.kind() != Some("screenshot_tab_hidden") {
        return error;
    }
    let index = session.tab_id().ok().and_then(|current| {
        session
            .tabs()
            .ok()?
            .iter()
            .position(|tab| tab.target_id == current)
    });
    let step = index.map_or_else(
        || "Run 'tabref tabs', then 'tabref tab <n>' for this tab, first".to_owned(),
        |index| format!("Run 'tabref tab {index}' first to bring it to the front"),
    );
    error.with_suggestion(format!("{step}; tabref never does that on its own"))
}

#[cfg(test)]
#[path = "screenshot_tests.rs"]
mod tests;
