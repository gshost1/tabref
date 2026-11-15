use super::is_embedded_runtime;

#[test]
fn an_electron_user_agent_is_an_embedded_runtime_and_chrome_is_not() {
    assert!(is_embedded_runtime(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Slack/4.41.0 Chrome/128.0.0.0 Electron/32.1.0 Safari/537.36"
    ));
    assert!(!is_embedded_runtime(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/156.0.0.0 Safari/537.36"
    ));
    assert!(!is_embedded_runtime(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36"
    ));
}
