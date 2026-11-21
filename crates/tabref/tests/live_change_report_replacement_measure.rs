#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::{harness, live, serve};

#[test]
fn measure_replacement_reports_against_the_new_rows_in_snap() {
    if !live() {
        return;
    }
    let port = serve(&[(
        "/",
        include_str!("fixtures/change_report_filtered_cells.html"),
    )]);
    let tabref = harness("replacement-measure");
    let url = format!("http://127.0.0.1:{port}/");
    tabref.ok(&["open", &url]);
    let selected = tabref.ok(&["select", "Depot", "Recent"]).stdout;
    let select_snap = tabref.ok(&["snap"]).stdout;
    let filled = tabref.ok(&["fill", "Owner", "Ada"]).stdout;
    let fill_snap = tabref.ok(&["snap"]).stdout;
    for (name, text) in [
        ("SELECT", &selected),
        ("SELECT SNAP", &select_snap),
        ("FILL", &filled),
        ("FILL SNAP", &fill_snap),
    ] {
        println!("=== {name}: {} bytes ===\n{text}=== END ===", text.len());
    }
    assert!(select_snap.contains("TRK-162120"), "{select_snap}");
    assert!(fill_snap.contains("TRK-162240"), "{fill_snap}");
    tabref.ok(&["open", &url]);
    for args in [
        &["--json", "select", "Depot", "Recent"][..],
        &["--json", "fill", "Owner", "Ada"][..],
    ] {
        let json = tabref.ok(args).json();
        let data = &json["data"];
        println!(
            "JSON {}: {} shown changes, {} omitted, reordered={}",
            json["command"],
            data["changes"].as_array().unwrap().len(),
            data["distinct_changes_omitted"],
            data["reordered"]
        );
    }
}
