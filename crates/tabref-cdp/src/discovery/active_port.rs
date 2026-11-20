use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use tabref_core::{AppError, DeliverySemantics, ErrorCode};

const ACTIVE_PORT_FILE: &str = "DevToolsActivePort";
const MAX_FILE_BYTES: u64 = 4096;

/// The contents of a browser profile's `DevToolsActivePort` file: the port
/// on line one and the browser WebSocket path on line two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivePort {
    pub port: u16,
    pub websocket_path: String,
}

/// Reads `<user_data_dir>/DevToolsActivePort`. The file is opened read-only
/// and is never rewritten or removed: the browser owns it, and a client that
/// deletes it breaks every later client of the same browser.
pub fn read_active_port(user_data_dir: &Path) -> Result<ActivePort, AppError> {
    let path = user_data_dir.join(ACTIVE_PORT_FILE);
    let file =
        open_active_port(user_data_dir).map_err(|error| missing(&path, &error.to_string()))?;
    let text = read_text(file).map_err(|error| malformed(&path, &error.to_string()))?;
    parse_active_port(&text).map_err(|error| {
        error.with_details(serde_json::json!({
            "kind": "cdp_active_port_malformed",
            "path": path.display().to_string(),
        }))
    })
}

/// Opens the profile's ordinary port file without following a link.
pub(crate) fn open_active_port(profile: &Path) -> io::Result<File> {
    #[cfg(unix)]
    {
        use rustix::fs::{Mode, OFlags, fstat, open, openat};
        let dir = open(
            profile,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let owner = fstat(&dir)?.st_uid;
        if owner != rustix::process::geteuid().as_raw() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the profile directory belongs to another user",
            ));
        }
        let file = openat(
            dir,
            ACTIVE_PORT_FILE,
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        ordinary_file(file.into())
    }
    #[cfg(not(unix))]
    {
        let path = profile.join(ACTIVE_PORT_FILE);
        if !std::fs::symlink_metadata(&path)?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "DevToolsActivePort is not a regular file",
            ));
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
            options.custom_flags(OPEN_REPARSE_POINT);
        }
        ordinary_file(options.open(path)?)
    }
}

fn ordinary_file(file: File) -> io::Result<File> {
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "DevToolsActivePort is not a regular file",
        ));
    }
    #[cfg(windows)]
    if super::windows_profile_owner::is_reparse_point(&metadata) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "DevToolsActivePort is a reparse point",
        ));
    }
    Ok(file)
}

pub(crate) fn read_text(file: File) -> io::Result<String> {
    let mut text = String::new();
    file.take(MAX_FILE_BYTES).read_to_string(&mut text)?;
    Ok(text)
}

/// Parses the two-line `DevToolsActivePort` format.
pub fn parse_active_port(text: &str) -> Result<ActivePort, AppError> {
    let mut lines = text.lines().map(str::trim);
    let port = lines
        .next()
        .and_then(|line| line.parse::<u16>().ok())
        .filter(|port| *port != 0)
        .ok_or_else(|| malformed(Path::new(ACTIVE_PORT_FILE), "line 1 is not a port"))?;
    let websocket_path = lines
        .next()
        .filter(|line| line.starts_with('/') && line.len() > 1)
        .ok_or_else(|| {
            malformed(
                Path::new(ACTIVE_PORT_FILE),
                "line 2 is not a WebSocket path",
            )
        })?
        .to_owned();
    Ok(ActivePort {
        port,
        websocket_path,
    })
}

fn missing(path: &Path, detail: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!("No DevToolsActivePort file at {}", path.display()),
    )
    .with_suggestion(
        "Enable remote debugging at chrome://inspect/#remote-debugging, or start the browser \
         with --remote-debugging-port, and pass its user-data directory",
    )
    .with_platform_detail(detail)
    .with_details(serde_json::json!({
        "kind": "cdp_active_port_missing",
        "path": path.display().to_string(),
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

fn malformed(path: &Path, detail: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "The DevToolsActivePort file at {} is malformed",
            path.display()
        ),
    )
    .with_platform_detail(detail)
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "active_port_tests.rs"]
mod tests;
