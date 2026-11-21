#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{harness, live, serve};

#[test]
fn remount_and_watch_reset_do_not_claim_popup_closure() {
    if !live() {
        return;
    }
    let port = serve(&[(
        "/",
        r#"<!doctype html>
<ul role=menu aria-label=Other id=other><li role=menuitem>Other action</li></ul>
<ul role=listbox aria-label=Category><li role=option>Alpha</li><li role=option>Beta</li><li role=option>Gamma</li></ul>
<button onclick="const list=document.querySelector('[role=listbox]');list.replaceWith(list.cloneNode(true))">Remount</button>
<button onclick="document.getElementById('other').remove();delete window[Symbol.for('tabref.watch')];const list=document.querySelector('[role=listbox]');list.replaceWith(list.cloneNode(true))">Reset watch</button>"#,
    )]);
    let tabref = harness("popup-remount-review");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    for action in ["Remount", "Reset watch"] {
        let text = tabref.ok(&["click", action]).stdout;
        assert!(!text.contains("listbox \"Category\" closed"), "{text}");
        assert!(!text.contains("+ Alpha"), "{text}");
    }
}

#[test]
fn a_toast_popover_is_not_a_closed_menu() {
    if !live() {
        return;
    }
    let port = serve(&[(
        "/",
        r#"<!doctype html><div popover=manual id=toast>Saved</div>
<button onclick="document.getElementById('toast').hidePopover()">Dismiss toast</button>
<script>document.getElementById('toast').showPopover()</script>"#,
    )]);
    let tabref = harness("toast-review");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Dismiss toast"]).stdout;
    assert!(text.contains("- Saved"), "{text}");
    assert!(!text.contains("closed"), "{text}");
}

#[test]
fn redrawn_rows_do_not_use_the_new_control_budget_or_hide_capped_names() {
    if !live() {
        return;
    }
    let port = serve(&[(
        "/",
        r#"<!doctype html>
<button onclick="draw(true);document.getElementById('choices').hidden=false">Rebuild</button>
<ul id=rows></ul><ul id=choices role=listbox aria-label=Choices hidden></ul>
<script>
function draw(reverse) {
  const names=Array.from({length:50},(_,n)=>'R'+String(n).padStart(2,'0'));
  if(reverse)names.reverse();
  document.getElementById('rows').innerHTML=names.map(name=>'<li><button>'+name+'</button></li>').join('');
}
draw(false);
document.getElementById('choices').innerHTML=Array.from({length:9},(_,n)=>'<li role=option>Choice '+n+'</li>').join('');
</script>"#,
    )]);
    let tabref = harness("redrawn-cap-review");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/")]);
    let text = tabref.ok(&["click", "Rebuild"]).stdout;
    assert!(text.contains("50 new refs"), "{text}");
    assert!(text.contains("option \"Choice 6\""), "{text}");
    assert!(text.contains("+ Choice 8"), "{text}");
    assert!(!text.contains("+ Choice 0"), "{text}");
    assert_eq!(
        text.lines().filter(|line| line.starts_with("new ")).count(),
        8,
        "{text}"
    );
}
