#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::{harness, live, serve};
use std::time::Instant;

#[test]
fn refresh_preserves_revocation_edits() {
    if !live() {
        return;
    }
    let html = r#"<!doctype html><button onclick="draw(true)">Refresh</button><table><tbody></tbody></table><script>function draw(fresh){document.querySelector('tbody').innerHTML=Array.from({length:10},(_,i)=>'<tr id="row-'+i+'"><td>TOK-'+(100+i)+'</td><td>'+(fresh&&i===7?'revoked':'active')+'</td><td>checked '+(fresh?'1s':'0s')+' ago</td></tr>').join('')}draw(false)</script>"#;
    let port = serve(&[("/", html)]);
    let tabref = harness("review-status");
    for json in [false, true] {
        tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
        let args = if json {
            &["--json", "click", "Refresh"][..]
        } else {
            &["click", "Refresh"][..]
        };
        let text = tabref.ok(args).stdout;
        assert!(!text.contains("rows replaced"), "{text}");
        assert!(
            text.contains("revoked") && text.contains("active") && text.contains("TOK-107"),
            "{text}"
        );
    }
}

#[test]
fn moved_unnamed_dialog_keeps_validation() {
    if !live() {
        return;
    }
    let html = r#"<!doctype html><div role=dialog><label>Name<input></label><button onclick="const old=this.parentElement;const box=document.createElement('section');box.append(old.cloneNode(true));old.remove();document.body.append(box);box.firstChild.insertAdjacentHTML('beforeend','<p>Name is taken</p>')">Save</button></div>"#;
    let port = serve(&[("/", html)]);
    let tabref = harness("review-moved-dialog");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Save"]).stdout;
    assert!(!text.contains("closed"), "{text}");
    assert!(text.contains("+ Name is taken"), "{text}");
}

#[test]
fn json_replacements_are_structured_and_do_not_confuse_toasts_with_rows() {
    if !live() {
        return;
    }
    let html = r#"<!doctype html><button onclick="draw(true);document.querySelector('p').textContent='Saved'">Replace</button><p></p><table><tbody></tbody></table><script>function draw(fresh){document.querySelector('tbody').innerHTML=Array.from({length:60},(_,i)=>'<tr><td>'+(fresh?(i===59?'Saved':'New-'+i):'Old-'+i)+'</td></tr>').join('')}draw(false)</script>"#;
    let port = serve(&[("/", html)]);
    let tabref = harness("review-json-rows");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let value = tabref.ok(&["--json", "click", "Replace"]).json();
    let data = &value["data"];
    let rows = &data["rows_replaced"];
    assert_eq!(rows["old"], 60, "{value}");
    assert_eq!(rows["new"], 60, "{value}");
    assert_eq!(
        rows["preview"].as_array().unwrap().len() as u64 + rows["omitted"].as_u64().unwrap(),
        60
    );
    assert!(
        rows["preview"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["id"].is_u64()),
        "{value}"
    );
    assert_eq!(rows["omitted"], 10, "{value}");
    assert!(data.get("new_rows_omitted").is_none(), "{value}");
    assert!(
        data["changes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| !c.as_str().unwrap().contains("rows replaced")),
        "{value}"
    );
    assert!(
        data["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "+ Saved"),
        "{value}"
    );
}

#[test]
fn large_watch_reads_are_bounded_and_measured() {
    if !live() {
        return;
    }
    let html = r#"<!doctype html><button onclick="document.querySelector('p').textContent='Tick '+(++tick)">Tick</button><p>Tick 0</p><table><tbody></tbody></table><div role=dialog></div><script>let tick=0;document.querySelector('tbody').innerHTML=Array.from({length:10000},(_,i)=>'<tr><td>TOK-'+i+'</td></tr>').join('');document.querySelector('[role=dialog]').textContent='x'.repeat(20000);window.rowReads=0;window.dialogReads=0;const d=Object.getOwnPropertyDescriptor(HTMLElement.prototype,'innerText');Object.defineProperty(HTMLElement.prototype,'innerText',{get(){if(this.matches('tr'))window.rowReads++;if(this.matches('[role=dialog]'))window.dialogReads++;return d.get.call(this)},set:d.set,configurable:true})</script>"#;
    let port = serve(&[("/", html)]);
    let tabref = harness("review-large-watch");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let mut times = Vec::new();
    for i in 0..5 {
        let start = Instant::now();
        let text = tabref.ok(&["click", "Tick"]).stdout;
        times.push(start.elapsed().as_millis());
        assert!(
            text.contains(&format!("Tick {{{i} -> {}}}", i + 1)),
            "{text}"
        );
    }
    times.sort_unstable();
    println!("10K ACTION ms sorted={times:?}, median={}", times[2]);
    let value = tabref
        .ok(&[
            "--json",
            "js",
            "({rows:window.rowReads,dialogs:window.dialogReads})",
        ])
        .json();
    println!("10K READ COUNTS: {value}");
    assert_eq!(value["data"]["value"]["rows"], 0, "{value}");
    assert_eq!(value["data"]["value"]["dialogs"], 0, "{value}");
}

#[test]
fn oversized_row_and_large_settings_pane_skip_metadata_reads() {
    if !live() {
        return;
    }
    let html = r#"<!doctype html><button onclick="document.querySelector('p').textContent='Row invalid'">Check</button><p role=alert></p><table><tbody><tr><td></td></tr></tbody></table><div role=dialog></div><script>document.querySelector('td').textContent='x'.repeat(20000);document.querySelector('[role=dialog]').innerHTML='<div></div>'.repeat(2000);window.rowReads=0;window.dialogReads=0;const d=Object.getOwnPropertyDescriptor(HTMLElement.prototype,'innerText');Object.defineProperty(HTMLElement.prototype,'innerText',{get(){if(this.matches('tr'))window.rowReads++;if(this.matches('[role=dialog]'))window.dialogReads++;return d.get.call(this)},set:d.set,configurable:true})</script>"#;
    let port = serve(&[("/", html)]);
    let tabref = harness("review-oversized-row");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Check"]).stdout;
    assert!(text.contains("+ Row invalid"), "{text}");
    assert!(!text.contains("rows replaced"), "{text}");
    let value = tabref
        .ok(&[
            "--json",
            "js",
            "({rows:window.rowReads,dialogs:window.dialogReads})",
        ])
        .json();
    assert_eq!(value["data"]["value"]["rows"], 0, "{value}");
    assert_eq!(value["data"]["value"]["dialogs"], 0, "{value}");
}
