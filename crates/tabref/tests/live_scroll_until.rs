#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::lazy_site::{lazy_site, rows};
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
