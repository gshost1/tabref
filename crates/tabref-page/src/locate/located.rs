use crate::ElementToken;

/// The element a [`Locator`](super::locator::Locator) resolved to: the token
/// to act on, the role and name it showed, its ref when the tree it was
/// found in carried refs, and the row or region around it, when that has a
/// name of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub token: ElementToken,
    pub role: String,
    pub name: Option<String>,
    pub ref_id: Option<String>,
    pub context: Option<String>,
}
