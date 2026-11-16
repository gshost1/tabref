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

#[test]
fn an_option_is_found_again_only_under_its_label() {
    assert!(OPTION_AT.starts_with("function(index, label)"));
    assert!(OPTION_AT.contains("return now === label ? option : null;"));
}

#[test]
fn a_read_only_field_is_not_typed_into_and_the_value_leaves_the_popup_out() {
    assert!(READ.contains("!el.readOnly && !el.disabled"));
    assert!(READ.contains("child !== popup && !['listbox', 'option'].includes(roleOf(child))"));
    assert!(READ.contains("list.filter(option => option.selected).map(option => option.label)"));
}
