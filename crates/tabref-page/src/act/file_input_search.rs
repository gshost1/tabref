use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::element_call::ElementCall;
use super::element_errors::script_failed;
use super::file_input_choice::FileInputCandidate;
use super::remote_element::call_function;
use crate::{CdpConnection, PageSession};

/// Lists, or with `mode` "pick" returns, the file inputs of the target's
/// document, open shadow roots included, each with the number of steps up
/// from the target to the first ancestor that holds it. Shadow roots are
/// crossed to their host on the way up, so a drop zone inside a component
/// still finds an input beside the component. An input `crosses` when it
/// belongs to a form other than the target's, or sits in a shadow root
/// whose host is not around the target. The list carries a `key` of
/// everything the choice rests on; "pick" returns the input only while the
/// page still gives the same key, so an input that a re-render moved into
/// the listed place is never the one that gets the files. The names keep
/// only ASCII letters, digits, `_`, `.` and `-` of the page's attributes.
const SEARCH: &str = r"function(mode, index, expected) {
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
  const top = place.has(doc.body) ? doc.body : doc.documentElement;
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
  const ownForm = this.form && this.form.localName === 'form' ? this.form : null;
  const targetForm = ownForm || chain.find((node) => node.localName === 'form') || null;
  const crosses = (el) => {
    if (el.form && el.form !== targetForm) return true;
    for (let root = el.getRootNode(); root && root.host; root = root.host.getRootNode()) {
      if (!place.has(root.host)) return true;
    }
    return false;
  };
  const depthOf = (el) => {
    for (let node = el; node; node = up(node)) {
      if (place.has(node)) return place.get(node);
    }
    return null;
  };
  const tidy = (text) => String(text).slice(0, 100).replace(/[^A-Za-z0-9_.-]/g, '');
  const nameOf = (el) => {
    if (tidy(el.id)) return 'input#' + tidy(el.id);
    if (tidy(el.name)) return 'input[name=' + tidy(el.name) + ']';
    const classes = (el.getAttribute('class') || '').trim().split(/\s+/).map(tidy).filter(Boolean).slice(0, 2);
    if (classes.length) return 'input.' + classes.join('.');
    return 'input[type=file]';
  };
  const hiddenOf = (el) => {
    const box = el.getBoundingClientRect();
    if (box.width * box.height === 0) return true;
    return el.checkVisibility ? !el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }) : false;
  };
  const candidates = inputs.map((el, at) => ({
    index: at,
    name: nameOf(el),
    multiple: el.multiple,
    disabled: el.disabled,
    self: el === this,
    labelled: el === labelled,
    depth: depthOf(el),
    crosses: crosses(el),
  }));
  const at = place.has(top) ? place.get(top) : null;
  const hasForm = targetForm !== null;
  const key = JSON.stringify([at, hasForm, candidates]);
  if (mode === 'pick') return key === expected ? inputs[index] || null : null;
  candidates.forEach((candidate, at) => {
    const el = inputs[at];
    candidate.hidden = hiddenOf(el);
    if (doc.defaultView !== doc.defaultView.top || el.getRootNode() !== doc) return;
    try {
      const matches = doc.querySelectorAll(candidate.name);
      if (matches.length === 1 && matches[0] === el) candidate.selector = candidate.name;
    } catch {}
  });
  return { top: at, hasForm, key, candidates };
}";

pub(crate) const UPLOAD_OBJECT_GROUP: &str = "tabref-upload";

/// The file inputs the target can reach, the depth of the page body above
/// it, and the key "pick" checks the page against.
pub(crate) struct FileInputs {
    pub(crate) candidates: Vec<FileInputCandidate>,
    pub(crate) top: Option<u32>,
    pub(crate) has_form: bool,
    pub(crate) key: String,
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
            arguments: vec![json!("list"), json!(0), json!("")],
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
    let key = listed
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Ok(FileInputs {
        candidates,
        top,
        has_form: listed
            .get("hasForm")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        key,
    })
}

/// The remote object id of the input the search listed at `index`, or
/// `None` when the page's file inputs are no longer the ones it listed.
pub(crate) fn pick_file_input(
    connection: &mut CdpConnection,
    session: &PageSession,
    object: &str,
    listed: (&FileInputs, usize),
    deadline: Deadline,
) -> Result<Option<String>, AppError> {
    let (inputs, index) = listed;
    let reply = connection.call(
        "Runtime.callFunctionOn",
        json!({
            "objectId": object,
            "functionDeclaration": SEARCH,
            "arguments": [{ "value": "pick" }, { "value": index }, { "value": inputs.key }],
            "returnByValue": false,
            "awaitPromise": false,
            "objectGroup": UPLOAD_OBJECT_GROUP,
        }),
        Some(&session.session_id),
        deadline,
    )?;
    if let Some(exception) = reply.get("exceptionDetails") {
        return Err(script_failed(exception));
    }
    Ok(reply
        .pointer("/result/objectId")
        .and_then(Value::as_str)
        .map(str::to_owned))
}
