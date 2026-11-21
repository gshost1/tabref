#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::lazy_site::{batch, lazy_site, rows};
use common::{harness, live};

const PAGE: &str = r#"<!doctype html><style>li{height:18px}</style><ol id=rows>ROWS</ol><p id=tail>More</p><script>
let busy=false, count=60;
new IntersectionObserver(entries=>{
 if (!entries[0].isIntersecting || busy || count>=240) return;
 busy=true;
 fetch('/entries?name=Entry&total=240&count=30&delay=20&from='+count).then(r=>r.json()).then(page=>{
  for(const text of page.items){const li=document.createElement('li');li.textContent=text;document.getElementById('rows').appendChild(li);count++;}
  busy=false;
 });
}).observe(document.getElementById('tail'));
</script>"#;

#[test]
fn cli_loop_flags_can_precede_the_direction_or_container() {
    if !live() {
        return;
    }
    let base = lazy_site(vec![(
        "/",
        PAGE.replace("240", "90")
            .replace("ROWS", &rows("li", "Entry", 60)),
    )]);
    let tabref = harness("scroll-cli-order");
    tabref.ok(&["open", &base]);
    for args in [
        vec!["scroll", "down", "--until", "Entry 90", "in", "Entry 1"],
        vec!["scroll", "--until", "Entry 1", "down"],
    ] {
        let run = tabref.run(&args);
        assert!(run.ok, "{}", run.stderr);
        assert!(run.stdout.contains("found"), "{}", run.stdout);
    }
}

#[test]
fn do_loop_flags_can_precede_the_direction_or_container() {
    if !live() {
        return;
    }
    let base = lazy_site(vec![(
        "/",
        PAGE.replace("240", "90")
            .replace("ROWS", &rows("li", "Entry", 60)),
    )]);
    let tabref = harness("scroll-do-order");
    tabref.ok(&["open", &base]);
    let run = batch(
        &tabref,
        "scroll down --until \"Entry 90\" in \"Entry 1\"\nscroll --until \"Entry 1\" down\n",
    );
    assert!(run.ok, "{}", run.stderr);
    assert_eq!(run.stdout.matches("ok found").count(), 2, "{}", run.stdout);
}

#[test]
fn a_slow_pending_fetch_never_claims_the_list_has_ended() {
    if !live() {
        return;
    }
    let page = PAGE
        .replace("240", "120")
        .replace("delay=20", "delay=10000")
        .replace("ROWS", &rows("li", "Entry", 60));
    let base = lazy_site(vec![("/", page)]);
    let tabref = harness("scroll-slow-end");
    for option in [vec!["scroll", "--until", "Entry 120"]] {
        tabref.ok(&["open", &base]);
        let first = tabref.ok(&["--json", "scroll", "down"]);
        assert_eq!(first.json()["data"]["loading"], true, "{}", first.stdout);
        std::thread::sleep(std::time::Duration::from_millis(3200));
        let run = tabref.ok(&option);
        assert!(!run.stdout.contains("bottom"), "{}", run.stdout);
        assert!(run.stdout.contains("more may load"), "{}", run.stdout);
    }
}

#[test]
fn until_ignores_closed_details_transparency_clipping_and_textarea_defaults() {
    if !live() {
        return;
    }
    for prefix in [
        "<details><summary>More</summary>Entry 90</details>",
        "<p style='opacity:0'>Entry 90</p>",
        "<p style='position:absolute;left:-9999px'>Entry 90</p>",
        "<p style='position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0)'>Entry 90</p>",
        "<textarea>Entry 90</textarea>",
    ] {
        ignores_non_rendered_match(prefix);
    }
}

fn ignores_non_rendered_match(prefix: &str) {
    let page = format!(
        "{prefix}{}",
        PAGE.replace("240", "90")
            .replace("ROWS", &rows("li", "Entry", 60))
    );
    let base = lazy_site(vec![("/", page)]);
    let tabref = harness("scroll-visible-text");
    tabref.ok(&["open", &base]);
    let run = tabref.ok(&["--json", "scroll", "--until", "Entry 90"]);
    let data = &run.json()["data"];
    assert!(data["steps"].as_u64().unwrap() > 0, "{data}");
    assert_eq!(data["found"], true, "{data}");
}

#[test]
fn until_ignores_the_document_title() {
    if live() {
        ignores_non_rendered_match("<title>Entry 90</title>");
    }
}

#[test]
fn until_ignores_display_none_text() {
    if live() {
        ignores_non_rendered_match("<div style=display:none>Entry 90</div>");
    }
}

#[test]
fn until_ignores_input_values() {
    if live() {
        ignores_non_rendered_match("<input value='Entry 90'>");
    }
}

#[test]
fn until_in_a_container_ignores_matching_text_outside_it() {
    if !live() {
        return;
    }
    let page = PAGE
        .replace("240", "90")
        .replace("ROWS", &rows("li", "Entry", 60))
        .replace(
            "<ol id=rows>",
            "<p>Entry 90</p><div style='height:320px;overflow:auto'><ol id=rows>",
        )
        .replace("<script>", "</div><script>");
    let base = lazy_site(vec![("/", page)]);
    let tabref = harness("scroll-container-text");
    tabref.ok(&["open", &base]);
    let run = tabref.ok(&[
        "--json", "scroll", "down", "in", "Entry 1", "--until", "Entry 90",
    ]);
    assert!(
        run.json()["data"]["steps"].as_u64().unwrap() > 0,
        "{}",
        run.stdout
    );
    assert_eq!(run.json()["data"]["found"], true);
}

#[test]
fn unquoted_until_values_explain_how_to_quote_them_in_cli_and_do() {
    if !live() {
        return;
    }
    let base = lazy_site(vec![("/", "<p>Start</p>".into())]);
    let tabref = harness("scroll-quote-value");
    tabref.ok(&["open", &base]);
    for run in [
        tabref.run(&["scroll", "down", "--until", "Entry", "810"]),
        batch(&tabref, "scroll down --until Entry 810\n"),
    ] {
        assert!(!run.ok, "{}", run.stdout);
        assert!(
            run.stderr
                .contains("extra value words must be one argument; quote the value"),
            "{}",
            run.stderr
        );
    }
}

#[test]
fn a_do_scroll_until_prints_the_summary_and_indented_find_refs() {
    if !live() {
        return;
    }
    let page = PAGE.replace("ROWS", &rows("li", "Entry", 60)).replace(
        "li.textContent=text;",
        "const link=document.createElement('a');link.href='#'+count;link.textContent=text;li.appendChild(link);",
    );
    let base = lazy_site(vec![("/", page)]);
    let tabref = harness("do-scroll-until");
    tabref.ok(&["open", &base]);
    let run = batch(
        &tabref,
        "scroll down --until \"Entry 240\"\nscroll down --to-end\n",
    );
    assert!(run.ok, "{}\n{}", run.stdout, run.stderr);
    assert!(
        run.stdout.contains("ok found \"Entry 240\" after"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.lines().any(|line| line.starts_with("  ")
            && line.contains("Entry 240")
            && line.contains("@s")),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("ok scrolled down"), "{}", run.stdout);
    assert!(!run.stdout.contains("\n+ "), "{}", run.stdout);
}

#[test]
fn a_do_scroll_loop_uses_the_container_holding_its_target() {
    if !live() {
        return;
    }
    let page = PAGE
        .replace("ROWS", &rows("li", "Entry", 60))
        .replace(
            "<ol id=rows>",
            "<div style='height:320px;overflow:auto'><ol id=rows>",
        )
        .replace("<script>", "</div><script>");
    let base = lazy_site(vec![("/", page)]);
    let tabref = harness("do-scroll-until-container");
    tabref.ok(&["open", &base]);
    let run = batch(
        &tabref,
        "scroll down in \"Entry 1\" --until \"Entry 240\"\nscroll down in \"Entry 1\" --to-end\n",
    );
    assert!(run.ok, "{}\n{}", run.stdout, run.stderr);
    assert!(
        run.stdout.contains("ok found \"Entry 240\" after"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("more may load"), "{}", run.stdout);
}

#[test]
fn one_command_reaches_a_late_lazy_row_and_returns_matches() {
    if !live() {
        return;
    }
    let base = lazy_site(vec![("/", PAGE.replace("ROWS", &rows("li", "Entry", 60)))]);
    let tabref = harness("scroll-until");
    tabref.ok(&["open", &base]);
    let run = tabref.ok(&["--json", "scroll", "down", "--until", "Entry 240"]);
    let data = run.json();
    assert_eq!(data["data"]["found"], true, "{data}");
    assert!(data["data"]["steps"].as_u64().unwrap() >= 4, "{data}");
    tabref.ok(&["scroll", "--to-end"]);
    let run = tabref.ok(&["--json", "scroll", "--until", "Entry 240"]);
    assert_eq!(run.json()["data"]["steps"], 0);
    let absent = tabref.ok(&["scroll", "--until", "Absent row"]);
    assert!(
        absent.stdout.contains("not on the page"),
        "{}",
        absent.stdout
    );
}

#[test]
fn shadow_text_matches_case_and_spacing_before_scrolling() {
    if !live() {
        return;
    }
    let base = lazy_site(vec![("/", r#"<div id=host></div><script>host.attachShadow({mode:'open'}).innerHTML='<p>ALAN <b>  Turing</b></p>';</script>"#.into())]);
    let tabref = harness("scroll-until-shadow");
    tabref.ok(&["open", &base]);
    let run = tabref.ok(&["--json", "scroll", "--until", "alan turing"]);
    assert_eq!(run.json()["data"]["steps"], 0);
    assert!(run.json()["data"]["count"].as_u64().unwrap() > 0);
    let invalid = tabref.run(&["scroll", "e1", "--to-end"]);
    assert!(!invalid.ok);
    assert!(invalid.stderr.contains("not a REF"), "{}", invalid.stderr);
}

#[test]
fn a_long_page_reports_the_step_cap_without_claiming_the_end() {
    if !live() {
        return;
    }
    let base = lazy_site(vec![(
        "/",
        "<div style='height:100000px'>Long page</div>".into(),
    )]);
    let tabref = harness("scroll-until-cap");
    tabref.ok(&["open", &base]);
    let run = tabref.ok(&["--json", "scroll", "--until", "Absent"]);
    assert_eq!(run.json()["data"]["steps"], 60);
    assert_eq!(run.json()["data"]["found"], false);
    assert_eq!(run.json()["data"]["cap"], "60-step cap");
    assert_eq!(run.json()["data"]["at_end"], false);
    tabref.ok(&["open", &base]);
    let run = tabref.ok(&["scroll", "--until", "Absent"]);
    assert!(
        run.stdout
            .contains("stopped at the 60-step cap; more may load"),
        "{}",
        run.stdout
    );
}
