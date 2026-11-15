use serde_json::json;

use super::add_warning;
use crate::output::outcome::Outcome;

#[test]
fn the_warning_leads_the_text_and_joins_the_json_data() {
    let mut outcome = Outcome::new("page Example", json!({ "url": "https://a.test/" }));
    add_warning(&mut outcome, "port 9333 is held by socat (pid 7)");

    assert_eq!(
        outcome.text,
        "warning: port 9333 is held by socat (pid 7)\npage Example"
    );
    assert_eq!(
        outcome.data["warnings"],
        json!(["port 9333 is held by socat (pid 7)"])
    );
    assert_eq!(outcome.data["url"], "https://a.test/");
}
