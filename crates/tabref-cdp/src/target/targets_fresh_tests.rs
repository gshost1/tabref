use std::collections::VecDeque;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::{attach_to_target_with, create_background_tab};
use crate::{CdpConnection, CdpTransport, transport_timeout};

struct Starting {
    frames: VecDeque<String>,
    waiting: VecDeque<u64>,
}

impl CdpTransport for Starting {
    fn send_text(&mut self, text: &str, _: Deadline) -> Result<(), AppError> {
        let request: Value = serde_json::from_str(text)?;
        let id = request["id"].as_u64().unwrap();
        let result = match request["method"].as_str().unwrap() {
            "Target.createTarget" => json!({"targetId":"P"}),
            "Target.attachToTarget" => json!({"sessionId":format!("S{id}")}),
            "Target.getTargetInfo" => json!({}),
            _ => {
                self.waiting.push_back(id);
                return Ok(());
            }
        };
        self.frames
            .push_back(json!({"id":id,"result":result}).to_string());
        Ok(())
    }

    fn receive_text(&mut self, deadline: Deadline) -> Result<String, AppError> {
        if let Some(frame) = self.frames.pop_front() {
            return Ok(frame);
        }
        if deadline.remaining_ms() > 500
            && let Some(id) = self.waiting.pop_front()
        {
            return Ok(json!({"id":id,"result":{}}).to_string());
        }
        Err(transport_timeout("receive"))
    }
}

#[test]
fn a_launched_browser_or_new_target_gets_one_full_budget_attach() {
    for launched in [false, true] {
        let mut connection = CdpConnection::new(Box::new(Starting {
            frames: VecDeque::new(),
            waiting: VecDeque::new(),
        }));
        if launched {
            connection.defer_initial_attach_probe();
        } else {
            assert_eq!(
                create_background_tab(
                    &mut connection,
                    "about:blank",
                    Deadline::after(1000).unwrap()
                )
                .unwrap(),
                "P"
            );
        }
        let calls = [("Page.enable", json!({}))];
        assert!(
            attach_to_target_with(&mut connection, "P", &calls, Deadline::after(1000).unwrap())
                .is_ok()
        );
        let error =
            attach_to_target_with(&mut connection, "P", &calls, Deadline::after(1000).unwrap())
                .unwrap_err();
        assert_eq!(error.kind(), Some("cdp_page_unresponsive"));
    }
}
