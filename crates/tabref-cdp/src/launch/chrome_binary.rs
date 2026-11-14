use std::path::{Path, PathBuf};

use tabref_core::{AppError, ErrorCode};

/// The environment variable that names the browser binary to launch.
pub const CHROME_ENV: &str = "TABREF_CHROME";

#[cfg(target_os = "macos")]
const CANDIDATES: &[&str] = &[
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
];

#[cfg(target_os = "windows")]
const CANDIDATES: &[&str] = &[
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
];

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const CANDIDATES: &[&str] = &[
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "microsoft-edge",
];

/// The browser to launch: `$TABREF_CHROME` when set, otherwise the first
/// installed Chrome, Chromium or Edge in the usual places for this OS.
pub fn find_chrome() -> Result<PathBuf, AppError> {
    if let Some(configured) = std::env::var_os(CHROME_ENV).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(configured));
    }
    CANDIDATES
        .iter()
        .find_map(|candidate| locate(Path::new(candidate)))
        .ok_or_else(|| {
            AppError::new(
                ErrorCode::ActionFailed,
                "No Chrome or Chromium binary found",
            )
            .with_suggestion(format!(
                "Install Google Chrome, or set {CHROME_ENV} to the browser binary"
            ))
            .with_details(serde_json::json!({ "kind": "chrome_not_found" }))
        })
}

fn locate(candidate: &Path) -> Option<PathBuf> {
    if candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(candidate))
        .find(|full| full.is_file())
}
