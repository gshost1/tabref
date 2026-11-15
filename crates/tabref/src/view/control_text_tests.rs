use tabref_page::{Control, SelectOption};

use super::{control_reading, control_text};
use crate::view::ref_style::RefStyle;
use crate::view::row_window::RowWindow;
use crate::view::view_options::ViewOptions;

fn options(refs: RefStyle) -> ViewOptions {
    ViewOptions {
        refs,
        ..ViewOptions::snap("http://127.0.0.1:8792/admin/", false, RowWindow::default())
    }
}

fn control(role: &str, name: &str) -> Control {
    Control {
        ref_text: Some("@s8f3k2p9:e4".into()),
        role: role.into(),
        name: name.into(),
        ..Control::default()
    }
}

#[test]
fn a_link_prints_its_ref_name_and_short_address() {
    let mut link = control("link", "Edit");
    link.href = Some("http://127.0.0.1:8792/admin/catalog/product/4499/edit".into());
    assert_eq!(
        control_text(&link, &options(RefStyle::Short), true),
        "[e4 Edit -> /admin/catalog/product/4499/edit]"
    );
    assert_eq!(
        control_text(&link, &options(RefStyle::Long), false),
        "[@s8f3k2p9:e4 Edit]"
    );
    assert_eq!(control_reading(&link), "Edit");
}

#[test]
fn fields_print_role_value_options_and_states() {
    let mut field = control("textfield", "Price ($)");
    field.value = Some("67.00".into());
    assert_eq!(
        control_text(&field, &options(RefStyle::Short), true),
        "[e4 textbox Price ($) = \"67.00\"]"
    );
    let mut select = control("combobox", "Assignee");
    select.value = Some("Ada Salazar (@asalazar)".into());
    select.options = vec![
        SelectOption {
            label: "Unassigned".into(),
            value: String::new(),
            selected: false,
        },
        SelectOption {
            label: "Ada Salazar (@asalazar)".into(),
            value: "asalazar".into(),
            selected: true,
        },
        SelectOption {
            label: "Bram".into(),
            value: "b7".into(),
            selected: false,
        },
    ];
    assert_eq!(
        control_text(&select, &options(RefStyle::Short), true),
        "[e4 select Assignee = \"Ada Salazar (@asalazar)\" options: Unassigned | Ada Salazar (@asalazar) | Bram (b7)]"
    );
    let mut toggle = control("button", "Economics");
    toggle.states = vec!["collapsed".into()];
    assert_eq!(
        control_text(&toggle, &options(RefStyle::Short), true),
        "[e4 button Economics (collapsed)]"
    );
}

#[test]
fn a_long_cross_site_address_drops_its_query() {
    let mut share = control("link", "Share");
    share.href = Some(format!("https://social.example/share?u={}", "x".repeat(90)));
    assert_eq!(
        control_text(&share, &options(RefStyle::Short), true),
        "[e4 Share -> social.example/share?…]"
    );
}
