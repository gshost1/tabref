use super::*;

fn skipped_size(bytes: Option<Value>, urls: Value) {
    let mut reply = json!({"blind":false,"opaque":urls});
    if let Some(bytes) = bytes {
        reply["opaqueBytes"] = bytes;
    }
    let transport = ScriptedTransport::new().expect("Runtime.evaluate", evaluated(reply));
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());
    let watch = arm_capture_watch(
        &mut connection,
        &session(),
        "size",
        Deadline::after(1000).unwrap(),
    )
    .unwrap();
    assert_eq!(sent.lock().unwrap().len(), 1);
    assert!(watch.blind);
}

#[test]
fn unknown_or_malformed_opaque_sizes_make_no_css_calls() {
    for bytes in [
        None,
        Some(Value::Null),
        Some(json!(0)),
        Some(json!(-1)),
        Some(json!(1.5)),
        Some(json!("1024")),
    ] {
        skipped_size(bytes, json!(["https://example.org/a.css"]));
    }
    skipped_size(Some(json!(0)), json!([]));
}

#[test]
fn more_than_256_kib_of_opaque_css_makes_no_css_calls() {
    for bytes in [256 * 1024 + 1, u64::MAX] {
        skipped_size(Some(json!(bytes)), json!(["https://example.org/a.css"]));
    }
}

#[test]
fn known_sizes_at_or_below_the_limit_still_classify_and_fail_closed_on_cdp_errors() {
    for bytes in [1, 256 * 1024] {
        let transport = ScriptedTransport::new()
            .expect("Runtime.evaluate", evaluated(json!({"blind":true,"opaque":["https://example.org/a.css"],"opaqueBytes":bytes})))
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
                "size",
                Deadline::after(1000).unwrap()
            )
            .unwrap()
            .blind
        );
        assert!(
            sent.lock()
                .unwrap()
                .iter()
                .any(|call| call["method"] == "CSS.enable")
        );
    }
}
