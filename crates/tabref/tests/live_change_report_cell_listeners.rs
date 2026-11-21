#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::{harness, live, serve};

fn newly_interactive(name: &str, markup: &str, setup: &str) {
    if !live() {
        return;
    }
    let html = format!(
        r#"<!doctype html><button onclick="document.getElementById('widgets').hidden=false">Reveal</button>
<section id=widgets hidden>{markup}
<table><tr><th>Passive heading</th></tr><tr><td>Passive cell</td></tr><tr><td><button>Open shipment</button></td></tr></table></section>
<script>const target=document.getElementById('target');{setup}</script>"#
    );
    let port = serve(&[("/", &html)]);
    let tabref = harness(name);
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Reveal"]).stdout;
    let new: Vec<_> = text
        .lines()
        .filter(|line| line.starts_with("new "))
        .collect();
    assert!(new.iter().any(|line| line.contains(name)), "{text}");
    assert!(
        new.iter()
            .any(|line| line.contains("button \"Open shipment\"")),
        "{text}"
    );
    assert!(
        !new.iter().any(|line| line.contains("Passive")
            || line.contains("Inactive cell")
            || line.contains("cell \"Open shipment\"")),
        "{text}"
    );
}

#[test]
fn event_listener_cells_are_new_controls() {
    newly_interactive(
        "Listener cell",
        "<table><tr><td id=target>Listener cell</td></tr></table>",
        "target.addEventListener('click',()=>target.textContent='Clicked')",
    );
}

#[test]
fn event_listener_rows_are_new_controls() {
    newly_interactive(
        "Listener row",
        "<div role=grid><div role=row id=target>Listener row</div></div>",
        "target.addEventListener('click',()=>target.textContent='Clicked')",
    );
}

#[test]
fn onclick_property_cells_are_new_controls() {
    newly_interactive(
        "Property cell",
        "<table><tr><td id=target>Property cell</td></tr></table>",
        "target.onclick=()=>target.textContent='Clicked'",
    );
}

#[test]
fn event_listener_sort_headers_are_new_controls() {
    newly_interactive(
        "Sort header",
        "<table><tr><th id=target aria-sort=none>Sort header</th></tr></table>",
        "target.addEventListener('click',()=>target.setAttribute('aria-sort','ascending'))",
    );
}

#[test]
fn active_descendant_cells_are_new_controls() {
    newly_interactive(
        "Active cell",
        "<div role=grid tabindex=0 aria-activedescendant=target><div role=row><div role=gridcell id=target>Active cell</div><div role=gridcell>Inactive cell</div></div></div>",
        "",
    );
}
