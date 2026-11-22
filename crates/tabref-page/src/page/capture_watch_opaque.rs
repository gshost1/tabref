use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::trace::record;
use super::{CdpConnection, PageSession, evaluate_quietly};

type Graph = Vec<(String, Option<Vec<String>>)>;

pub(super) fn prepare(
    connection: &mut CdpConnection,
    page: &PageSession,
    token: &str,
    urls: &[Value],
    deadline: Deadline,
) -> Result<(), AppError> {
    let session = Some(page.session_id.as_str());
    cdp(connection, page, "DOM.enable", json!({}), deadline)?;
    let stale = connection.drain_events(|event| {
        event.session_id.as_deref() == session && event.method.starts_with("CSS.")
    });
    record("opaque.drain", || json!({"stale":stale.len(),"urls":urls}));
    let dropped = connection.dropped_event_count();
    let result = collect(connection, page, deadline);
    let cleanup = Deadline::after(100).unwrap_or(deadline);
    let css_off = cdp(connection, page, "CSS.disable", json!({}), cleanup);
    let dom_off = cdp(connection, page, "DOM.disable", json!({}), cleanup);
    let late = connection.drain_events(|event| {
        event.session_id.as_deref() == session && event.method.starts_with("CSS.")
    });
    let graph = result?;
    css_off?;
    dom_off?;
    record(
        "opaque.events",
        || json!({"late":late.iter().map(|event| json!({"method":event.method,"params":event.params})).collect::<Vec<_>>(),"dropped":connection.dropped_event_count() - dropped,"graph":graph.len()}),
    );
    if connection.dropped_event_count() != dropped || !late.is_empty() {
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
        .iter()
        .filter_map(Value::as_str)
        .filter(|url| safe_imports(url, &entries, &mut HashSet::new(), &mut 256))
        .collect();
    record(
        "opaque.approval",
        || json!({"requested":urls.len(),"approved":safe.len()}),
    );
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
    cdp(connection, page, "CSS.enable", json!({}), deadline)?;
    let events = connection.drain_events(|event| {
        event.session_id.as_deref() == session && event.method == "CSS.styleSheetAdded"
    });
    record("opaque.added", || json!({"count":events.len()}));
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
        let reply = cdp(
            connection,
            page,
            "CSS.getStyleSheetText",
            json!({"styleSheetId":header["styleSheetId"]}),
            deadline,
        )?;
        let parsed = reply["text"].as_str().and_then(|text| {
            super::css::classify(header["styleSheetId"].as_str().unwrap_or(""), text)
        });
        if parsed.is_none() {
            record(
                "opaque.reject",
                || json!({"url":url,"construct":reply["text"].as_str().map(super::css::reject_reason)}),
            );
        }
        graph.push((url.to_owned(), parsed));
    }
    let changed = connection.drain_events(|event| {
        event.session_id.as_deref() == session
            && matches!(
                event.method.as_str(),
                "CSS.styleSheetChanged" | "CSS.styleSheetRemoved"
            )
    });
    if !changed.is_empty() {
        record("opaque.changed", || json!({"count":changed.len()}));
        graph.clear();
    }
    Ok(graph)
}

fn cdp(
    connection: &mut CdpConnection,
    page: &PageSession,
    method: &str,
    params: Value,
    deadline: Deadline,
) -> Result<Value, AppError> {
    connection
        .call(method, params, Some(&page.session_id), deadline)
        .inspect_err(|error| {
            record(
                "opaque.cdp_error",
                || json!({"step":method,"error":error.to_string()}),
            );
        })
}

fn safe_imports(
    url: &str,
    graph: &HashMap<String, Option<Vec<String>>>,
    seen: &mut HashSet<String>,
    budget: &mut usize,
) -> bool {
    if *budget == 0 || seen.len() >= 16 || !seen.insert(url.to_owned()) {
        record(
            "opaque.import",
            || json!({"url":url,"reason":"cycle-or-budget"}),
        );
        return false;
    }
    *budget -= 1;
    if graph.get(url).is_none_or(Option::is_none) {
        record(
            "opaque.import",
            || json!({"url":url,"reason":if graph.contains_key(url) {"unclassifiable"} else {"missing"}}),
        );
    }
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
