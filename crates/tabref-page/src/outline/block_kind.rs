/// What a block of the page outline is. Landmarks carry their short name
/// (`main`, `nav`, `header`, `footer`, `aside`, `form`, `search`,
/// `region`); a heading carries its level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    Generic,
    Paragraph,
    Heading(u8),
    List,
    ListItem,
    Table,
    Row,
    Cell,
    HeaderCell,
    Landmark(String),
    Dialog,
    Details,
    Pre,
    Frame,
    Term,
    Description,
    Quote,
}

impl BlockKind {
    /// Kinds that never fold into the line of the block around them.
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            Self::Heading(_)
                | Self::List
                | Self::Table
                | Self::Landmark(_)
                | Self::Dialog
                | Self::Frame
                | Self::Pre
        )
    }
}
