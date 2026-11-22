use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tabref_cdp::{CdpConnection, CdpTransport, PageSession, ScriptedTransport};
use tabref_core::{AppError, Deadline};

use super::super::{CaptureWatch, IDLE_WAIT_MS, arm_capture_watch};

#[path = "capture_watch_opaque_size_tests.rs"]
mod opaque_size;

fn session() -> PageSession {
    PageSession {
        target_id: "T".into(),
        session_id: "S".into(),
    }
}

fn evaluated(value: Value) -> Value {
    json!({"result":{"type":"object", "value":value}})
}

#[test]
fn opaque_preparation_retains_its_shared_budget_and_cleanup_allowance() {
    struct Timed {
        inner: ScriptedTransport,
        seen: Arc<Mutex<Vec<(String, u64)>>>,
    }
    impl CdpTransport for Timed {
        fn send_text(&mut self, text: &str, deadline: Deadline) -> Result<(), AppError> {
            let message: Value = serde_json::from_str(text)?;
            self.seen.lock().unwrap().push((
                message["method"].as_str().unwrap().to_owned(),
                deadline.remaining_ms(),
            ));
            self.inner.send_text(text, deadline)
        }
        fn receive_text(&mut self, deadline: Deadline) -> Result<String, AppError> {
            self.inner.receive_text(deadline)
        }
    }
    let transport = ScriptedTransport::new()
        .expect("Runtime.evaluate", evaluated(json!({"blind":true,"opaque":["https://example.org/a.css"],"opaqueBytes":12})))
        .expect("DOM.enable", json!({}))
        .expect_with_events("CSS.enable", vec![json!({"method":"CSS.styleSheetAdded","sessionId":"S","params":{"header":{
            "sourceURL":"https://example.org/a.css","isInline":false,"length":3,"styleSheetId":"a"
        }}})], json!({"result":{}}))
        .expect("CSS.getStyleSheetText", json!({"text":"a{}"}))
        .expect("CSS.disable", json!({}))
        .expect("DOM.disable", json!({}))
        .expect("Runtime.evaluate", evaluated(json!([["https://example.org/a.css",[]]])))
        .expect("Runtime.evaluate", evaluated(Value::Null))
        .expect("Runtime.evaluate", evaluated(json!({"blind":false})));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut connection = CdpConnection::new(Box::new(Timed {
        inner: transport,
        seen: Arc::clone(&seen),
    }));
    assert!(
        !arm_capture_watch(
            &mut connection,
            &session(),
            "budget",
            Deadline::after(500).unwrap()
        )
        .unwrap()
        .blind
    );
    for (method, remaining) in seen.lock().unwrap().iter() {
        let minimum = if method.ends_with(".disable") {
            50
        } else {
            200
        };
        assert!(*remaining > minimum, "{method}: {remaining} ms");
    }
}

#[test]
fn another_blind_reason_skips_opaque_preparation() {
    let transport = ScriptedTransport::new().expect(
        "Runtime.evaluate",
        evaluated(json!({
            "blind":true,"why":"animation:pulse","opaque":["https://example.org/a.css"],"opaqueBytes":12
        })),
    );
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());
    assert!(
        arm_capture_watch(
            &mut connection,
            &session(),
            "animation",
            Deadline::after(1000).unwrap()
        )
        .unwrap()
        .blind
    );
    assert_eq!(sent.lock().unwrap().len(), 1);
}

#[test]
fn rejection_stops_before_fetching_the_next_sheet_or_rearming() {
    for first in ["a:has(b){color:red}", "a{color:red}"] {
        let headers = ["a", "b"].map(|id| json!({"method":"CSS.styleSheetAdded","sessionId":"S","params":{"header":{
            "sourceURL":format!("https://example.org/{id}.css"),"isInline":false,"length":first.len(),"styleSheetId":id
        }}})).to_vec();
        let mut transport = ScriptedTransport::new()
            .expect("Runtime.evaluate", evaluated(json!({"blind":true,"opaque":["https://example.org/a.css","https://example.org/b.css"],"opaqueBytes":24})))
            .expect("DOM.enable", json!({}))
            .expect_with_events("CSS.enable", headers, json!({"result":{}}))
            .expect("CSS.getStyleSheetText", json!({"text":first}));
        if first == "a{color:red}" {
            transport = transport.expect(
                "CSS.getStyleSheetText",
                json!({"text":"a:has(b){color:red}"}),
            );
        }
        let transport = transport
            .expect("CSS.disable", json!({}))
            .expect("DOM.disable", json!({}));
        let sent = transport.sent_log();
        let mut connection = CdpConnection::new(transport.boxed());
        assert!(
            arm_capture_watch(
                &mut connection,
                &session(),
                "reject",
                Deadline::after(1000).unwrap()
            )
            .unwrap()
            .blind
        );
        assert!(
            !sent
                .lock()
                .unwrap()
                .iter()
                .skip(1)
                .any(|call| call["method"] == "Runtime.evaluate")
        );
    }
}

#[test]
fn no_opaque_sheets_make_only_the_original_arm_evaluation() {
    for blind in [false, true] {
        let transport =
            ScriptedTransport::new().expect("Runtime.evaluate", evaluated(json!({"blind":blind})));
        let sent = transport.sent_log();
        let mut connection = CdpConnection::new(transport.boxed());
        let watch = CaptureWatch::arm_with_budget(
            &mut connection,
            &session(),
            "token",
            Deadline::after(1000).unwrap(),
            std::time::Duration::from_millis(IDLE_WAIT_MS + 11),
        )
        .unwrap();
        assert_eq!(watch.blind, blind);
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["method"], "Runtime.evaluate");
    }
}

#[test]
fn css_errors_do_not_approve_an_opaque_sheet() {
    let transport = ScriptedTransport::new()
        .expect(
            "Runtime.evaluate",
            evaluated(
                json!({"blind":true,"opaque":["https://example.org/a.css"],"opaqueBytes":12}),
            ),
        )
        .expect("DOM.enable", json!({}))
        .expect_error("CSS.enable", -32000, "CSS unavailable")
        .expect("CSS.disable", json!({}))
        .expect("DOM.disable", json!({}));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());
    assert!(
        arm_capture_watch(
            &mut connection,
            &session(),
            "token",
            Deadline::after(1000).unwrap()
        )
        .unwrap()
        .blind
    );
    assert_eq!(sent.lock().unwrap().len(), 5);
}

#[test]
fn dropped_events_cannot_approve_even_a_safe_sheet() {
    let events = (0..1100)
        .map(|_| json!({"method":"CSS.styleSheetAdded", "sessionId":"S", "params":{}}))
        .collect();
    let transport = ScriptedTransport::new()
        .expect(
            "Runtime.evaluate",
            evaluated(
                json!({"blind":true,"opaque":["https://example.org/a.css"],"opaqueBytes":12}),
            ),
        )
        .expect("DOM.enable", json!({}))
        .expect_with_events("CSS.enable", events, json!({"result":{}}))
        .expect("CSS.disable", json!({}))
        .expect("DOM.disable", json!({}));
    let mut connection = CdpConnection::new(transport.boxed());
    assert!(
        arm_capture_watch(
            &mut connection,
            &session(),
            "token",
            Deadline::after(1000).unwrap()
        )
        .unwrap()
        .blind
    );
    assert!(connection.dropped_event_count() > 0);
}

#[test]
fn font_inventory_events_do_not_discard_classified_rules() {
    let transport = ScriptedTransport::new()
        .expect("Runtime.evaluate", evaluated(json!({"blind":true,"opaque":["https://example.org/a.css"],"opaqueBytes":12})))
        .expect("DOM.enable", json!({}))
        .expect_with_events("CSS.enable", vec![json!({"method":"CSS.styleSheetAdded","sessionId":"S","params":{"header":{"sourceURL":"https://example.org/a.css","isInline":false,"length":12,"styleSheetId":"sheet"}}})], json!({"result":{}}))
        .expect_with_events("CSS.getStyleSheetText", vec![json!({"method":"CSS.fontsUpdated","sessionId":"S","params":{"font":{"fontFamily":"Example"}}})], json!({"result":{"text":"a{color:red}"}}))
        .expect("CSS.disable", json!({}))
        .expect("DOM.disable", json!({}))
        .expect("Runtime.evaluate", evaluated(json!([["https://example.org/a.css",[]]])))
        .expect("Runtime.evaluate", evaluated(Value::Null))
        .expect("Runtime.evaluate", evaluated(json!({"blind":false})));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());
    assert!(
        !arm_capture_watch(
            &mut connection,
            &session(),
            "token",
            Deadline::after(1000).unwrap()
        )
        .unwrap()
        .blind
    );
    let sent = sent.lock().unwrap();
    assert!(
        sent[0]["params"]["expression"]
            .as_str()
            .unwrap()
            .contains("if (document.visibilityState === 'hidden') arm();")
    );
    assert!(
        sent.last().unwrap()["params"]["expression"]
            .as_str()
            .unwrap()
            .contains("if (true) arm();")
    );
}

#[test]
fn late_stylesheet_events_still_refuse_approval() {
    for method in [
        "CSS.styleSheetChanged",
        "CSS.styleSheetRemoved",
        "CSS.styleSheetAdded",
        "CSS.mediaQueryResultChanged",
        "CSS.unknown",
    ] {
        let transport = ScriptedTransport::new()
            .expect(
                "Runtime.evaluate",
                evaluated(
                    json!({"blind":true,"opaque":["https://example.org/a.css"],"opaqueBytes":12}),
                ),
            )
            .expect("DOM.enable", json!({}))
            .expect("CSS.enable", json!({}))
            .expect_with_events(
                "CSS.disable",
                vec![json!({"method":method,"sessionId":"S","params":{}})],
                json!({"result":{}}),
            )
            .expect("DOM.disable", json!({}));
        let mut connection = CdpConnection::new(transport.boxed());
        assert!(
            arm_capture_watch(
                &mut connection,
                &session(),
                "token",
                Deadline::after(1000).unwrap()
            )
            .unwrap()
            .blind,
            "{method}"
        );
    }
}
