use super::{origin_of, resolve_href, short_href};

const BASE: &str = "http://127.0.0.1:8792/admin/catalog/products?p=2#top";

#[test]
fn hrefs_resolve_like_a_browser() {
    assert_eq!(
        resolve_href(BASE, "/admin/catalog/product/4499/edit"),
        "http://127.0.0.1:8792/admin/catalog/product/4499/edit"
    );
    assert_eq!(
        resolve_href(BASE, "?p=3"),
        "http://127.0.0.1:8792/admin/catalog/products?p=3"
    );
    assert_eq!(
        resolve_href(BASE, "#rows"),
        "http://127.0.0.1:8792/admin/catalog/products?p=2#rows"
    );
    assert_eq!(
        resolve_href(BASE, "product/1"),
        "http://127.0.0.1:8792/admin/catalog/product/1"
    );
    assert_eq!(
        resolve_href(BASE, "../shop/./x"),
        "http://127.0.0.1:8792/admin/shop/x"
    );
    assert_eq!(
        resolve_href(BASE, "//cdn.test/a.js"),
        "http://cdn.test/a.js"
    );
    assert_eq!(resolve_href(BASE, "mailto:a@b.c"), "mailto:a@b.c");
}

#[test]
fn same_origin_links_shorten_to_their_path() {
    let page = "http://127.0.0.1:8792/shop/";
    assert_eq!(
        short_href(page, "http://127.0.0.1:8792/shop/category/women-shoes/?p=2"),
        "/shop/category/women-shoes/?p=2"
    );
    assert_eq!(
        short_href(page, "http://127.0.0.1:8792/shop/#reviews"),
        "#reviews"
    );
    assert_eq!(
        short_href(page, "https://arxiv.org/abs/1"),
        "arxiv.org/abs/1"
    );
    assert_eq!(
        short_href(page, "http://other.test/x"),
        "http://other.test/x"
    );
    assert_eq!(origin_of("https://a.test:8/x?y"), Some("https://a.test:8"));
    assert_eq!(origin_of("data:text/html,x"), None);
}
