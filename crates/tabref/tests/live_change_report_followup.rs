#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::{harness, live, serve};

#[test]
fn checkbox_first_columns_preserve_revocation_edits() {
    if !live() {
        return;
    }
    let html = r#"<!doctype html><button onclick="draw(true)">Refresh</button><table><tbody></tbody></table><script>function draw(fresh){document.querySelector('tbody').innerHTML=Array.from({length:10},(_,i)=>'<tr><td><input type=checkbox></td><td>TOK-'+(100+i)+'</td><td>'+(fresh&&i===7?'revoked':'active')+'</td><td>checked '+(fresh?'1s':'0s')+' ago</td></tr>').join('')}draw(false)</script>"#;
    let port = serve(&[("/", html)]);
    let tabref = harness("checkbox-key");
    for args in [
        &["click", "Refresh"][..],
        &["--json", "click", "Refresh"][..],
    ] {
        tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
        let text = tabref.ok(args).stdout;
        println!("CHECKBOX REFRESH: {text}");
        assert!(!text.contains("rows replaced"), "{text}");
        assert!(
            text.contains("TOK-107") && text.contains("revoked") && text.contains("active"),
            "{text}"
        );
        if args[0] == "--json" {
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(value["data"]["rows_replaced"].is_null(), "{value}");
            assert!(
                value["data"]["changes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|line| {
                        let line = line.as_str().unwrap();
                        line.contains("TOK-107") && line.contains("revoked") && line.contains("->")
                    }),
                "{value}"
            );
        }
    }
}

fn address_dialog(long: bool) -> String {
    let html = r#"<!doctype html><p id=toast></p><dialog open aria-label=Address><h2>Address</h2><label>Country<select><optgroup label=Countries></optgroup></select></label><button onclick="document.querySelector('#toast').textContent='Saved';this.closest('dialog').remove()">Save</button></dialog><script>document.querySelector('optgroup').innerHTML=Array.from({length:600},(_,i)=>'<option>C'+i+'SUFFIX</option>').join('');window.dialogReads=0;const desc=Object.getOwnPropertyDescriptor(HTMLElement.prototype,'innerText');Object.defineProperty(HTMLElement.prototype,'innerText',{get(){if(this.matches('dialog'))window.dialogReads++;return desc.get.call(this)},set:desc.set,configurable:true})</script>"#;
    html.replace("SUFFIX", if long { "xxxxxxxxxxxxxxxxxxxx" } else { "" })
        .replace(
            "this.closest('dialog').remove()",
            if long {
                "window.saved=true"
            } else {
                "this.closest('dialog').remove()"
            },
        )
}

#[test]
fn closing_a_named_dialog_with_600_native_options_is_one_summary() {
    if !live() {
        return;
    }
    let html = address_dialog(false);
    let port = serve(&[("/", &html)]);
    let tabref = harness("dialog-native-options");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let metrics=tabref.ok(&["--json","js","({dialog:document.querySelector('dialog').innerText.length,select:document.querySelector('select').innerText.length})"]).json();
    println!("NATIVE SELECT TEXT: {metrics}");
    assert!(
        metrics["data"]["value"]["select"].as_u64().unwrap() > 2000,
        "{metrics}"
    );
    assert!(
        metrics["data"]["value"]["dialog"].as_u64().unwrap() < 4096,
        "{metrics}"
    );
    let text = tabref.ok(&["click", "Save"]).stdout;
    println!("NATIVE SELECT CLOSE: {text}");
    assert!(text.contains("- dialog \"Address\" closed"), "{text}");
    assert!(text.contains("+ Saved"), "{text}");
    assert!(!text.contains("- C0"), "{text}");
    assert!(!text.contains("more: tabref snap"), "{text}");
}

#[test]
fn long_native_options_do_not_spend_the_dialog_character_budget() {
    if !live() {
        return;
    }
    let html = address_dialog(false).replace("<option>C", "<option>Country number ");
    let port = serve(&[("/", &html)]);
    let tabref = harness("dialog-long-native-options");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Save"]).stdout;
    println!("LONG SELECT CLOSE: {text}");
    assert!(text.contains("- dialog \"Address\" closed"), "{text}");
    assert!(text.contains("+ Saved"), "{text}");
    assert!(!text.contains("- Country number"), "{text}");
    assert!(!text.contains("more: tabref snap"), "{text}");
}

#[test]
fn changing_first_column_preserves_later_key_edits() {
    if !live() {
        return;
    }
    let html = r#"<!doctype html><button onclick="draw(true)">Refresh</button><table><tbody></tbody></table><script>function draw(fresh){document.querySelector('tbody').innerHTML=Array.from({length:10},(_,i)=>'<tr><td>'+(fresh?'1s':'0s')+' ago</td><td>TOK-'+(100+i)+'</td><td>'+(fresh&&i===7?'revoked':'active')+'</td></tr>').join('')}draw(false)</script>"#;
    let port = serve(&[("/", html)]);
    let tabref = harness("later-column-key");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Refresh"]).stdout;
    println!("LATER KEY REFRESH: {text}");
    assert!(!text.contains("rows replaced"), "{text}");
    assert!(
        text.contains("TOK-107") && text.contains("revoked"),
        "{text}"
    );
}

#[test]
fn dialog_text_and_current_selection_still_obey_the_character_budget() {
    if !live() {
        return;
    }
    let html = address_dialog(true).replace(
        "<h2>Address</h2>",
        &format!("<h2>Address</h2><p id=oversized>{}</p>", "x".repeat(5000)),
    );
    let port = serve(&[("/", &html)]);
    let tabref = harness("dialog-option-budget");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Save"]).stdout;
    assert!(text.contains("+ Saved"), "{text}");
    let value = tabref.ok(&["--json", "js", "window.dialogReads"]).json();
    assert_eq!(value["data"]["value"], 0, "{value}");
    tabref.ok(&["js", "document.querySelector('#oversized').remove();document.querySelector('select').replaceChildren(...Array.from({length:10000},()=>document.createElement('option')));document.querySelector('#toast').textContent='';window.dialogReads=0"]);
    tabref.ok(&["click", "Save"]);
    let value = tabref.ok(&["--json", "js", "window.dialogReads"]).json();
    assert_eq!(value["data"]["value"], 0, "{value}");
    tabref.ok(&["js", "document.querySelector('select').innerHTML='<option>'+ 'x'.repeat(5000)+'</option>';window.dialogReads=0"]);
    tabref.ok(&["click", "Save"]);
    let value = tabref.ok(&["--json", "js", "window.dialogReads"]).json();
    assert_eq!(value["data"]["value"], 0, "{value}");
}

#[test]
fn intermediate_do_replacements_use_full_preview_and_structured_json() {
    if !live() {
        return;
    }
    let html = r#"<!doctype html><p id=toast></p><button onclick="draw(true);saved()">Go</button><button onclick="document.querySelector('tbody').replaceChildren();saved()">Clear</button><button>Done</button><table><tbody></tbody></table><script>function saved(){document.querySelector('#toast').textContent='Saved'}function draw(fresh){document.querySelector('tbody').innerHTML=Array.from({length:12},(_,i)=>'<tr><td>'+(fresh?'New-':'Old-')+i+'</td></tr>').join('')}draw(false)</script>"#;
    let port = serve(&[("/", html)]);
    let tabref = harness("intermediate-replacement");
    let url = format!("http://127.0.0.1:{port}/");
    for (button, count) in [("Go", 12), ("Clear", 0)] {
        let batch = format!("click {button}; click Done");
        tabref.ok(&["open", &url]);
        let text = tabref.ok(&["do", &batch]).stdout;
        tabref.ok(&["open", &url]);
        let value = tabref.ok(&["--json", "do", &batch]).json();
        println!("INTERMEDIATE TEXT: {text}\nINTERMEDIATE JSON: {value}");
        assert!(!text.contains("(+12 more)"), "{text}");
        assert_eq!(
            text.matches(&format!("- 12 rows replaced, {count} new"))
                .count(),
            1,
            "{text}"
        );
        let step = &value["data"]["steps"][0]["data"];
        assert_eq!(step["rows_replaced"]["new"], count, "{value}");
        assert_eq!(step["rows_replaced"]["omitted"], 0, "{value}");
        assert_eq!(step["changes"], serde_json::json!(["+ Saved"]), "{value}");
        assert!(text.contains("+ Saved"), "{text}");
        if count > 0 {
            assert!(
                text.contains("\n  New-0") && text.contains("(+7 more: tabref snap)"),
                "{text}"
            );
        }
        tabref.ok(&["open", &url]);
        let standalone = tabref.ok(&["--json", "click", button]).json();
        assert_eq!(
            step["rows_replaced"], standalone["data"]["rows_replaced"],
            "{value}\n{standalone}"
        );
        tabref.ok(&["open", &url]);
        let failed = tabref.run(&[
            "--json",
            "--timeout",
            "1000",
            "do",
            &format!("click {button}; click Missing"),
        ]);
        assert!(!failed.ok);
        let error = failed.json();
        assert_eq!(error["error"]["details"]["failed_step"], 2, "{error}");
        assert_eq!(
            error["error"]["details"]["steps_done"][0]["data"]["rows_replaced"],
            step["rows_replaced"],
            "{error}"
        );
    }
}
