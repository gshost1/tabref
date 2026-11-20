//! Exercises the view on loopback fixtures, with external iframe loads
//! blocked by the fixtures' content security policy.

mod common;

use common::{harness, live, serve};

const NOTE: &str = "note: bot check on this page";
const CLOUDFLARE: &str = include_str!("fixtures/bot_check/cloudflare.html");
const HCAPTCHA: &str = include_str!("fixtures/bot_check/hcaptcha.html");
const RECAPTCHA: &str = include_str!("fixtures/bot_check/recaptcha.html");
const ARTICLE: &str = include_str!("fixtures/bot_check/article.html");

#[test]
fn challenge_pages_keep_the_normal_view_and_refs() {
    if !live() {
        return;
    }
    let port = serve(&[
        ("/cloudflare", CLOUDFLARE),
        ("/hcaptcha", HCAPTCHA),
        ("/recaptcha", RECAPTCHA),
        ("/article", ARTICLE),
        ("/cdn-cgi/challenge-platform/scripts/jsd/main.js", ""),
    ]);
    let tabref = harness("bot-check");
    let url = |path: &str| format!("http://127.0.0.1:{port}/{path}");

    for (path, provider, heading) in [
        ("cloudflare", "Cloudflare", "Checking your browser"),
        ("hcaptcha", "hCaptcha", "Verify to continue"),
        ("recaptcha", "reCAPTCHA", "Verify to continue"),
    ] {
        let address = url(path);
        let opened = tabref.ok(&["open", &address]);
        assert_note(&opened.stdout, provider);
        assert!(opened.stdout.contains(heading), "{}", opened.stdout);

        let snapped = tabref.ok(&["--json", "snap"]).json();
        assert_eq!(snapped["data"]["bot_check"], true, "{snapped}");
        assert_eq!(snapped["data"]["url"], address, "{snapped}");
        assert_eq!(snapped["data"]["bot_check_provider"], provider, "{snapped}");
        assert!(snapped["data"]["snapshot_id"].is_string(), "{snapped}");
        assert!(
            snapped["data"]["lines"]
                .as_array()
                .unwrap()
                .iter()
                .any(|line| line.as_str().unwrap().contains(heading)),
            "{snapped}"
        );
    }
    for command in [&["read"][..], &["tab", "0"]] {
        let view = tabref.ok(command);
        assert_note(&view.stdout, "reCAPTCHA");
        assert!(view.stdout.contains("Log in"), "{}", view.stdout);
    }
    let view = tabref.ok(&["snap"]);
    let button = view
        .stdout
        .split('[')
        .find(|part| part.contains("button Log in"))
        .unwrap();
    let reference = button.split_whitespace().next().unwrap();
    let scoped = tabref.ok(&["snap", "--root", reference]);
    assert_note(&scoped.stdout, "reCAPTCHA");
    assert!(scoped.stdout.contains("Log in"), "{}", scoped.stdout);

    let opened = tabref.ok(&["open", &url("article")]);
    assert!(!opened.stdout.contains(NOTE), "{}", opened.stdout);
    assert!(opened.stdout.contains("The library opened in 1952."));

    let clicked = tabref.ok(&["--json", "click", "Sign in"]).json();
    let page = &clicked["data"]["page"];
    assert_eq!(page["bot_check_provider"], "reCAPTCHA", "{clicked}");
    assert!(page["snapshot_id"].is_string(), "{clicked}");
    assert!(
        page["ref_count"].as_u64().is_some_and(|count| count > 0),
        "{clicked}"
    );
}

fn assert_note(text: &str, provider: &str) {
    assert_eq!(
        text.lines().next().unwrap(),
        format!("{NOTE} ({provider})"),
        "{text}"
    );
    assert_eq!(text.matches(NOTE).count(), 1, "{text}");
}
