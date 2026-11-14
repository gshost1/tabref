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
