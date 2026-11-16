/// What a scroll did: whether its target moved, whether the scroll came
/// near enough to the end of the content that what it set loading was
/// watched, whether the target sits at its very end now, the items the
/// page added meanwhile (their count, the first line of the first and the
/// last, their lines as far as 600 characters go and how many more there
/// were), and whether a request it started was still running when the
/// wait ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScrollOutcome {
    pub moved: bool,
    pub watched: bool,
    pub at_end: bool,
    pub items: u64,
    pub first: Option<String>,
    pub last: Option<String>,
    pub lines: Vec<String>,
    pub more_lines: u64,
    pub loading: bool,
}
