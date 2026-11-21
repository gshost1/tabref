use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline, ErrorCode};

use super::attach_to_target_with;
use crate::{CdpConnection, CdpTransport, transport_timeout};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Blocked,
    Recover,
    BrowserSlow,
    BrowserError,
    Partial,
    ConfirmProgress,
    Daemon,
    SendFailure,
    PageError,
    ReleaseOnly,
}

struct Delayed {
    mode: Mode,
    frames: VecDeque<String>,
    empty_reads: usize,
    sent: Arc<Mutex<Vec<Value>>>,
    budgets: Arc<Mutex<Vec<u64>>>,
}

fn reply(id: u64) -> String {
    json!({"id":id,"result":{}}).to_string()
}

impl CdpTransport for Delayed {
    fn send_text(&mut self, text: &str, _: Deadline) -> Result<(), AppError> {
        let value: Value = serde_json::from_str(text)?;
        self.sent.lock().unwrap().push(value.clone());
        match value["method"].as_str().unwrap() {
            "Target.attachToTarget" => self
                .frames
                .push_back(json!({"id":value["id"],"result":{"sessionId":"S"}}).to_string()),
            "Page.enable" if self.mode == Mode::SendFailure => {
                return Err(AppError::internal("send failed"));
            }
            "Page.enable" if self.mode == Mode::PageError => self
                .frames
                .push_back(json!({"id":3,"error":{"code":-32000,"message":"denied"}}).to_string()),
            "Page.enable" if self.mode == Mode::Partial => self.frames.push_back(reply(3)),
            "Target.getTargetInfo" if self.mode == Mode::BrowserError => self.frames.push_back(
                json!({"id":value["id"],"error":{"code":-32000,"message":"unavailable"}})
                    .to_string(),
            ),
            "Target.getTargetInfo" if self.mode != Mode::BrowserSlow => {
                self.frames.push_back(reply(value["id"].as_u64().unwrap()));
                if matches!(self.mode, Mode::Recover | Mode::ConfirmProgress) {
                    self.frames.push_back(reply(2));
                    if self.mode == Mode::Recover {
                        self.frames.push_back(reply(3));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn receive_text(&mut self, deadline: Deadline) -> Result<String, AppError> {
        self.budgets.lock().unwrap().push(deadline.remaining_ms());
        if let Some(frame) = self.frames.pop_front() {
            return Ok(frame);
        }
        self.empty_reads += 1;
        if self.mode == Mode::ReleaseOnly && deadline.remaining_ms() > 500 {
            return Ok(reply(2));
        }
        match (self.mode, self.empty_reads) {
            (Mode::Partial, 2) => Ok(reply(2)),
            (Mode::ConfirmProgress, 3) => Ok(reply(3)),
            (Mode::BrowserSlow, 3) | (Mode::BrowserError, 2) => {
                self.frames.push_back(reply(3));
                Ok(reply(2))
            }
            _ => Err(transport_timeout("receive")),
        }
    }
}

#[test]
fn staged_waits_preserve_pending_replies_and_only_fail_after_confirmation() {
    for mode in [
        Mode::Blocked,
        Mode::Recover,
        Mode::BrowserSlow,
        Mode::BrowserError,
        Mode::Partial,
        Mode::ConfirmProgress,
        Mode::Daemon,
        Mode::SendFailure,
        Mode::PageError,
        Mode::ReleaseOnly,
    ] {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let budgets = Arc::new(Mutex::new(Vec::new()));
        let mut connection = CdpConnection::new(Box::new(Delayed {
            mode,
            frames: VecDeque::new(),
            empty_reads: 0,
            sent: sent.clone(),
            budgets: budgets.clone(),
        }));
        if mode == Mode::Daemon {
            connection.set_attach_probe(false);
        }
        let follow_ups = [("Page.enable", json!({}))];
        let result = attach_to_target_with(
            &mut connection,
            "P",
            if mode == Mode::ReleaseOnly {
                &[]
            } else {
                &follow_ups
            },
            Deadline::after(1000).unwrap(),
        );
        let sent = sent.lock().unwrap();
        let checked = sent
            .iter()
            .any(|value| value["method"] == "Target.getTargetInfo");
        let budgets = budgets.lock().unwrap();
        assert_eq!(
            checked,
            matches!(
                mode,
                Mode::Blocked
                    | Mode::Recover
                    | Mode::BrowserSlow
                    | Mode::BrowserError
                    | Mode::ConfirmProgress
            )
        );
        match mode {
            Mode::Blocked => {
                assert_eq!(result.unwrap_err().code, ErrorCode::Timeout);
                assert!(budgets.iter().any(|ms| *ms <= 35));
                assert!(budgets.last().unwrap() <= &70);
                assert!(
                    connection
                        .next_reply(&[2, 3], Deadline::after(10).unwrap())
                        .unwrap_err()
                        .message
                        .contains("not in flight")
                );
            }
            Mode::Daemon => {
                assert_eq!(result.unwrap_err().code, ErrorCode::Timeout);
                assert!(budgets.iter().all(|ms| *ms > 500));
            }
            Mode::SendFailure | Mode::PageError => {
                let error = result.unwrap_err();
                assert_ne!(error.code, ErrorCode::Timeout);
                assert!(error.suggestion.is_none());
                assert!(
                    connection
                        .next_reply(&[2], Deadline::after(10).unwrap())
                        .unwrap_err()
                        .message
                        .contains("not in flight")
                );
            }
            _ => {
                assert!(result.is_ok());
                if matches!(
                    mode,
                    Mode::BrowserSlow
                        | Mode::BrowserError
                        | Mode::Partial
                        | Mode::ConfirmProgress
                        | Mode::ReleaseOnly
                ) {
                    assert!(budgets.last().unwrap() > &500);
                }
            }
        }
    }
}
