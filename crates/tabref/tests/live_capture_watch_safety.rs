//! Opt-in: TABREF_LIVE=1 cargo test -p tabref --test live_capture_watch_safety.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use serde_json::json;
use tabref_cdp::{CdpConnection, PageSession, attach_to_target, connect, list_page_targets};
use tabref_core::Deadline;
use tabref_page::{arm_capture_watch, check_capture_watch};

fn evaluate(connection: &mut CdpConnection, page: &PageSession, expression: &str) {
    let result = connection
        .call(
            "Runtime.evaluate",
            json!({ "expression": expression, "awaitPromise": true }),
            Some(&page.session_id),
            Deadline::after(30_000).unwrap(),
        )
        .unwrap();
    assert!(result.get("exceptionDetails").is_none(), "{result}");
}

fn page() -> (common::Harness, CdpConnection, PageSession) {
    let harness = common::harness("watch-safety");
    let deadline = Deadline::after(30_000).unwrap();
    let mut connection = connect(&harness.browser.endpoint().unwrap(), deadline).unwrap();
    let target = list_page_targets(&mut connection, deadline)
        .unwrap()
        .remove(0);
    let page = attach_to_target(&mut connection, &target.target_id, deadline).unwrap();
    evaluate(
        &mut connection,
        &page,
        "document.body.innerHTML='<input><div id=host></div><iframe></iframe>'; host.attachShadow({mode:'open'}).innerHTML='<input>'; document.querySelector('iframe').contentDocument.body.innerHTML='<input>'",
    );
    (harness, connection, page)
}

#[test]
fn validity_and_validation_message_changes_are_seen_in_every_root() {
    if !common::live() {
        return;
    }
    let (_harness, mut connection, page) = page();
    for root in [
        "document",
        "host.shadowRoot",
        "document.querySelector('iframe').contentDocument",
    ] {
        for message in ["first error", "second error", ""] {
            let deadline = Deadline::after(30_000).unwrap();
            let watch = arm_capture_watch(&mut connection, &page, "validity", deadline).unwrap();
            assert!(!watch.blind);
            assert_eq!(
                check_capture_watch(&mut connection, &page, &watch, deadline),
                "same"
            );
            evaluate(
                &mut connection,
                &page,
                &format!(
                    "{root}.querySelector('input').setCustomValidity({})",
                    json!(message)
                ),
            );
            assert_ne!(
                check_capture_watch(&mut connection, &page, &watch, deadline),
                "same",
                "{root} {message}"
            );
        }
    }
}

#[test]
fn finished_animations_are_compared_by_identity_state_and_time_in_every_root() {
    if !common::live() {
        return;
    }
    let (_harness, mut connection, page) = page();
    for root in [
        "document",
        "host.shadowRoot",
        "document.querySelector('iframe').contentDocument",
    ] {
        let target = format!("{root}.querySelector('input')");
        evaluate(
            &mut connection,
            &page,
            &format!(
                "window.anim={target}.animate([{{opacity:1}},{{opacity:1}}],{{duration:100,fill:'forwards'}}); anim.finish()"
            ),
        );
        for edit in [
            "anim.currentTime=120".to_owned(),
            "anim.pause();anim.ready.then(()=>true)".to_owned(),
            format!(
                "anim.cancel();window.anim={target}.animate([{{opacity:1}},{{opacity:1}}],{{duration:100,fill:'forwards'}});anim.finish()"
            ),
            "anim.cancel()".to_owned(),
        ] {
            let deadline = Deadline::after(30_000).unwrap();
            let watch = arm_capture_watch(&mut connection, &page, "animation", deadline).unwrap();
            assert!(!watch.blind, "{root}: {edit}");
            assert_eq!(
                check_capture_watch(&mut connection, &page, &watch, deadline),
                "same"
            );
            evaluate(&mut connection, &page, &edit);
            assert_ne!(
                check_capture_watch(&mut connection, &page, &watch, deadline),
                "same",
                "{root}: {edit}"
            );
        }
    }
}

#[test]
fn upgraded_custom_elements_are_blind_in_every_root() {
    if !common::live() {
        return;
    }
    let (_harness, mut connection, page) = page();
    for (index, root) in [
        "document",
        "host.shadowRoot",
        "document.querySelector('iframe').contentDocument",
    ]
    .iter()
    .enumerate()
    {
        evaluate(
            &mut connection,
            &page,
            &format!(
                "{{let root={root};let realm=(root.ownerDocument??root).defaultView;realm.customElements.define('x-secret-{index}',class extends realm.HTMLElement {{constructor(){{super();this.attachInternals().role='button'}}}});(root.body??root).appendChild(realm.document.createElement('x-secret-{index}'));}}"
            ),
        );
        let watch = arm_capture_watch(
            &mut connection,
            &page,
            "internals",
            Deadline::after(30_000).unwrap(),
        )
        .unwrap();
        assert!(watch.blind, "{root}");
        evaluate(
            &mut connection,
            &page,
            &format!("{root}.querySelector('x-secret-{index}').remove()"),
        );
    }
}

#[test]
fn internals_and_registry_calls_invalidate_an_armed_watch_in_every_realm() {
    if !common::live() {
        return;
    }
    let (_harness, mut connection, page) = page();
    for realm in ["window", "document.querySelector('iframe').contentWindow"] {
        for edit in [
            format!(
                "{{let realm={realm};try{{realm.HTMLElement.prototype.attachInternals.call(realm.document.body)}}catch{{}}}}"
            ),
            format!(
                "{{let realm={realm};realm.customElements.upgrade(realm.document.createElement('div'))}}"
            ),
            format!(
                "{{let realm={realm};realm.customElements.define('x-later',class extends realm.HTMLElement {{}})}}"
            ),
        ] {
            let deadline = Deadline::after(30_000).unwrap();
            let watch = arm_capture_watch(&mut connection, &page, "hooks", deadline).unwrap();
            assert!(!watch.blind);
            evaluate(&mut connection, &page, &edit);
            assert_ne!(
                check_capture_watch(&mut connection, &page, &watch, deadline),
                "same",
                "{edit}"
            );
        }
    }
}

#[test]
fn a_page_above_the_full_visibility_scan_cap_is_blind() {
    if !common::live() {
        return;
    }
    let (_harness, mut connection, page) = page();
    evaluate(
        &mut connection,
        &page,
        "document.body.innerHTML='<span></span>'.repeat(30001)",
    );
    let watch = arm_capture_watch(
        &mut connection,
        &page,
        "oversize",
        Deadline::after(30_000).unwrap(),
    )
    .unwrap();
    assert!(watch.blind);
}
