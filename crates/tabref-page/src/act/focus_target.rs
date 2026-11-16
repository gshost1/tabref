use crate::ElementToken;

/// What the `focus` command focuses: the element, and whether the page
/// focuses it itself (`in_page`), as for a shadow host, whose control the
/// browser picks, where `DOM.focus` refuses the host.
#[derive(Debug, Clone)]
pub struct FocusTarget {
    pub token: ElementToken,
    pub in_page: bool,
}
