//! The role words a target may start with, and the snapshot roles each one
//! covers. A field is anything that takes typed or picked values.

const FIELD_ROLES: &[&str] = &[
    "textfield",
    "combobox",
    "datefield",
    "timefield",
    "incrementor",
    "slider",
    "colorwell",
];

const WORDS: &[(&str, &[&str])] = &[
    ("button", &["button", "menubutton", "disclosure"]),
    ("link", &["link"]),
    ("field", FIELD_ROLES),
    ("textbox", &["textfield"]),
    ("textfield", &["textfield"]),
    ("textarea", &["textfield"]),
    ("input", FIELD_ROLES),
    ("searchbox", &["textfield"]),
    ("checkbox", &["checkbox"]),
    ("radio", &["radiobutton"]),
    ("switch", &["switch"]),
    ("select", &["combobox", "listbox"]),
    ("combobox", &["combobox"]),
    ("dropdown", &["combobox", "listbox"]),
    ("listbox", &["listbox"]),
    ("option", &["option"]),
    ("tab", &["tab"]),
    ("menuitem", &["menuitem"]),
    ("heading", &["heading"]),
    ("row", &["row"]),
    ("cell", &["cell"]),
    ("slider", &["slider"]),
    ("image", &["image"]),
    ("summary", &["disclosure"]),
    ("disclosure", &["disclosure"]),
    ("date", &["datefield", "textfield"]),
];

/// The snapshot roles a role word covers, or `None` when `word` is no role
/// word.
pub(crate) fn role_word(word: &str) -> Option<&'static [&'static str]> {
    let lowered = word.to_ascii_lowercase();
    WORDS
        .iter()
        .find(|(name, _)| *name == lowered)
        .map(|(_, roles)| *roles)
}

/// The roles a field-taking action (fill) reaches.
pub(crate) fn field_roles() -> &'static [&'static str] {
    FIELD_ROLES
}
