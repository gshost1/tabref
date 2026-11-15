/// What a DOM node is: an element with its upper-case tag name, a text
/// node with its raw text, a document, or anything else (comments, doctype).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Element(String),
    Text(String),
    Document,
    Other,
}
