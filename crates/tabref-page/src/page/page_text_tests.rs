use serde_json::json;

use super::{page_text, squeeze_blank_lines};
use crate::CdpConnection;
use crate::act::act_fixture::{deadline, session};
use tabref_cdp::ScriptedTransport;

#[test]
fn blank_line_runs_collapse_and_trailing_spaces_go() {
    assert_eq!(
        squeeze_blank_lines("  Title  \n\n\n\nBody   \n\n  \nEnd\n\n"),
        "Title\n\nBody\n\nEnd"
    );
}

#[test]
fn long_text_is_cut_at_the_limit_and_says_so() {
    let transport = ScriptedTransport::new().expect(
        "Runtime.evaluate",
        json!({ "result": { "type": "string", "value": "abcdefghij" } }),
    );
    let mut connection = CdpConnection::new(transport.boxed());

    let (text, cut) = page_text(&mut connection, &session(), 4, deadline()).unwrap();

    assert_eq!(text, "abcd");
    assert!(cut);
}

#[test]
fn reading_the_text_is_not_a_user_gesture() {
    let transport = ScriptedTransport::new().expect(
        "Runtime.evaluate",
        json!({ "result": { "type": "string", "value": "hi" } }),
    );
    let sent = transport.sent_log();
    let mut connection = CdpConnection::new(transport.boxed());

    page_text(&mut connection, &session(), 100, deadline()).unwrap();

    assert_eq!(sent.lock().unwrap()[0]["params"]["userGesture"], false);
}
