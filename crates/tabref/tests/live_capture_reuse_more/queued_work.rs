use super::{STILL, common, open, run, snapshot_id, stop};
use crate::common::live;

const QUEUED: &str = "{ setTimeout(() => { \
fetch(location.href).then(reply => reply.text()).then(() => { const p = document.createElement('p'); p.textContent = 'Queued needle words'; document.body.append(p); }); \
const until = performance.now() + 40; while (performance.now() < until); }, 0); 1 }";

const HIDDEN: &str = r"{
    let started;
    Object.defineProperty(document, 'visibilityState', {
        get() { started ??= performance.now(); return 'hidden'; },
        configurable: true
    });
    window.requestIdleCallback = run => {
        started ??= performance.now();
        return setTimeout(run, 50);
    };
    const observe = MutationObserver.prototype.observe;
    MutationObserver.prototype.observe = function (...args) {
        if (started !== undefined) window.armWait ??= performance.now() - started;
        return observe.apply(this, args);
    };
    1
}";

#[test]
fn work_queued_before_a_capture_runs_before_the_watch_is_armed() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-queued");
    for round in 0..3 {
        open(&tabref, STILL);
        run(&tabref, &["js", QUEUED], &[]);
        let snapped = run(&tabref, &["snap"], &[]);
        assert!(
            snapped.stdout.contains("Queued needle"),
            "round {round}: the capture missed work queued before it:\n{}",
            snapped.stdout
        );
        let found = run(&tabref, &["find", "needle"], &[]);
        assert_eq!(
            snapshot_id(&found),
            snapshot_id(&snapped),
            "round {round}: the queued work spoiled the kept capture"
        );
    }
    stop(&tabref);
}

#[test]
fn a_hidden_tab_arms_the_watch_without_waiting_for_idle() {
    if !live() {
        return;
    }
    let tabref = common::harness("reuse-hidden");
    open(&tabref, STILL);
    run(&tabref, &["js", HIDDEN], &[]);
    let snapped = run(&tabref, &["snap"], &[]);
    let found = run(&tabref, &["find", "needle"], &[]);
    assert_eq!(
        snapshot_id(&found),
        snapshot_id(&snapped),
        "the hidden tab's capture was not kept"
    );
    let timing = run(&tabref, &["js", "window.armWait ?? null"], &[]).json();
    let arm_wait_ms = timing["data"]["value"].as_f64().expect("missing arm time");
    assert!(arm_wait_ms < 30.0, "hidden tab arm wait: {arm_wait_ms} ms");
    stop(&tabref);
}
