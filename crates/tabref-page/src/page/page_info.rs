use serde::Serialize;
use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use crate::CdpConnection;
use tabref_cdp::transport_failed;

/// A tab's address and title as the browser reports them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
}

impl PageInfo {
    /// Reads `target_id`'s address and title with `Target.getTargetInfo`,
    /// a browser-level call that never waits on the page's own thread.
    pub fn read(
        connection: &mut CdpConnection,
        target_id: &str,
        deadline: Deadline,
    ) -> Result<Self, AppError> {
        let reply = connection.call(
            "Target.getTargetInfo",
            json!({ "targetId": target_id }),
            None,
            deadline,
        )?;
        let info = reply
            .get("targetInfo")
            .ok_or_else(|| transport_failed("Target.getTargetInfo", "reply has no targetInfo"))?;
        let text = |key: &str| {
            info.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Ok(Self {
            url: text("url"),
            title: text("title"),
        })
    }
}
