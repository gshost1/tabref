#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::{harness, live, serve};

#[test]
fn filtering_tables_reports_text_and_real_controls_not_passive_cells() {
    if !live() {
        return;
    }
    let port = serve(&[(
        "/",
        include_str!("fixtures/change_report_filtered_cells.html"),
    )]);
    let tabref = harness("filtered-cells");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let selected = tabref.ok(&["select", "Depot", "Recent"]).stdout;
    let filled = tabref.ok(&["fill", "Owner", "Ada"]).stdout;
    println!(
        "=== FILTER SELECT: {} bytes ===\n{selected}=== END ===",
        selected.len()
    );
    println!(
        "=== FILTER FILL: {} bytes ===\n{filled}=== END ===",
        filled.len()
    );
    for text in [&selected, &filled] {
        assert!(
            !text
                .lines()
                .any(|line| line.starts_with("new ") && line.contains(" cell ")),
            "{text}"
        );
        assert!(text.contains("TRK-"), "{text}");
        assert!(!text.contains("more new:"), "{text}");
    }
    assert!(selected.contains("\"All\" -> \"Recent\""), "{selected}");
    assert!(selected.contains("Owner required"), "{selected}");
    assert!(selected.contains("Balance {10 -> 20}"), "{selected}");
    assert!(selected.contains("button \"Retry shipment\""), "{selected}");
    assert!(!selected.contains("[disabled]"), "{selected}");
    assert!(filled.contains("Balance {20 -> 30}"), "{filled}");
    let data = tabref.ok(&["--json", "select", "Depot", "All"]).json();
    assert!(
        data["data"]["new"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| !entry.as_str().unwrap().contains(" cell ")),
        "{data}"
    );
}

#[test]
fn interactive_grid_cells_sort_headers_and_inner_buttons_are_kept() {
    if !live() {
        return;
    }
    let port = serve(&[(
        "/",
        r#"<!doctype html><button onclick="document.getElementById('widgets').hidden=false">Reveal grid</button>
<section id=widgets hidden><div role=grid><div role=row>
<div role=gridcell>Passive gridcell</div>
<div role=gridcell tabindex=0>Focusable gridcell</div>
<div role=gridcell tabindex=-1>Roving gridcell</div>
<div role=gridcell contenteditable=true>Editable gridcell</div>
<div role=gridcell><button>Open shipment</button></div>
</div></div><table><tr><th>Static heading</th><th aria-sort=none onclick="this.setAttribute('aria-sort','ascending')">Sortable heading</th></tr>
<tr><td onclick="this.textContent='Clicked'">Clickable cell</td></tr></table></section>"#,
    )]);
    let tabref = harness("interactive-cells");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Reveal grid"]).stdout;
    let new: Vec<_> = text
        .lines()
        .filter(|line| line.starts_with("new "))
        .collect();
    for name in [
        "Focusable gridcell",
        "Roving gridcell",
        "Editable gridcell",
        "Sortable heading",
        "Clickable cell",
        "Open shipment",
    ] {
        assert!(new.iter().any(|line| line.contains(name)), "{text}");
    }
    assert!(
        !new.iter()
            .any(|line| line.contains("Passive gridcell") || line.contains("Static heading")),
        "{text}"
    );
}

#[test]
fn redrawing_passive_rows_keeps_the_existing_reorder_preview() {
    if !live() {
        return;
    }
    let port = serve(&[(
        "/",
        include_str!("fixtures/change_report_filtered_cells.html"),
    )]);
    let tabref = harness("reordered-passive-cells");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["select", "Depot", "Reverse"]).stdout;
    println!(
        "=== FILTER REORDER: {} bytes ===\n{text}=== END ===",
        text.len()
    );
    assert!(
        text.contains("reordered 120 lines below \"Tracking\", now starting:\n  TRK-162119"),
        "{text}"
    );
    assert!(!text.contains("drawn again:"), "{text}");
    assert!(text.contains("button \"Retry shipment\""), "{text}");
}
