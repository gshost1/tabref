//! The page scripts `select` runs on an ARIA combobox, a button that opens
//! a listbox, or a listbox. Each finds the control's popup the same way: the
//! listbox its `aria-controls` or `aria-owns` names (on the control or on
//! the text field inside it), the control itself when it is a listbox, else
//! the one listbox that became visible since the first read. The first read
//! records which listboxes were visible then.

macro_rules! find {
    () => {
        r"
  const doc = this.ownerDocument;
  const root = this.getRootNode();
  const memo = Symbol.for('tabref.aria-select');
  const norm = text => String(text || '').replace(/\s+/g, ' ').trim();
  const roleOf = el => norm(el.getAttribute && el.getAttribute('role')).split(' ')[0].toLowerCase();
  const byId = id => (root.getElementById ? root.getElementById(id) : null) || doc.getElementById(id);
  const named = (el, attr) => norm(el.getAttribute(attr)).split(' ').filter(Boolean).map(byId).filter(Boolean);
  const shown = el => !!el && el.isConnected && !el.closest('[hidden]') && el.getClientRects().length > 0
    && (!el.checkVisibility || el.checkVisibility({ visibilityProperty: true, opacityProperty: true }));
  const typable = el => !!el && !el.readOnly && !el.disabled && el.getAttribute('aria-readonly') !== 'true'
    && (el.localName === 'textarea' || el.isContentEditable
      || (el.localName === 'input' && /^(text|search|email|url|tel|)$/.test(norm(el.getAttribute('type')).toLowerCase())));
  const role = roleOf(this);
  const tag = this.localName;
  let kind = 'none';
  let field = typable(this) ? this : null;
  if (role === 'combobox') {
    kind = 'combobox';
    if (!field && !(this.localName === 'input' || this.localName === 'textarea')) {
      field = this.querySelector('input, textarea, [role=textbox], [role=searchbox]');
    }
    if (field && !typable(field)) field = null;
  } else if (role === 'listbox') {
    kind = 'listbox';
  } else if ((tag === 'button' || role === 'button') && norm(this.getAttribute('aria-haspopup')).toLowerCase() === 'listbox') {
    kind = 'button';
  }
  const listboxIn = el => roleOf(el) === 'listbox' ? el : el.querySelector('[role=listbox]');
  let popup = kind === 'listbox' ? this : null;
  for (const el of [this, field].filter(Boolean)) {
    for (const target of [...named(el, 'aria-controls'), ...named(el, 'aria-owns')]) {
      if (!popup) popup = listboxIn(target);
    }
  }
  const visible = () => Array.from(doc.querySelectorAll('[role=listbox]')).filter(shown);
  if (!popup && kind !== 'none') {
    const before = window[memo];
    const fresh = before ? visible().filter(el => !before.has(el)) : [];
    if (fresh.length === 1) popup = fresh[0];
  }
  const options = !popup ? [] : [
    ...popup.querySelectorAll('[role=option]'),
    ...named(popup, 'aria-owns').flatMap(el => roleOf(el) === 'option' ? [el] : [...el.querySelectorAll('[role=option]')]),
  ];
"
    };
}

/// Reads the control: what it is, whether its popup shows options, the
/// options with their labels, and the value it displays. `record` keeps the
/// listboxes visible now, for a popup no attribute names.
pub(crate) const READ: &str = concat!(
    "function(record) {",
    find!(),
    r"
  if (record) window[memo] = new Set(visible());
  const off = el => {
    const shut = el.closest('[aria-disabled=true]');
    return !!shut && (shut === el || popup.contains(shut));
  };
  const list = options.map(option => ({
    label: norm(option.getAttribute('aria-label')) || norm(option.textContent),
    text: norm(option.textContent),
    value: '',
    disabled: off(option),
    shown: shown(option),
    selected: option.getAttribute('aria-selected') === 'true',
  }));
  const own = el => {
    let text = '';
    const walk = node => {
      for (const child of node.childNodes) {
        if (child.nodeType === 3) text += ' ' + child.data;
        else if (child.nodeType === 1 && child !== popup && !['listbox', 'option'].includes(roleOf(child))
          && !['script', 'style', 'template'].includes(child.localName)) walk(child);
      }
    };
    walk(el);
    return norm(text);
  };
  const value = kind === 'listbox' ? list.filter(option => option.selected).map(option => option.label).join(', ')
    : field ? norm(field.isContentEditable ? field.textContent : field.value)
    : (this.localName === 'input' || this.localName === 'textarea') ? norm(this.value) : own(this);
  let focus = doc.activeElement;
  while (focus && focus.shadowRoot && focus.shadowRoot.activeElement) focus = focus.shadowRoot.activeElement;
  return {
    kind,
    editable: !!field,
    open: !!popup && shown(popup) && list.some(option => option.shown),
    multi: !!popup && popup.getAttribute('aria-multiselectable') === 'true',
    focused: !!focus && (focus === this || focus === field || this.contains(focus)),
    options: list,
    value,
    describe: '<' + tag + (role ? ' role=' + role : '') + '>',
  };
}"
);

/// The option at `index` of the control's popup, as an element, only when
/// its label is still `label`: options that were drawn again since the
/// read give `null`, never a neighbour.
pub(crate) const OPTION_AT: &str = concat!(
    "function(index, label) {",
    find!(),
    r"  const option = options[index];
  if (!option) return null;
  const now = norm(option.getAttribute('aria-label')) || norm(option.textContent);
  return now === label ? option : null;
}"
);

/// The text field inside an ARIA 1.1 combobox, as an element.
pub(crate) const FIELD: &str = concat!("function() {", find!(), "  return field;\n}");

#[cfg(test)]
#[path = "aria_select_scripts_tests.rs"]
mod tests;
