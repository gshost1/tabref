use serde_json::{Value, json};
use tabref_cdp::{CdpConnection, PageSession, ScriptedTransport};
use tabref_core::Deadline;

use super::super::{CaptureWatch, IDLE_WAIT_MS, arm_capture_watch};

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
            evaluated(json!({"blind":true,"opaque":["https://example.org/a.css"]})),
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
            evaluated(json!({"blind":true,"opaque":["https://example.org/a.css"]})),
        )
        .expect("DOM.enable", json!({}))
        .expect_with_events("CSS.enable", events, json!({"result":{}}))
        .expect("CSS.disable", json!({}))
        .expect("DOM.disable", json!({}))
        .expect(
            "Runtime.evaluate",
            evaluated(json!({"blind":true,"opaque":["https://example.org/a.css"]})),
        );
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
        .expect("Runtime.evaluate", evaluated(json!({"blind":true,"opaque":["https://example.org/a.css"]})))
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
                evaluated(json!({"blind":true,"opaque":["https://example.org/a.css"]})),
            )
            .expect("DOM.enable", json!({}))
            .expect("CSS.enable", json!({}))
            .expect_with_events(
                "CSS.disable",
                vec![json!({"method":method,"sessionId":"S","params":{}})],
                json!({"result":{}}),
            )
            .expect("DOM.disable", json!({}))
            .expect("Runtime.evaluate", evaluated(json!({"blind":true})));
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
