/// The conditions `wait` was given as flags, of which exactly one may be
/// set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WaitFlags<'a> {
    pub(crate) text: Option<&'a str>,
    pub(crate) gone: Option<&'a str>,
    pub(crate) url: Option<&'a str>,
    pub(crate) idle: bool,
    pub(crate) download: bool,
    pub(crate) target: Option<&'a str>,
    pub(crate) state: Option<&'a str>,
}
