use super::normalize_url;

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
        "../b",
        "//cdn.example.com/x",
    ] {
        assert_eq!(normalize_url(url), url);
        assert!(super::is_relative(url), "{url}");
    }
    assert!(!super::is_relative("example.com/a"));
    assert!(!super::is_relative("http://127.0.0.1:8000/"));
}
