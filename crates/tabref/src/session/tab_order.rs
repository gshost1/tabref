use tabref_cdp::PageTarget;

/// Orders `tabs` the way the last listing did, so an index an agent read
/// from `tabs` still names the same tab after another tab opens: tabs in
/// `saved` (one target id per line) keep their order, and tabs it does not
/// name follow in the browser's order. Closed tabs drop out.
pub(crate) fn stable_order(tabs: Vec<PageTarget>, saved: &str) -> Vec<PageTarget> {
    let mut remaining = tabs;
    let mut ordered = Vec::with_capacity(remaining.len());
    for id in saved.lines().map(str::trim).filter(|id| !id.is_empty()) {
        if let Some(index) = remaining.iter().position(|tab| tab.target_id == id) {
            ordered.push(remaining.remove(index));
        }
    }
    ordered.extend(remaining);
    ordered
}

/// The listing as `stable_order` reads it back.
pub(crate) fn order_text(tabs: &[PageTarget]) -> String {
    tabs.iter()
        .map(|tab| tab.target_id.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "tab_order_tests.rs"]
mod tests;
