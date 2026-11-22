#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

#[path = "../../tabref-page/src/page/capture_watch_css.rs"]
mod classifier;

use serde_json::json;
use tabref_cdp::{attach_to_target, connect, list_page_targets};
use tabref_core::Deadline;

fn sheets() -> Vec<String> {
    let values = [
        "red",
        "url(x)",
        "url(x/*)",
        "url(x\"y)",
        "url(x(y)",
        "url(x y)",
        "url(x\\\ny)",
        "url(  'a')",
        "url(a\\29 b)",
        "u\\72l(x/*)",
        "\"a\"",
        "\"a\u{000c}b\"",
        "\"a\r\nb\"",
        "\"a\\\r\nb\"",
        "'a\\27 b'",
        "'a\\00000c b'",
        "'a\\\u{000c}b'",
        "[a(b)]",
        "[a)}",
        "var(--x, [red])",
        "/*x*/red",
        "url(/**/)",
        "url(\\)abc)",
        "url(\"a)\")",
        "url(x\u{000b})",
        "url(x\\)",
        "url(x\\\r\ny)",
        "url(x\t)",
        "url( x/**/)",
        "'unterminated",
    ];
    let wrappers = ["plain", "media", "supports", "layer", "nested", "container"];
    let mut seed = 0xd1ff_e2e1_u64;
    let mut result = vec![
        ".a{b:url(x\"y)}@container (min-width:1px){.x{color:red}}.c{d:\")}".into(),
        ".a{b:url(x/*)}@container (min-width:1px){.x{color:red}}.c{d:e*/)}".into(),
        ".a{--v:\"\u{000C}}@container (min-width:1px){.x{color:red}}.c{--w:\"}".into(),
    ];
    for i in 0..600 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let value = values[(seed >> 32) as usize % values.len()];
        let base = format!(".a{{--v:{value};color:red}}");
        let mut sheet = match wrappers[i % wrappers.len()] {
            "media" => format!("@media screen and (min-width:1px){{{base}}}"),
            "supports" => format!("@supports(display:grid){{{base}}}"),
            "layer" => format!("@layer example{{{base}}}"),
            "nested" => format!("@layer example{{@media screen{{{base}}}}}"),
            "container" => format!("@container(width>1px){{{base}}}"),
            _ => base,
        };
        match i % 9 {
            0 => sheet.push_str("@container(width>1px){.x{color:red}}"),
            1 => sheet.push_str(".z{color:blue}"),
            2 => sheet.insert_str(0, "<!--"),
            3 => sheet.push_str("-->"),
            4 => sheet.insert_str(0, "/* { @container */"),
            5 => sheet.push_str("@\\63 ontainer(width>1px){.x{color:red}}"),
            6 => sheet.push_str(".x:ha\\73(b){color:red}"),
            _ => {}
        }
        result.push(sheet);
    }
    result
}

#[test]
fn approved_rule_structure_matches_chrome_cssom() {
    if !common::live() {
        return;
    }
    let sheets = sheets();
    let harness = common::harness("css-differential");
    let deadline = Deadline::after(60_000).unwrap();
    let mut connection = connect(&harness.browser.endpoint().unwrap(), deadline).unwrap();
    let target = list_page_targets(&mut connection, deadline)
        .unwrap()
        .remove(0);
    let page = attach_to_target(&mut connection, &target.target_id, deadline).unwrap();
    let port = common::serve(&[("/", "<!doctype html><title>Differential</title>")]);
    connection
        .call(
            "Page.navigate",
            json!({"url":format!("http://127.0.0.1:{port}/")}),
            Some(&page.session_id),
            deadline,
        )
        .unwrap();
    let expression = format!(
        r"(async () => {{
        if (document.readyState !== 'complete') await new Promise(r => addEventListener('load', r, {{once:true}}));
        return {}.map(text => {{
            const el = document.createElement('style'); el.textContent = text; document.head.append(el);
            const names = [...el.sheet.cssRules].map(rule => rule.type === CSSRule.STYLE_RULE ? 'style' : rule.cssText.match(/^@([\w-]+)/)[1].toLowerCase());
            el.remove(); return names;
        }});
    }})()",
        json!(sheets)
    );
    let reply = connection
        .call(
            "Runtime.evaluate",
            json!({"expression":expression,"returnByValue":true,"awaitPromise":true}),
            Some(&page.session_id),
            deadline,
        )
        .unwrap();
    assert!(reply.get("exceptionDetails").is_none(), "{reply}");
    let chrome = reply["result"]["value"].as_array().unwrap();
    let mut approved = 0;
    let mut rejected = 0;
    let mut disagreements = Vec::new();
    for (text, names) in sheets.iter().zip(chrome) {
        if let Some(parsed) = classifier::structure::structure(text) {
            approved += 1;
            if json!(parsed) != *names {
                disagreements.push(format!("{text:?}: classifier={parsed:?}, Chrome={names}"));
            }
        } else {
            rejected += 1;
        }
    }
    eprintln!(
        "generated={} approved={approved} rejected={rejected} disagreements={}",
        sheets.len(),
        disagreements.len()
    );
    assert!(approved > 20);
    assert!(disagreements.is_empty(), "{}", disagreements.join("\n"));
}
