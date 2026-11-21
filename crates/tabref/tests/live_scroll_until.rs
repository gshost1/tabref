#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::lazy_site::{batch, lazy_site, rows};
use common::{harness, live};

const PAGE: &str = r#"<!doctype html><style>li{height:18px}</style><ol id=rows>ROWS</ol><p id=tail>More</p><script>
let busy=false, count=30;
new IntersectionObserver(entries=>{
 if (!entries[0].isIntersecting || busy || count>=810) return;
 busy=true;
 fetch('/entries?name=Entry&total=810&count=30&delay=20&from='+count).then(r=>r.json()).then(page=>{
  for(const text of page.items){const li=document.createElement('li');li.textContent=text;document.getElementById('rows').appendChild(li);count++;}
  busy=false;
 });
}).observe(document.getElementById('tail'));
</script>"#;

#[test]
fn a_do_scroll_until_prints_the_summary_and_indented_find_refs() {
    if !live() {
        return;
    }
    let page = PAGE.replace("ROWS", &rows("li", "Entry", 30)).replace(
        "li.textContent=text;",
        "const link=document.createElement('a');link.href='#'+count;link.textContent=text;li.appendChild(link);",
    );
    let base = lazy_site(vec![("/", page)]);
    let tabref = harness("do-scroll-until");
    tabref.ok(&["open", &base]);
    let run = batch(
        &tabref,
        "scroll down --until \"Entry 810\"\nscroll down --to-end\n",
    );
    assert!(run.ok, "{}\n{}", run.stdout, run.stderr);
    assert!(
        run.stdout.contains("ok found \"Entry 810\" after"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.lines().any(|line| line.starts_with("  ")
            && line.contains("Entry 810")
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
        .replace("ROWS", &rows("li", "Entry", 30))
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
        "scroll down in \"Entry 1\" --until \"Entry 810\"\nscroll down in \"Entry 1\" --to-end\n",
    );
    assert!(run.ok, "{}\n{}", run.stdout, run.stderr);
    assert!(
        run.stdout.contains("ok found \"Entry 810\" after"),
        "{}",
        run.stdout
    );
    assert!(run.stdout.contains("to the bottom"), "{}", run.stdout);
}

#[test]
fn one_command_reaches_a_late_lazy_row_and_returns_matches() {
    if !live() {
        return;
    }
    let base = lazy_site(vec![("/", PAGE.replace("ROWS", &rows("li", "Entry", 30)))]);
    let tabref = harness("scroll-until");
    tabref.ok(&["open", &base]);
    let run = tabref.ok(&["--json", "scroll", "down", "--until", "Entry 810"]);
    let data = run.json();
    assert_eq!(data["data"]["found"], true, "{data}");
    assert!(data["data"]["steps"].as_u64().unwrap() > 20, "{data}");
    tabref.ok(&["scroll", "--to-end"]);
    let run = tabref.ok(&["--json", "scroll", "--until", "Entry 810"]);
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
}
