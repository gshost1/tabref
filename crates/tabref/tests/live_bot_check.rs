//! Exercises the view on loopback fixtures, with external iframe loads
//! blocked by the fixtures' content security policy.

mod common;

use common::{harness, live, serve};
use serde_json::json;

const NOTE: &str = "note: bot check on this page";
const CLOUDFLARE: &str = include_str!("fixtures/bot_check/cloudflare.html");
const HCAPTCHA: &str = include_str!("fixtures/bot_check/hcaptcha.html");
const RECAPTCHA: &str = include_str!("fixtures/bot_check/recaptcha.html");
const ARTICLE: &str = include_str!("fixtures/bot_check/article.html");

#[test]
fn challenge_pages_replace_the_view_until_the_page_changes() {
    if !live() {
        return;
    }
    let port = serve(&[
        ("/cloudflare", CLOUDFLARE),
        ("/hcaptcha", HCAPTCHA),
        ("/recaptcha", RECAPTCHA),
        ("/article", ARTICLE),
    ]);
    let tabref = harness("bot-check");
    let url = |path: &str| format!("http://127.0.0.1:{port}/{path}");

    for path in ["cloudflare", "hcaptcha", "recaptcha"] {
        let address = url(path);
        let opened = tabref.ok(&["open", &address]);
        assert_eq!(opened.stdout.trim_end(), NOTE, "{path}");

        let snapped = tabref.ok(&["--json", "snap"]).json();
        assert_eq!(snapped["data"]["bot_check"], true, "{snapped}");
        assert_eq!(snapped["data"]["url"], address, "{snapped}");
        assert_eq!(snapped["data"]["lines"], json!([NOTE]), "{snapped}");
        assert!(snapped["data"]["snapshot_id"].is_null(), "{snapped}");
    }
    for command in [&["read"][..], &["tab", "0"]] {
        assert_eq!(tabref.ok(command).stdout.trim_end(), NOTE, "{command:?}");
    }

    let opened = tabref.ok(&["open", &url("article")]);
    assert!(!opened.stdout.contains(NOTE), "{}", opened.stdout);
    assert!(opened.stdout.contains("The library opened in 1952."));
}
