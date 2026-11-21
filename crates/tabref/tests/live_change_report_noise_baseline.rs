//! Closed popups are summarized without repeating their options.
//! TABREF_LIVE=1 cargo test -p tabref --test live_change_report_noise_baseline -- --nocapture
#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, serve};

#[test]
fn observe_change_report_noise() {
    if !live() {
        return;
    }
    let port = serve(&[
        (
            "/custom",
            include_str!("fixtures/change_report_custom_combo.html"),
        ),
        (
            "/native",
            include_str!("fixtures/change_report_native_frame.html"),
        ),
        (
            "/native-fields",
            include_str!("fixtures/change_report_native_fields.html"),
        ),
        (
            "/owned",
            include_str!("fixtures/change_report_owned_combo.html"),
        ),
    ]);
    let tabref = harness("change-report-noise-baseline");
    let base = format!("http://127.0.0.1:{port}");
    tabref.ok(&["open", &format!("{base}/custom")]);
    for (label, args) in [
        ("CUSTOM OPEN", vec!["click", "combobox \"Category\""]),
        (
            "CUSTOM CLOSE",
            vec!["press", "Escape", "combobox \"Category\""],
        ),
    ] {
        let report = tabref.ok(&args).stdout;
        println!(
            "=== {label}: {} bytes ===\n{report}=== END ===",
            report.len()
        );
        if label == "CUSTOM CLOSE" {
            assert_eq!(
                report,
                "pressed Escape in \"Category\"\n- listbox \"Category\" closed (18 options)\n"
            );
        }
    }
    tabref.ok(&["open", &format!("{base}/native")]);
    let report = tabref.ok(&["click", "Reload editor"]).stdout;
    println!(
        "=== NATIVE RELOAD: {} bytes ===\n{report}=== END ===",
        report.len()
    );
    tabref.ok(&["open", &format!("{base}/owned")]);
    let report = tabref.ok(&["click", "Search categories"]).stdout;
    println!(
        "=== ARIA OWNS OPEN: {} bytes ===\n{report}=== END ===",
        report.len()
    );
    let _ = tabref.run(&["daemon", "stop"]);
}

#[test]
fn popup_closes_in_shadow_roots_and_frames_keep_other_changes() {
    if !live() {
        return;
    }
    let inner = r#"<!doctype html><p>Shared</p>
<ul role=menu aria-label=Actions><li role=menuitem>Shared</li><li role=menuitem>Delete</li></ul>
<p id=error></p><p id=value>Value old</p><p id=gone>Unrelated removal</p>
<button onclick="document.querySelector('ul').hidden=true;document.getElementById('error').textContent='Validation failed';document.getElementById('value').textContent='Value new';document.getElementById('gone').remove()">Close menu</button>"#;
    let shadow = r#"<!doctype html><x-popup></x-popup><script>
customElements.define('x-popup', class extends HTMLElement { connectedCallback() {
this.attachShadow({mode:'open'}).innerHTML = '<p>Shared</p><ul role=listbox aria-label=Category><li role=option>Shared</li><li role=option>Arts</li></ul><button>Close popup</button>';
this.shadowRoot.querySelector('button').onclick=()=>this.shadowRoot.querySelector('ul').remove();
}});</script>"#;
    let port = serve(&[
        ("/inner", inner),
        ("/shadow", shadow),
        (
            "/frame",
            "<!doctype html><iframe src=/inner style='width:700px;height:400px'></iframe>",
        ),
    ]);
    let tabref = harness("popup-close-roots");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/shadow")]);
    let report = tabref.ok(&["click", "Close popup"]).stdout;
    assert!(
        report.contains("- listbox \"Category\" closed (2 options)"),
        "{report}"
    );
    assert!(!report.contains("- Shared"), "{report}");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/frame")]);
    let report = tabref.ok(&["click", "Close menu"]).stdout;
    assert!(
        report.contains("- menu \"Actions\" closed (2 items)"),
        "{report}"
    );
    assert!(report.contains("Validation failed"), "{report}");
    assert!(report.contains("new"), "{report}");
    assert!(report.contains("Unrelated removal"), "{report}");
    assert!(!report.contains("- Shared"), "{report}");
    let _ = tabref.run(&["daemon", "stop"]);
}
