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
