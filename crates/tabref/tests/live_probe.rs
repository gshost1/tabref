//! Live regressions from the second review of the page view: a consent
//! wall in a cross-site frame, a sticky purchase bar, a fixed full-screen
//! reader shell, `display: contents`, and content hidden with
//! `hidden=until-found` and `content-visibility: hidden`. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_probe`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, serve};

const CONSENT_PAGE: &str = r#"<!doctype html><html><head><title>News</title></head><body>
<h1>Headline</h1><p>The council approved the new bridge on Tuesday after a long debate.</p>
<script>document.write('<iframe style="position:fixed;inset:0;width:100vw;height:100vh;border:0" src="http://localhost:' + location.port + '/cmp"></iframe>')</script>
</body></html>"#;
const CMP: &str = r#"<!doctype html><html><body><h2>We value your privacy</h2><p>We and our 812 partners store and access information on your device.</p><button>Accept all</button><button>Reject all</button></body></html>"#;

const STICKY: &str = r#"<!doctype html><html><head><title>Product</title></head><body>
<main><h1>Trail running shoe</h1><p>Light, grippy, and made for long days on rocky ground.</p>
<p>Price $129.00</p><button>Add to cart</button></main>
<div style="position:fixed;left:0;right:0;bottom:0;height:90px;background:#fff">Trail running shoe, Size 42, Colour Blue — $129.00 <button>Add to cart</button></div>
</body></html>"#;

const CONTENTS: &str = r#"<!doctype html><html><head><title>Contents</title></head><body>
<div style="display:contents"><ul><li>Alpha</li><li>Beta</li></ul><button>Save settings</button></div>
<section hidden="until-found"><p>Secret appendix about zebras.</p></section>
<div style="content-visibility:hidden"><p>Skipped content about llamas.</p></div>
</body></html>"#;

const SHELL: &str = r#"<!doctype html><html><head><title>Reader</title></head><body>
<div style="position:fixed;inset:0;overflow:auto"><article><h1>Long read</h1>
<p>First paragraph of the article, which goes on for a good while about many things in detail.</p>
<p>Second paragraph continues the story with more facts and figures that matter to the reader a lot.</p>
<p>Third paragraph: the key fact is that the bridge opens on 12 May 2027.</p>
<button>Share</button><button>Save</button></article></div>
</body></html>"#;

#[test]
fn probe_live_views() {
    if !live() {
        return;
    }
    let port = serve(&[
        ("/consent", CONSENT_PAGE),
        ("/cmp", CMP),
        ("/sticky", STICKY),
        ("/contents", CONTENTS),
        ("/shell", SHELL),
    ]);
    let tabref = harness("probe");
    let base = format!("http://127.0.0.1:{port}");
    let open = |page: &str| tabref.ok(&["open", &format!("{base}/{page}")]).stdout;

    let consent = open("consent");
    let first = consent.lines().nth(1).unwrap();
    assert!(
        first.starts_with("dialog \"We value your privacy\""),
        "{consent}"
    );
    assert!(
        consent.contains("button Accept all]") && consent.contains("button Reject all]"),
        "{consent}"
    );
    assert!(
        consent.contains("The council approved the new bridge"),
        "{consent}"
    );
    let read = tabref.ok(&["read"]).stdout;
    assert!(read.contains("We and our 812 partners store"), "{read}");
    assert!(
        read.contains("The council approved the new bridge"),
        "{read}"
    );

    let sticky = open("sticky");
    assert!(
        !sticky.lines().any(|line| line.starts_with("dialog")),
        "{sticky}"
    );
    assert_eq!(sticky.matches("button Add to cart]").count(), 2, "{sticky}");

    let shell = open("shell");
    assert!(
        shell.contains("the bridge opens on 12 May 2027."),
        "{shell}"
    );
    assert!(
        shell.lines().last().unwrap().ends_with("all shown"),
        "{shell}"
    );

    let contents = open("contents");
    assert!(
        contents.contains("Alpha") && contents.contains("button Save settings]"),
        "{contents}"
    );
    assert!(
        !contents.contains("zebras") && !contents.contains("llamas"),
        "{contents}"
    );
    for needle in ["zebras", "llamas"] {
        let found = tabref.ok(&["find", needle]).stdout;
        assert!(
            found.starts_with(&format!("1 match for \"{needle}\" (1 hidden)")),
            "{found}"
        );
    }
}
