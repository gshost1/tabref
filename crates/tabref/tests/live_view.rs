//! Golden page views: `open`, `snap`, `read` and `find` on fixture pages in
//! headless Chrome, compared with `tests/golden/*.txt`. Refs and the
//! fixture port are normalised. Opt-in: `TABREF_LIVE=1 cargo test -p tabref
//! --test live_view`; `TABREF_BLESS=1` rewrites the golden files.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;

use common::{Harness, harness, live, serve};

const DEFINITIONS: &str = include_str!("fixtures/view/definitions.html");
const MODAL: &str = include_str!("fixtures/view/modal.html");
const HIDDEN: &str = include_str!("fixtures/view/hidden.html");

const FRAMES: &str = r#"<!doctype html><html><head><title>Frames</title></head><body>
<h1>Story</h1><p>The council approved the new bridge on Tuesday.</p>
<iframe width="1" height="1" srcdoc="<p>pixel</p>"></iframe>
<iframe width="400" height="120" srcdoc="<p>Poll: should the bridge have a cycle lane?</p><button>Yes</button>"></iframe>
<script>document.write('<iframe width="300" height="250" src="http://localhost:' + location.port + '/ad"></iframe>')</script>
</body></html>"#;

const AD: &str = r#"<!doctype html><html><body><a href="https://ads.example/click">Buy shoes now</a></body></html>"#;

fn long_table() -> String {
    let rows: String = (1..=30)
        .map(|row| {
            format!(
                "<tr><td>{}</td><td>Item {row}</td><td>${row}.00</td><td><a href=/item/{}/edit>Edit</a></td></tr>",
                1000 + row,
                1000 + row
            )
        })
        .collect();
    format!(
        "<!doctype html><html><head><title>Items</title></head><body><h1>Items</h1><p>30 records found</p>\
         <table><tr><th>ID</th><th>Name</th><th>Price</th><th>Action</th></tr>{rows}</table></body></html>"
    )
}

fn site() -> (Harness, String, u16) {
    let table = long_table();
    let port = serve(&[
        ("/definitions", DEFINITIONS),
        ("/modal", MODAL),
        ("/hidden", HIDDEN),
        ("/frames", FRAMES),
        ("/ad", AD),
        ("/table", Box::leak(table.into_boxed_str())),
    ]);
    (harness("view"), format!("http://127.0.0.1:{port}"), port)
}

/// `@s8f3k2p9:e4` becomes `e4`, and the fixture port `PORT`.
fn normalize(text: &str, port: u16) -> String {
    let text = text.replace(&port.to_string(), "PORT");
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find("@s") {
        out.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let id_len = after
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .count();
        if after[id_len..].starts_with(':') && id_len > 0 {
            rest = &after[id_len + 1..];
        } else {
            out.push_str("@s");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn golden(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"));
    if std::env::var_os("TABREF_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        expected, actual,
        "golden {name} differs; rerun with TABREF_BLESS=1 to accept"
    );
}

#[test]
fn page_views_match_their_golden_files() {
    if !live() {
        return;
    }
    let (tabref, base, port) = site();
    let view = |args: &[&str]| normalize(&tabref.ok(args).stdout, port);

    let definitions = view(&["open", &format!("{base}/definitions")]);
    assert!(
        definitions.contains("(1/2) ") && definitions.contains("(2/2) "),
        "{definitions}"
    );
    golden("definitions-open", &definitions);
    golden("definitions-read", &view(&["read"]));

    let table = view(&["open", &format!("{base}/table")]);
    assert!(
        table.contains("table 30 rows: ID | Name | Price | Action"),
        "{table}"
    );
    assert!(table.contains("tabref snap --rows 21-"), "{table}");
    golden("table-open", &table);
    golden("table-rows", &view(&["snap", "--rows", "21-"]));

    let modal = view(&["open", &format!("{base}/modal")]);
    assert!(modal.starts_with("page \"News front page\""), "{modal}");
    golden("modal-open", &modal);

    let frames = view(&["open", &format!("{base}/frames")]);
    assert!(frames.contains("frames: "), "{frames}");
    golden("frames-open", &frames);

    tabref.ok(&["open", &format!("{base}/hidden")]);
    let found = view(&["find", "econ."]);
    assert!(found.starts_with("5 matches for \"econ.\""), "{found}");
    golden("hidden-find", &found);
}
