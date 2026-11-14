use serde_json::json;
use tabref_core::state::is_known_state;

use super::states_of;
use crate::tree::cdp_ax_node::CdpAxNode;

fn node(properties: serde_json::Value) -> CdpAxNode {
    serde_json::from_value(json!({ "nodeId": "1", "properties": properties })).unwrap()
}

#[test]
fn positive_properties_become_core_states() {
    let parsed = node(json!([
        { "name": "focused", "value": { "type": "booleanOrUndefined", "value": true } },
        { "name": "checked", "value": { "type": "tristate", "value": "true" } },
        { "name": "expanded", "value": { "type": "booleanOrUndefined", "value": true } },
        { "name": "selected", "value": { "type": "booleanOrUndefined", "value": true } },
        { "name": "disabled", "value": { "type": "boolean", "value": true } },
        { "name": "required", "value": { "type": "boolean", "value": true } },
        { "name": "invalid", "value": { "type": "token", "value": "spelling" } },
        { "name": "hasPopup", "value": { "type": "token", "value": "listbox" } },
    ]));
    let states = states_of(&parsed);
    assert!(
        states.iter().all(|state| is_known_state(state)),
        "{states:?}"
    );
    assert_eq!(
        states,
        [
            "focused", "disabled", "expanded", "checked", "selected", "required", "invalid",
            "haspopup"
        ]
    );
}

#[test]
fn negative_and_mixed_values_are_read_exactly() {
    let parsed = node(json!([
        { "name": "focused", "value": { "type": "booleanOrUndefined", "value": false } },
        { "name": "checked", "value": { "type": "tristate", "value": "mixed" } },
        { "name": "invalid", "value": { "type": "token", "value": "false" } },
        { "name": "pressed", "value": { "type": "tristate", "value": "false" } },
    ]));
    assert_eq!(states_of(&parsed), ["indeterminate"]);
    assert!(states_of(&node(json!([]))).is_empty());
}
