use serde_json::json;
use tabref_page::{DomNode, DomTree, NodeKind, NodeLayout, PageInfo};

use super::{bot_check_view, challenge_url};
use crate::output::outcome::Outcome;

#[test]
fn challenge_urls_require_the_provider_host_and_challenge_path() {
    for (src, expected) in [
        (
            "https://www.google.com/recaptcha/api2/bframe?k=key",
            Some("reCAPTCHA"),
        ),
        (
            "//www.recaptcha.net/recaptcha/enterprise/bframe",
            Some("reCAPTCHA"),
        ),
        (
            "https://newassets.hcaptcha.com/captcha/v1/hcaptcha.html#frame=challenge",
            Some("hCaptcha"),
        ),
        ("https://www.google.com/recaptcha/api2/anchor", None),
        ("https://www.google.com.test/recaptcha/api2/bframe", None),
        ("https://hcaptcha.com/captcha/widget#frame=checkbox", None),
        (
            "https://fakehcaptcha.com/captcha/widget#frame=challenge",
            None,
        ),
    ] {
        assert_eq!(challenge_url(src), expected, "{src}");
    }
}

#[test]
fn bot_check_view_preserves_lines_and_refs_and_ignores_the_jsd_script() {
    let info = PageInfo {
        title: "Just a moment...".into(),
        url: "https://site.test/".into(),
    };
    let mut node = DomNode {
        parent: None,
        children: vec![],
        kind: NodeKind::Element("SCRIPT".into()),
        backend_id: 1,
        attrs: vec![(
            "src".into(),
            "/cdn-cgi/challenge-platform/scripts/jsd/main.js".into(),
        )],
        layout: None,
        form: Default::default(),
    };
    let mut document = DomTree {
        target_id: "tab".into(),
        frame_id: "page".into(),
        url: info.url.clone(),
        base_url: info.url.clone(),
        nodes: vec![node.clone()],
    };
    let normal = Outcome::new(
        "page\n[e1 button Log in]",
        json!({
            "snapshot_id": "s1", "ref_count": 1, "lines": ["[e1 button Log in]"]
        }),
    );
    let mut report = normal.clone();
    bot_check_view(&info, Some(&document), &mut report);
    assert_eq!(report, normal);

    node.kind = NodeKind::Element("IFRAME".into());
    node.attrs[0].1 = "https://www.google.com/recaptcha/api2/bframe".into();
    node.layout = Some(NodeLayout {
        display: "block".into(),
        position: "static".into(),
        visible: false,
        bounds: [0.0, 0.0, 200.0, 200.0],
    });
    document.nodes[0] = node;
    bot_check_view(&info, Some(&document), &mut report);
    assert_eq!(report, normal);
    document.nodes[0].layout.as_mut().unwrap().visible = true;
    bot_check_view(&info, Some(&document), &mut report);
    assert_eq!(
        report.text,
        format!("note: bot check on this page (reCAPTCHA)\n{}", normal.text)
    );
    assert_eq!(report.data["bot_check"], true);
    assert_eq!(report.data["bot_check_provider"], "reCAPTCHA");
    for key in ["snapshot_id", "ref_count", "lines"] {
        assert_eq!(report.data[key], normal.data[key]);
    }
}
