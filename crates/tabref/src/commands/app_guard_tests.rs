use super::{is_embedded_runtime, is_web_page};

#[test]
fn an_electron_user_agent_is_an_embedded_runtime_and_chrome_is_not() {
    assert!(is_embedded_runtime(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Slack/4.41.0 Chrome/128.0.0.0 Electron/32.1.0 Safari/537.36"
    ));
    assert!(!is_embedded_runtime(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/156.0.0.0 Safari/537.36"
    ));
}

#[test]
fn only_web_and_blank_pages_may_always_be_replaced() {
    assert!(is_web_page("https://app.slack.com/client"));
    assert!(is_web_page("HTTP://127.0.0.1:8000/"));
    assert!(is_web_page("about:blank"));
    assert!(is_web_page(""));
    assert!(!is_web_page("file:///opt/notes/index.html"));
    assert!(!is_web_page("app://obsidian.md/index.html"));
    assert!(!is_web_page("vscode-file://vscode-app/workbench.html"));
}
