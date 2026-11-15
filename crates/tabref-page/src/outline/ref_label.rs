/// What the accessibility snapshot says about an element that has a ref:
/// the ref as the view prints it, the role, the accessible name, the value,
/// the state tokens, and whether it takes a click of its own. The outline names a control by these, so a view
/// line and the ref it carries always agree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefLabel {
    pub ref_text: String,
    pub role: String,
    pub name: Option<String>,
    pub value: Option<String>,
    pub states: Vec<String>,
    pub clickable: bool,
}
