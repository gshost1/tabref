use super::{FIELD, OPTION_AT, READ};

#[test]
fn every_script_is_one_function_with_the_popup_lookup() {
    for script in [READ, OPTION_AT, FIELD] {
        assert!(script.starts_with("function("), "{script}");
        assert!(script.trim_end().ends_with('}'), "{script}");
        assert!(script.contains("const options = !popup ? [] :"), "{script}");
        assert!(
            script.contains("Symbol.for('tabref.aria-select')"),
            "{script}"
        );
    }
    assert!(READ.contains("window[memo] = new Set(visible())"));
}
