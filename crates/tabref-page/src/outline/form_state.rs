/// The live state of a form element and the frame link of an iframe, as
/// the DOM snapshot reports them: the current value of an input, textarea
/// or select, whether a checkbox or radio is checked, whether an option is
/// selected, the index of the document an in-process iframe shows, and
/// whether the node is a pseudo-element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormState {
    pub input_value: Option<String>,
    pub checked: bool,
    pub option_selected: bool,
    pub content_document: Option<usize>,
    pub pseudo: bool,
}
