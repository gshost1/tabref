use tabref_core::AppError;
use tabref_page::PageInfo;

use super::{normalize_url, open_again, unread_page};

#[test]
fn a_pending_navigation_in_open_may_be_opened_again() {
    let pending = AppError::timeout("http://a.test/ did not respond before the deadline")
        .with_details(
            serde_json::json!({ "kind": "cdp_navigation_pending", "url": "http://a.test/" }),
        );

    let error = open_again(pending);

    let details = error.details.unwrap();
    assert_eq!(details["kind"], "cdp_navigation_pending");
    assert_eq!(details["retryable"], true);
    assert!(error.suggestion.unwrap().contains("same open again"));
}

#[test]
fn a_bare_host_gets_https_and_schemes_are_kept() {
    assert_eq!(normalize_url("example.com"), "https://example.com");
    assert_eq!(
        normalize_url(" example.com/a?b=1 "),
        "https://example.com/a?b=1"
    );
    assert_eq!(
        normalize_url("http://127.0.0.1:8000/"),
        "http://127.0.0.1:8000/"
    );
    assert_eq!(normalize_url("about:blank"), "about:blank");
    assert_eq!(normalize_url("data:text/html,hi"), "data:text/html,hi");
}

#[test]
fn an_existing_local_file_becomes_a_file_url() {
    let file = std::env::temp_dir().join(format!("tabref-open-{}.html", std::process::id()));
    std::fs::write(&file, "<p>hi</p>").unwrap();
    let url = normalize_url(&file.to_string_lossy());
    std::fs::remove_file(&file).unwrap();
    assert!(url.starts_with("file:///"), "{url}");
    assert!(url.ends_with(".html"));
}

#[test]
fn a_loopback_host_defaults_to_plain_http() {
    assert_eq!(normalize_url("localhost:8000/a"), "http://localhost:8000/a");
    assert_eq!(normalize_url("127.0.0.1"), "http://127.0.0.1");
    assert_eq!(normalize_url("[::1]:3000"), "http://[::1]:3000");
    assert_eq!(
        normalize_url("localhost.example.com"),
        "https://localhost.example.com"
    );
}

#[test]
fn a_path_of_the_current_page_stays_relative() {
    for url in [
        "/shop/account/",
        "?page=2",
        "#top",
        "./a.html",
        "//cdn.example.com/x",
    ] {
        assert_eq!(normalize_url(url), url);
        assert!(super::is_relative(url), "{url}");
    }
    assert!(!super::is_relative("example.com/a"));
    assert!(!super::is_relative("http://127.0.0.1:8000/"));
}

#[test]
fn an_unread_page_names_the_tab_address_or_says_it_is_the_one_opened() {
    let failed = AppError::timeout("snapshot failed");
    let landed = PageInfo {
        url: "http://a.test/landed".into(),
        title: "Landed".into(),
    };
    let report = unread_page("http://a.test/hop", Ok(landed), &failed);
    assert_eq!(
        report.text,
        "page \"Landed\" http://a.test/landed\n\
         note: could not read the page (snapshot failed); run tabref snap to see it"
    );
    assert_eq!(report.data["url"], "http://a.test/landed");
    assert_eq!(report.data["title"], "Landed");

    let report = unread_page(
        "http://a.test/hop",
        Err(AppError::timeout("tab gone")),
        &failed,
    );
    let said = "could not read the page or the address it ended at (snapshot failed); \
                run tabref snap to see it; the address shown is the one opened";
    assert_eq!(
        report.text,
        format!("opened http://a.test/hop\nnote: {said}")
    );
    assert_eq!(report.data["url"], "http://a.test/hop");
    assert_eq!(report.data["report_error"], said);
}
