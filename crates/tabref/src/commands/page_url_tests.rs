use super::{resolve_url, same_page, short_url};

const EDIT: &str = "http://127.0.0.1:8792/admin/catalog/product/4499/edit";

#[test]
fn a_same_origin_address_prints_as_its_path() {
    assert_eq!(
        short_url(EDIT, "http://127.0.0.1:8792/admin/catalog/products?saved=1"),
        "/admin/catalog/products?saved=1"
    );
    assert_eq!(short_url(EDIT, "http://127.0.0.1:8792"), "/");
    assert_eq!(
        short_url(EDIT, "https://example.com/a"),
        "https://example.com/a"
    );
}

#[test]
fn a_page_is_its_address_without_query_or_fragment() {
    assert!(same_page(
        "http://x/forum/f/books?sort=new",
        "http://x/forum/f/books?sort=hot#top"
    ));
    assert!(!same_page("http://x/a/1/edit", "http://x/a"));
}

#[test]
fn relative_targets_complete_from_the_current_page() {
    assert_eq!(
        resolve_url(EDIT, "/admin/catalog/product/5916/edit"),
        "http://127.0.0.1:8792/admin/catalog/product/5916/edit"
    );
    assert_eq!(
        resolve_url("http://x/shop/product/1?rp=1", "?rp=2"),
        "http://x/shop/product/1?rp=2"
    );
    assert_eq!(resolve_url("http://x/a/b", "./c"), "http://x/a/c");
    assert_eq!(resolve_url("http://x/a#top", "#end"), "http://x/a#end");
    assert_eq!(resolve_url(EDIT, "example.com"), "example.com");
    assert_eq!(resolve_url("about:blank", "/a"), "/a");
}
