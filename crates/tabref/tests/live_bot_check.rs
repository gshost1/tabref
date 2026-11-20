//! Exercises the view on loopback fixtures, with external iframe loads
//! blocked by the fixtures' content security policy.

mod common;

use common::{harness, live, serve};

const NOTE: &str = "note: bot check on this page";
const CLOUDFLARE: &str = include_str!("fixtures/bot_check/cloudflare.html");
const HCAPTCHA: &str = include_str!("fixtures/bot_check/hcaptcha.html");
const RECAPTCHA: &str = include_str!("fixtures/bot_check/recaptcha.html");
const ARTICLE: &str = include_str!("fixtures/bot_check/article.html");
const SUBMIT: &str = include_str!("fixtures/bot_check/submit.html");

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
    let scoped = tabref.ok(&["snap", "--root", reference(&view.stdout, "button Log in")]);
    assert_note(&scoped.stdout, "reCAPTCHA");
    assert!(scoped.stdout.contains("Log in"), "{}", scoped.stdout);
    let field = tabref.ok(&["read", reference(&view.stdout, "textbox Username")]);
    assert_note(&field.stdout, "reCAPTCHA");
    assert!(field.stdout.contains("Username"), "{}", field.stdout);

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

#[test]
fn revealing_a_challenge_same_page_is_reported_by_click_and_do() {
    if !live() {
        return;
    }
    let port = serve(&[("/submit", SUBMIT)]);
    let tabref = harness("submit-challenge");
    let url = format!("http://127.0.0.1:{port}/submit");
    let expected = format!("{NOTE} (reCAPTCHA)");

    let opened = tabref.ok(&["open", &url]);
    assert!(!opened.stdout.contains(NOTE), "{}", opened.stdout);
    let clicked = tabref.ok(&["click", "Submit"]);
    assert!(clicked.stdout.starts_with("clicked"), "{}", clicked.stdout);
    assert_eq!(
        clicked
            .stdout
            .lines()
            .filter(|line| *line == expected)
            .count(),
        1,
        "{}",
        clicked.stdout
    );

    for steps in ["click Submit", "click Submit\nexpect text \"Sign in\""] {
        tabref.ok(&["open", &url]);
        let batch = tabref.ok(&["--json", "do", steps]).json();
        let click = &batch["data"]["steps"][0];
        let text = click["text"].as_str().unwrap_or_default();
        assert!(text.starts_with("ok click"), "{batch}");
        assert_eq!(
            text.lines().filter(|line| *line == expected).count(),
            1,
            "{batch}"
        );
        assert_eq!(click["data"]["bot_check_provider"], "reCAPTCHA", "{batch}");
        assert_eq!(click["data"]["navigated"], false, "{batch}");
    }
}

fn reference<'a>(text: &'a str, control: &str) -> &'a str {
    text.split('[')
        .filter_map(|token| token.split_once(' '))
        .find_map(|(reference, label)| label.starts_with(control).then_some(reference))
        .unwrap_or_default()
}

fn assert_note(text: &str, provider: &str) {
    assert_eq!(
        text.lines().next().unwrap_or_default(),
        format!("{NOTE} ({provider})"),
        "{text}"
    );
    assert_eq!(text.matches(NOTE).count(), 1, "{text}");
}
