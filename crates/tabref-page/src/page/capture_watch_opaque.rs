use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::{CdpConnection, PageSession, evaluate_quietly};

type Graph = Vec<(String, Option<Vec<String>>)>;

const INVENTORY: &str = r"(() => {
  const key = Symbol.for('tabref.capture-opaque');
  const entries = [];
  let elements = 0, rules = 0, overflow = false;
  const seen = new WeakSet();
  const visit = root => {
    const nodes = root.querySelectorAll('*');
    elements += nodes.length;
    if (elements > 30000) { overflow = true; return; }
    for (const sheet of [...(root.styleSheets ?? []), ...(root.adoptedStyleSheets ?? [])]) {
      const walk = sheet => {
        if (seen.has(sheet)) return;
        seen.add(sheet);
        try { for (const rule of sheet.cssRules) {
          if (++rules > 10000) { overflow = true; return; }
          if (rule.styleSheet) walk(rule.styleSheet);
        } }
        catch { entries.push({sheet, href: sheet.href}); }
      };
      walk(sheet);
      if (overflow) return;
    }
    for (const el of nodes) { if (el.shadowRoot) visit(el.shadowRoot); if (overflow) return; }
  };
  visit(document);
  window[key] = {token: TOKEN, entries};
  return overflow ? [] : entries.map(entry => entry.href);
})()";

pub(super) fn prepare(
    connection: &mut CdpConnection,
    page: &PageSession,
    token: &str,
    deadline: Deadline,
) -> Result<(), AppError> {
    let inventory = INVENTORY.replace("TOKEN", &json!(token).to_string());
    let urls = evaluate_quietly(connection, page, &inventory, deadline)?;
    if urls.as_array().is_none_or(Vec::is_empty) {
        return Ok(());
    }
    let session = Some(page.session_id.as_str());
    connection.call("DOM.enable", json!({}), session, deadline)?;
    connection.discard_events(|event| {
        event.session_id.as_deref() == session && event.method.starts_with("CSS.")
    });
    let dropped = connection.dropped_event_count();
    let result = collect(connection, page, deadline);
    let cleanup = Deadline::after(100).unwrap_or(deadline);
    let css_off = connection.call("CSS.disable", json!({}), session, cleanup);
    let dom_off = connection.call("DOM.disable", json!({}), session, cleanup);
    connection.discard_events(|event| {
        event.session_id.as_deref() == session && event.method.starts_with("CSS.")
    });
    let graph = result?;
    css_off?;
    dom_off?;
    if connection.dropped_event_count() != dropped {
        return Ok(());
    }
    let graph = json!(graph);
    let graph = evaluate_quietly(
        connection,
        page,
        &format!(
            r"(() => {{
      const graph = {graph};
      return graph.map(([url, imports]) => [url, imports?.map(value => new URL(value, url).href)]);
    }})()"
        ),
        deadline,
    )?;
    let mut entries: HashMap<String, Option<Vec<String>>> = HashMap::new();
    for entry in graph.as_array().into_iter().flatten() {
        let Some(url) = entry[0].as_str() else {
            continue;
        };
        let imports = entry[1].as_array().and_then(|values| {
            values
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect()
        });
        entries
            .entry(url.to_owned())
            .and_modify(|old| {
                if old != &imports {
                    *old = None;
                }
            })
            .or_insert(imports);
    }
    let safe: Vec<_> = urls
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|url| safe_imports(url, &entries, &mut HashSet::new(), &mut 256))
        .collect();
    evaluate_quietly(
        connection,
        page,
        &format!(
            r"(() => {{
      const pending = window[Symbol.for('tabref.capture-opaque')];
      if (pending?.token !== {}) return;
      pending.approved = pending.entries.filter(entry => {}.includes(entry.href));
    }})()",
            json!(token),
            json!(safe)
        ),
        deadline,
    )?;
    Ok(())
}

fn collect(
    connection: &mut CdpConnection,
    page: &PageSession,
    deadline: Deadline,
) -> Result<Graph, AppError> {
    let session = Some(page.session_id.as_str());
    connection.call("CSS.enable", json!({}), session, deadline)?;
    let events = connection.drain_events(|event| {
        event.session_id.as_deref() == session && event.method == "CSS.styleSheetAdded"
    });
    if events.len() > 128 {
        return Ok(Vec::new());
    }
    let mut graph = Vec::new();
    for event in events {
        let header = &event.params["header"];
        let Some(url) = header["sourceURL"].as_str().filter(|url| !url.is_empty()) else {
            continue;
        };
        if header["isInline"].as_bool() != Some(false) {
            continue;
        }
        if header["length"]
            .as_u64()
            .is_none_or(|length| length > 1_000_000)
        {
            graph.push((url.to_owned(), None));
            continue;
        }
        let reply = connection.call(
            "CSS.getStyleSheetText",
            json!({"styleSheetId":header["styleSheetId"]}),
            session,
            deadline,
        )?;
        let parsed = reply["text"]
            .as_str()
            .and_then(|text| super::css::classify(url, text));
        graph.push((url.to_owned(), parsed));
    }
    Ok(graph)
}

fn safe_imports(
    url: &str,
    graph: &HashMap<String, Option<Vec<String>>>,
    seen: &mut HashSet<String>,
    budget: &mut usize,
) -> bool {
    if *budget == 0 || seen.len() >= 16 || !seen.insert(url.to_owned()) {
        return false;
    }
    *budget -= 1;
    let safe = graph
        .get(url)
        .and_then(Option::as_ref)
        .is_some_and(|imports| {
            imports
                .iter()
                .all(|url| safe_imports(url, graph, seen, budget))
        });
    seen.remove(url);
    safe
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_unreadable_cyclic_and_excessive_imports_are_blind() {
        for graph in [
            HashMap::from([("a".into(), Some(vec!["missing".into()]))]),
            HashMap::from([("a".into(), None)]),
            HashMap::from([("a".into(), Some(vec!["a".into()]))]),
            HashMap::from([
                ("a".into(), Some(vec!["b".into(); 300])),
                ("b".into(), Some(vec![])),
            ]),
        ] {
            assert!(!safe_imports("a", &graph, &mut HashSet::new(), &mut 256));
        }
        let graph = HashMap::from([
            ("a".into(), Some(vec!["b".into()])),
            ("b".into(), Some(vec![])),
        ]);
        assert!(safe_imports("a", &graph, &mut HashSet::new(), &mut 256));
    }
}
