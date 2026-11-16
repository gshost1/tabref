//! Live runs of what an action reports when its effect lands inside an
//! open shadow root or a same-origin frame, against headless Chrome and
//! local fixture pages, and of what it says when the effect may sit where
//! the change check cannot read. Opt-in:
//! `TABREF_LIVE=1 cargo test -p tabref --test live_act_frames`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Harness, harness, live, serve};

const NOTHING: &str = "(no visible change)";
const UNCHECKED_FRAME: &str = "(no visible change; a frame from another site was not checked)";

const SHADOW: &str = r#"<!doctype html><title>Panel</title><h1>Account</h1><x-card></x-card>
<button>Noop</button>
<script>
customElements.define('x-card', class extends HTMLElement { connectedCallback() {
  const root = this.attachShadow({ mode: 'open' });
  root.innerHTML = '<style>p { color: teal }</style><button id=keep>Keep address</button><p id=state>Address not kept</p>';
  root.getElementById('keep').addEventListener('click', () => {
    root.getElementById('state').textContent = 'Address kept for billing';
  });
}});
</script>"#;

const FRAMED: &str = r#"<!doctype html><title>Ticket</title><h1>Ticket 7</h1>
<button onclick="frames[0].document.getElementById('log').textContent = 'Escalated to tier two'">Escalate</button>
<button onclick="frames[0].document.querySelector('x-badge').shadowRoot.getElementById('mark').textContent = 'Priority high'">Raise</button>
<button>Noop</button>
<iframe src="/composer" title="Composer" style="width:600px;height:200px"></iframe>"#;

const COMPOSER: &str = r#"<!doctype html><title>Composer</title><p id=log>No activity</p>
<x-badge></x-badge>
<form action="/composer-sent"><label>Note <input name=note></label><button>Send note</button></form>
<script>
customElements.define('x-badge', class extends HTMLElement { connectedCallback() {
  this.attachShadow({ mode: 'open' }).innerHTML = '<b id=mark>Priority normal</b>';
}});
</script>"#;

const COMPOSER_SENT: &str =
    "<!doctype html><title>Composer</title><p>Note sent to the customer</p>";

const REMOTE: &str = r#"<!doctype html><title>Remote</title><p id=out>Widget idle</p>
<script>addEventListener('message', () => { document.getElementById('out').textContent = 'Widget changed'; });</script>"#;

fn site() -> (Harness, String) {
    let port = serve(&[
        ("/shadow", SHADOW),
        ("/framed", FRAMED),
        ("/composer", COMPOSER),
        ("/composer-sent", COMPOSER_SENT),
    ]);
    (harness("act-frames"), format!("http://127.0.0.1:{port}"))
}

#[test]
fn a_click_whose_effect_lands_in_an_open_shadow_root_reports_the_change() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/shadow")]);
    let kept = tabref.ok(&["click", "Keep address"]);
    assert!(!kept.stdout.contains(NOTHING), "{}", kept.stdout);
    assert!(kept.stdout.contains("kept for billing"), "{}", kept.stdout);
    assert!(!kept.stdout.contains("color: teal"), "{}", kept.stdout);
    let noop = tabref.ok(&["click", "Noop"]);
    assert_eq!(noop.stdout.trim(), format!("clicked \"Noop\" {NOTHING}"));
}

#[test]
fn a_click_whose_effect_lands_in_a_same_origin_frame_reports_the_change() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/framed")]);
    let escalated = tabref.ok(&["click", "Escalate"]);
    assert!(!escalated.stdout.contains(NOTHING), "{}", escalated.stdout);
    assert!(
        escalated.stdout.contains("Escalated to tier two"),
        "{}",
        escalated.stdout
    );
    let raised = tabref.ok(&["click", "Raise"]);
    assert!(raised.stdout.contains("Priority high"), "{}", raised.stdout);
    let noop = tabref.ok(&["click", "Noop"]);
    assert_eq!(noop.stdout.trim(), format!("clicked \"Noop\" {NOTHING}"));
}

#[test]
fn a_form_sent_inside_a_frame_reports_the_frame_s_new_page() {
    if !live() {
        return;
    }
    let (tabref, base) = site();
    tabref.ok(&["open", &format!("{base}/framed")]);
    let run = tabref.ok(&["do", "fill Note \"call back\"; click \"Send note\""]);
    assert!(!run.stdout.contains(NOTHING), "{}", run.stdout);
    assert!(
        run.stdout.contains("Note sent to the customer"),
        "{}",
        run.stdout
    );
}

#[test]
fn a_change_the_check_cannot_read_says_so_instead_of_no_visible_change() {
    if !live() {
        return;
    }
    let remote = serve(&[("/remote", REMOTE)]);
    let page = format!(
        r#"<!doctype html><title>Embed</title><h1>Embed</h1>
<button onclick="frames[0].postMessage('go', '*')">Poke widget</button>
<iframe src="http://localhost:{remote}/remote" title="Widget" style="width:400px;height:120px"></iframe>"#
    );
    let port = serve(&[("/embed", page.as_str())]);
    let tabref = harness("act-frames-remote");
    tabref.ok(&["open", &format!("http://127.0.0.1:{port}/embed")]);
    let poked = tabref.ok(&["click", "Poke widget"]);
    assert_eq!(
        poked.stdout.trim(),
        format!("clicked \"Poke widget\" {UNCHECKED_FRAME}")
    );
}
