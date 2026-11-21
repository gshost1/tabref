#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::{harness, live, serve};

#[test]
fn replacements_show_new_rows_instead_of_old_removals_in_text_and_json() {
    if !live() {
        return;
    }
    let port = serve(&[(
        "/",
        include_str!("fixtures/change_report_filtered_cells.html"),
    )]);
    let tabref = harness("rows-replaced");
    let url = format!("http://127.0.0.1:{port}/");
    tabref.ok(&["open", &url]);
    for (args, total, start) in [
        (&["select", "Depot", "Recent"][..], 120, 162120),
        (&["fill", "Owner", "Ada"][..], 54, 162240),
    ] {
        let text = tabref.ok(args).stdout;
        println!(
            "=== REPLACEMENT {}: {} bytes ===\n{text}=== END ===",
            args[0],
            text.len()
        );
        assert!(
            text.contains(&format!("- 120 rows replaced, {total} new")),
            "{text}"
        );
        for offset in 0..5 {
            assert!(
                text.contains(&format!("\n  TRK-{}", start + offset)),
                "{text}"
            );
        }
        assert!(!text.contains(&format!("\n  TRK-{}", start + 5)), "{text}");
        assert!(!text.contains("- TRK-"), "{text}");
        assert!(text.contains("Balance"), "{text}");
        if args[0] == "select" {
            assert!(text.contains("Owner required"), "{text}");
            assert!(text.contains("button \"Retry shipment\""), "{text}");
        }
    }
    tabref.ok(&["open", &url]);
    for (args, total, start) in [
        (&["--json", "select", "Depot", "Recent"][..], 120, 162120),
        (&["--json", "fill", "Owner", "Ada"][..], 54, 162240),
    ] {
        let value = tabref.ok(args).json();
        let data = &value["data"];
        let lines = data["changes"].as_array().unwrap();
        let rows = &data["rows_replaced"];
        let shown = rows["preview"].as_array().unwrap().len();
        assert_eq!(
            rows["preview"][0]["text"],
            format!("TRK-{start}"),
            "{value}"
        );
        assert!(rows["preview"][0]["id"].is_u64(), "{value}");
        assert_eq!(rows["new"], total, "{value}");
        println!(
            "=== JSON {}: {shown} row identities shown, {} omitted ===",
            args[1], rows["omitted"]
        );
        assert_eq!(
            shown + rows["omitted"].as_u64().unwrap() as usize,
            total,
            "{value}"
        );
        assert!(data.get("new_rows_omitted").is_none(), "{value}");
        assert!(
            !lines
                .iter()
                .any(|line| line.as_str().unwrap().contains("rows replaced")),
            "{value}"
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.as_str().unwrap().starts_with("- TRK-")),
            "{value}"
        );
    }
}

#[test]
fn partial_redraws_keep_the_existing_redrawn_rows_report() {
    if !live() {
        return;
    }
    let html = r#"<!doctype html><button onclick="const rows=[...document.querySelectorAll('tbody tr')];document.querySelector('tbody').replaceChildren(...rows.map((row,i)=>i<10?row:row.cloneNode(true)).reverse())">Redraw</button><table><thead><tr><th>Code</th><th>Action</th></tr></thead><tbody></tbody></table><script>document.querySelector('tbody').innerHTML=Array.from({length:50},(_,i)=>'<tr><td>R'+String(i).padStart(2,'0')+'</td><td><button>Open '+i+'</button></td></tr>').join('')</script>"#;
    let port = serve(&[("/", html)]);
    let tabref = harness("partial-redraw");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Redraw"]).stdout;
    assert!(text.contains("reordered 50 rows"), "{text}");
    assert!(text.contains("rows drawn again: 40 new refs"), "{text}");
    assert!(!text.contains("rows replaced"), "{text}");
}

#[test]
fn replacement_keeps_toasts_errors_values_and_enabled_buttons() {
    if !live() {
        return;
    }
    let html = include_str!("fixtures/change_report_filtered_cells.html")
        .replace(
            "162000+offset+n)+'</td>",
            "162000+offset+n)+(n===8?'<p role=alert>Row invalid</p>':'')+'</td>",
        )
        .replace("<table>", "<p id=toast></p><table>")
        .replace(
            "const rows=",
            "document.getElementById('toast').textContent='Saved';const rows=",
        );
    let port = serve(&[("/", &html)]);
    let tabref = harness("replacement-extras");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["select", "Depot", "Recent"]).stdout;
    for expected in [
        "rows replaced",
        "+ Saved",
        "+ Row invalid",
        "Owner required",
        "Balance {10 -> 20}",
        "button \"Retry shipment\"",
    ] {
        assert!(text.contains(expected), "{text}");
    }
}
