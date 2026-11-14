use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tabref_core::AppError;

use crate::{BrowserEndpoint, EndpointSource, WebSocketUrl};

/// A browser tabref started, as it records it so later invocations reach
/// the same browser without being told where it listens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchedBrowser {
    pub pid: u32,
    pub port: u16,
    pub websocket_path: String,
    pub profile_dir: PathBuf,
    pub headless: bool,
}

impl LaunchedBrowser {
    pub fn endpoint(&self) -> Result<BrowserEndpoint, AppError> {
        Ok(BrowserEndpoint {
            websocket_url: WebSocketUrl::loopback(self.port, &self.websocket_path)?,
            source: EndpointSource::Launched,
            product: None,
        })
    }
}
