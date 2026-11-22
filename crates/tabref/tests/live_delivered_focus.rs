//! A focus is delivered once `DOM.focus` returned: a page that then holds
//! up the watch on the focus never makes the focus look undelivered or safe
//! to retry.
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::focus_page::own_browser;
use common::{live, serve};

const BUSY: &str = r#"<!doctype html><title>Start</title>
<input aria-label="Busy" onfocus="setTimeout(() => { const t = Date.now(); while (Date.now() - t < 4000) {} }, 0)">"#;

#[test]
fn a_page_busy_right_after_a_focus_keeps_the_focus_delivered() {
    if !live() {
        return;
    }
    let port = serve(&[("/", BUSY)]);
    let tabref = own_browser("delivered-focus-busy");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let envelope = tabref
        .run(&["--json", "--timeout", "2000", "focus", "Busy"])
        .json();
    assert_eq!(envelope["ok"], false, "{envelope}");
    let error = &envelope["error"];
    assert_eq!(
        error["disposition"]["delivery"], "delivered_unverified",
        "{envelope}"
    );
    assert_eq!(error["disposition"]["retry"], "unsafe", "{envelope}");
    let said = error["details"]["report_error"]
        .as_str()
        .unwrap_or_default();
    assert!(
        said.starts_with("could not read what the action set off ("),
        "{envelope}"
    );
}
