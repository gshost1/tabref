//! Addresses as results print them (a same-origin address as its path,
//! query and fragment, anything else in full) and as steps give them.

/// `after` as seen from `before`: `/admin/products?saved=1` when both share
/// an origin, `after` itself otherwise.
pub(crate) fn short_url(before: &str, after: &str) -> String {
    match (origin(before), origin(after)) {
        (Some(left), Some(right)) if left == right => {
            let rest = &after[right.len()..];
            if rest.is_empty() {
                "/".to_owned()
            } else {
                rest.to_owned()
            }
        }
        _ => after.to_owned(),
    }
}

/// Resolves `target` against the page at `base`: `/path`, `?query`,
/// `#fragment` and `./relative` forms are completed from `base`; anything
/// else is returned as it is.
pub(crate) fn resolve_url(base: &str, target: &str) -> String {
    let Some(origin) = origin(base) else {
        return target.to_owned();
    };
    let page = base.split(['?', '#']).next().unwrap_or(base);
    if target.starts_with("//") {
        let scheme = origin.split("://").next().unwrap_or("https");
        return format!("{scheme}:{target}");
    }
    if target.starts_with('/') {
        return format!("{origin}{target}");
    }
    if target.starts_with('?') {
        return format!("{page}{target}");
    }
    if target.starts_with('#') {
        let without = base.split('#').next().unwrap_or(base);
        return format!("{without}{target}");
    }
    if let Some(relative) = target.strip_prefix("./") {
        let directory = match page.rfind('/') {
            Some(slash) if slash >= origin.len() => &page[..=slash],
            _ => return format!("{origin}/{relative}"),
        };
        return format!("{directory}{relative}");
    }
    target.to_owned()
}

fn origin(url: &str) -> Option<&str> {
    let scheme_end = url.find("://")?;
    let rest = &url[scheme_end + 3..];
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(&url[..scheme_end + 3 + host_end])
}

#[cfg(test)]
#[path = "page_url_tests.rs"]
mod tests;
