/// Resolves `href` against `base` the way a browser does for the common
/// shapes: absolute (`https:`, `mailto:`), scheme-relative (`//host/x`),
/// root-relative (`/x`), query-only (`?q`), fragment-only (`#f`) and
/// relative (`x/y`, `../y`). Dot segments are removed.
pub fn resolve_href(base: &str, href: &str) -> String {
    let href = href.trim();
    if has_scheme(href) {
        return href.to_owned();
    }
    let Some((scheme, rest)) = base.split_once("://") else {
        return href.to_owned();
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let origin = format!("{scheme}://{}", &rest[..authority_end]);
    let base_path_full = &rest[authority_end..];
    let base_no_fragment = base.split('#').next().unwrap_or(base);
    if let Some(network) = href.strip_prefix("//") {
        return format!("{scheme}://{network}");
    }
    if href.is_empty() {
        return base_no_fragment.to_owned();
    }
    if href.starts_with('#') {
        return format!("{base_no_fragment}{href}");
    }
    let base_path = base_path_full
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    if href.starts_with('?') {
        let path = if base_path.is_empty() { "/" } else { base_path };
        return format!("{origin}{path}{href}");
    }
    let joined = if href.starts_with('/') {
        href.to_owned()
    } else {
        let directory = base_path.rfind('/').map_or("/", |end| &base_path[..=end]);
        format!("{directory}{href}")
    };
    format!("{origin}{}", remove_dot_segments(&joined))
}

/// `url` as short as it can be read from a page at `page_url`: the path,
/// query and fragment when both share an origin, `#fragment` when only the
/// fragment differs, else the whole address without `https://`.
pub fn short_href(page_url: &str, url: &str) -> String {
    let page_origin = origin_of(page_url);
    match (page_origin, origin_of(url)) {
        (Some(page), Some(target)) if page == target => {
            let rest = &url[target.len()..];
            let rest = if rest.is_empty() { "/" } else { rest };
            let page_rest = page_url[page.len()..].split('#').next().unwrap_or_default();
            if let Some((path, fragment)) = rest.split_once('#') {
                if path == page_rest {
                    return format!("#{fragment}");
                }
            }
            rest.to_owned()
        }
        _ => url.strip_prefix("https://").unwrap_or(url).to_owned(),
    }
}

/// `scheme://host[:port]` of an absolute http(s) address.
pub fn origin_of(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(&url[..scheme.len() + 3 + end])
}

fn has_scheme(href: &str) -> bool {
    let Some(colon) = href.find(':') else {
        return false;
    };
    let scheme = &href[..colon];
    !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && !href[..colon].contains('/')
}

fn remove_dot_segments(path: &str) -> String {
    let (path, suffix) = match path.find(['?', '#']) {
        Some(at) => (&path[..at], &path[at..]),
        None => (path, ""),
    };
    let mut kept: Vec<&str> = Vec::new();
    let segments: Vec<&str> = path.split('/').collect();
    let last = segments.len().saturating_sub(1);
    for (position, segment) in segments.iter().enumerate() {
        match *segment {
            "." => {
                if position == last {
                    kept.push("");
                }
            }
            ".." => {
                if kept.len() > 1 {
                    kept.pop();
                }
                if position == last {
                    kept.push("");
                }
            }
            other => kept.push(other),
        }
    }
    let mut joined = kept.join("/");
    if !joined.starts_with('/') {
        joined.insert(0, '/');
    }
    format!("{joined}{suffix}")
}

#[cfg(test)]
#[path = "url_resolve_tests.rs"]
mod tests;
