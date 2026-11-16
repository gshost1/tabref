use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::element_call::ElementCall;
use super::element_errors::{script_failed, stale};
use super::file_input_choice::FileInputCandidate;
use super::remote_element::call_function;
use crate::{CdpConnection, ElementToken, PageSession};

/// Lists, or with `mode` "pick" returns, the file inputs of the target's
/// document, open shadow roots included, each with the number of steps up
/// from the target to the first ancestor that holds it. Shadow roots are
/// crossed to their host on the way up, so a drop zone inside a component
/// still finds an input beside the component.
const SEARCH: &str = r"function(mode, index) {
  const isFile = (el) => !!el && el.localName === 'input' && el.type === 'file';
  const up = (node) => {
    const parent = node.parentNode;
    if (!parent) return null;
    return parent.nodeType === 11 && parent.host ? parent.host : parent;
  };
  const chain = [];
  for (let node = this; node; node = up(node)) chain.push(node);
  const place = new Map(chain.map((node, at) => [node, at]));
  const doc = this.ownerDocument || document;
  const top = doc.body || doc.documentElement;
  const inputs = [];
  const under = (root) => {
    for (const el of root.querySelectorAll('*')) {
      if (isFile(el)) inputs.push(el);
      if (el.shadowRoot) under(el.shadowRoot);
    }
  };
  under(doc);
  const label = this.localName === 'label' ? this : (this.closest ? this.closest('label') : null);
  const labelled = label && isFile(label.control) ? label.control : null;
  for (const extra of [this, labelled]) {
    if (isFile(extra) && !inputs.includes(extra)) inputs.push(extra);
  }
  if (mode === 'pick') return inputs[index] || null;
  const depthOf = (el) => {
    for (let node = el; node; node = up(node)) {
      if (place.has(node)) return place.get(node);
    }
    return null;
  };
  const nameOf = (el) => {
    if (el.id) return 'input#' + el.id;
    if (el.name) return 'input[name=' + el.name + ']';
    const classes = (el.getAttribute('class') || '').trim().split(/\s+/).filter(Boolean).slice(0, 2);
    if (classes.length) return 'input.' + classes.join('.');
    return 'input[type=file]';
  };
  const hiddenOf = (el) => {
    const box = el.getBoundingClientRect();
    if (box.width * box.height === 0) return true;
    return el.checkVisibility ? !el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }) : false;
  };
  return {
    top: place.has(top) ? place.get(top) : null,
    candidates: inputs.map((el, at) => ({
      index: at,
      name: nameOf(el),
      hidden: hiddenOf(el),
      multiple: el.multiple,
      disabled: el.disabled,
      self: el === this,
      labelled: el === labelled,
      depth: depthOf(el),
    })),
  };
}";

/// The file inputs the target can reach, and the depth of the page body
/// above it.
pub(crate) struct FileInputs {
    pub(crate) candidates: Vec<FileInputCandidate>,
    pub(crate) top: Option<u32>,
}

pub(crate) fn list_file_inputs(
    connection: &mut CdpConnection,
    session: &PageSession,
    object: &str,
    deadline: Deadline,
) -> Result<FileInputs, AppError> {
    let listed = call_function(
        connection,
        session,
        object,
        ElementCall {
            function: SEARCH,
            arguments: vec![json!("list"), json!(0)],
        },
        deadline,
    )?;
    let candidates = listed
        .get("candidates")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| AppError::internal(format!("file input search: {error}")))?
        .unwrap_or_default();
    let top = listed
        .get("top")
        .and_then(Value::as_u64)
        .and_then(|depth| u32::try_from(depth).ok());
    Ok(FileInputs { candidates, top })
}

/// The remote object id of the input the search listed at `index`.
pub(crate) fn pick_file_input(
    connection: &mut CdpConnection,
    session: &PageSession,
    object: &str,
    index: usize,
    token: &ElementToken,
    deadline: Deadline,
) -> Result<String, AppError> {
    let reply = connection.call(
        "Runtime.callFunctionOn",
        json!({
            "objectId": object,
            "functionDeclaration": SEARCH,
            "arguments": [{ "value": "pick" }, { "value": index }],
            "returnByValue": false,
            "awaitPromise": false,
        }),
        Some(&session.session_id),
        deadline,
    )?;
    if let Some(exception) = reply.get("exceptionDetails") {
        return Err(script_failed(exception));
    }
    reply
        .pointer("/result/objectId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| stale(token, "the file input left the page during the upload"))
}
