use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::cdp_transport::CdpTransport;
use super::transport_error::transport_timeout;

/// A fake transport for unit tests, compiled only for tests and the
/// `scripted` feature. Each expectation names the CDP method the next
/// request must carry and the reply object (`result` or `error`) to answer it
/// with; unsolicited frames queue ahead of that reply. Every request is
/// logged so a test can inspect what was sent, and a request out of order
/// fails its call. Dropping the transport with expectations left over fails
/// the test.
#[derive(Default)]
pub struct ScriptedTransport {
    expectations: VecDeque<(String, Vec<Value>, Value)>,
    inbound: VecDeque<String>,
    sent: Arc<Mutex<Vec<Value>>>,
    violations: Vec<String>,
}

impl ScriptedTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn expect(self, method: &str, result: Value) -> Self {
        self.expect_with_events(method, Vec::new(), json!({ "result": result }))
    }

    pub fn expect_error(self, method: &str, code: i64, message: &str) -> Self {
        self.expect_with_events(
            method,
            Vec::new(),
            json!({ "error": { "code": code, "message": message } }),
        )
    }

    pub fn expect_with_events(mut self, method: &str, events: Vec<Value>, reply: Value) -> Self {
        self.expectations
            .push_back((method.to_owned(), events, reply));
        self
    }

    /// A request that is sent but never answered.
    pub fn expect_silence(self, method: &str) -> Self {
        self.expect_with_events(method, Vec::new(), Value::Null)
    }

    pub fn push_inbound(mut self, frame: Value) -> Self {
        self.inbound.push_back(frame.to_string());
        self
    }

    pub fn sent_log(&self) -> Arc<Mutex<Vec<Value>>> {
        Arc::clone(&self.sent)
    }

    pub fn boxed(self) -> Box<dyn CdpTransport> {
        Box::new(self)
    }
}

impl CdpTransport for ScriptedTransport {
    fn send_text(&mut self, text: &str, _deadline: Deadline) -> Result<(), AppError> {
        let request: Value = serde_json::from_str(text)?;
        if let Ok(mut sent) = self.sent.lock() {
            sent.push(request.clone());
        }
        let method = request["method"].as_str().unwrap_or_default().to_owned();
        let Some((expected, events, mut reply)) = self.expectations.pop_front() else {
            let violation = format!("unexpected CDP request {method}");
            self.violations.push(violation.clone());
            return Err(AppError::internal(violation));
        };
        if method != expected {
            let violation = format!("CDP request order: sent {method}, expected {expected}");
            self.violations.push(violation.clone());
            return Err(AppError::internal(violation));
        }
        for event in events {
            self.inbound.push_back(event.to_string());
        }
        if reply.is_null() {
            return Ok(());
        }
        reply["id"] = request["id"].clone();
        if let Some(session) = request.get("sessionId") {
            reply["sessionId"] = session.clone();
        }
        self.inbound.push_back(reply.to_string());
        Ok(())
    }

    fn receive_text(&mut self, _deadline: Deadline) -> Result<String, AppError> {
        self.inbound
            .pop_front()
            .ok_or_else(|| transport_timeout("receive"))
    }

    fn poll_text(&mut self) -> Result<Option<String>, AppError> {
        Ok(self.inbound.pop_front())
    }
}

impl Drop for ScriptedTransport {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let pending: Vec<&String> = self.expectations.iter().map(|(m, _, _)| m).collect();
            assert!(pending.is_empty(), "unsent expected requests: {pending:?}");
            assert!(self.violations.is_empty(), "{:?}", self.violations);
        }
    }
}
