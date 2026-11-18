use tabref_core::{AppError, DeliverySemantics, ErrorCode};

pub(crate) fn endpoint_unreachable(port: u16, detail: &str) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!("No DevTools server is listening on 127.0.0.1:{port}"),
    )
    .with_suggestion(
        "Start the browser with remote debugging enabled; a DevToolsActivePort file left by \
         an exited browser names a port nothing listens on",
    )
    .with_platform_detail(detail)
    .with_details(serde_json::json!({ "kind": "cdp_endpoint_unavailable", "port": port }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// A port that answers HTTP but not the DevTools `path` asked for. Chrome
/// answers `/json/version` only when started with
/// `--remote-debugging-port`; with remote debugging turned on at
/// chrome://inspect the same port takes WebSocket connections alone.
pub(crate) fn no_http_route(port: u16, path: &str, status: u16) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!(
            "Port {port} answered {path} with HTTP {status}: a browser is there, but it gives no DevTools address over HTTP"
        ),
    )
    .with_suggestion(format!(
        "A browser with remote debugging turned on at chrome://inspect speaks WebSocket only. \
         For your own browser run 'tabref connect --user-chrome' (--user-data-dir DIR names \
         one tabref does not know), or pass --cdp ws://127.0.0.1:{port}<path>, the path being \
         line 2 of its profile's DevToolsActivePort file"
    ))
    .with_details(serde_json::json!({
        "kind": "cdp_no_http_route",
        "port": port,
        "http_status": status,
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// The refusal for a port whose listening sockets changed between the owner
/// check and the connection: whatever listens now was never checked.
pub(crate) fn port_changed_hands(port: u16) -> AppError {
    AppError::new(
        ErrorCode::PermDenied,
        format!("Port {port} changed hands between tabref's owner check and its connection"),
    )
    .with_suggestion("Run the command again; tabref checks the port's owner each time it connects")
    .with_details(serde_json::json!({ "kind": "cdp_endpoint_changed", "port": port }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// The refusal for an endpoint whose WebSocket address is not the checked
/// port on 127.0.0.1: connecting there would reach whatever listens at an
/// address the owner check never looked at.
pub(crate) fn endpoint_mismatch(checked: u16, advertised: &str) -> AppError {
    AppError::new(
        ErrorCode::PermDenied,
        format!(
            "The DevTools endpoint for port {checked} points at {advertised}, which tabref did not check"
        ),
    )
    .with_suggestion("Run 'tabref open <url> --launch' for a browser of tabref's own")
    .with_details(serde_json::json!({
        "kind": "cdp_endpoint_mismatch",
        "port": checked,
        "advertised": advertised,
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

pub(crate) fn handshake_rejected(status: u16, port: u16) -> AppError {
    AppError::new(
        ErrorCode::PermDenied,
        format!("The browser refused the DevTools connection on port {port} (HTTP {status})"),
    )
    .with_suggestion(
        "Allow the remote-debugging connection in the browser, or enable remote debugging \
         at chrome://inspect/#remote-debugging, then retry",
    )
    .with_details(serde_json::json!({
        "kind": "cdp_connection_rejected",
        "port": port,
        "http_status": status,
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

pub(crate) fn approval_pending(port: u16, elapsed_ms: u128) -> AppError {
    AppError::new(
        ErrorCode::PermDenied,
        format!(
            "The browser on port {port} did not approve the DevTools connection before the deadline"
        ),
    )
    .with_suggestion(
        "Accept the browser's 'Allow remote debugging' prompt, then retry with a longer timeout",
    )
    .with_details(serde_json::json!({
        "kind": "cdp_approval_pending",
        "port": port,
        "elapsed_ms": elapsed_ms,
        "retryable": true,
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

pub(crate) fn connection_declined(port: u16, detail: &str) -> AppError {
    AppError::new(
        ErrorCode::PermDenied,
        format!("The browser on port {port} closed the DevTools connection before it was approved"),
    )
    .with_suggestion("The remote-debugging prompt was likely declined; accept it and retry")
    .with_platform_detail(detail)
    .with_details(serde_json::json!({ "kind": "cdp_connection_rejected", "port": port }))
    .with_disposition(DeliverySemantics::not_delivered())
}
