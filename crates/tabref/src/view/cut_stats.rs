/// What rendering left out before any budget applied: rows of long tables
/// and lists outside the row window, and the first row a `--rows` window
/// would need to show the next ones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CutStats {
    pub(crate) rows_cut: usize,
    pub(crate) next_row: Option<usize>,
}
