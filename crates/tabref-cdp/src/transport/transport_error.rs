use tabref_core::{AppError, ErrorCode};

pub fn transport_timeout(operation: &str) -> AppError {
    AppError::timeout(format!(
        "The DevTools connection did not finish '{operation}' before the deadline"
    ))
    .with_details(serde_json::json!({ "kind": "cdp_timeout", "operation": operation }))
}

pub fn transport_closed(detail: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::ActionFailed, "The DevTools connection closed")
        .with_suggestion("Reconnect to the browser; it may have exited or dropped the session")
        .with_platform_detail(detail)
        .with_details(serde_json::json!({ "kind": "cdp_transport_closed" }))
}

pub fn transport_failed(operation: &str, detail: impl Into<String>) -> AppError {
    AppError::new(
        ErrorCode::ActionFailed,
        format!("The DevTools connection failed during '{operation}'"),
    )
    .with_platform_detail(detail)
    .with_details(serde_json::json!({ "kind": "cdp_transport_failed", "operation": operation }))
}

pub fn is_timeout(error: &AppError) -> bool {
    error.code == ErrorCode::Timeout
}
