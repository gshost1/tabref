use serde::Deserialize;

/// A file input the page script found near the upload target: its place in
/// the script's list, how the result names it (`input#cv`,
/// `input.dz-hidden-input`), whether a person can see it, whether it takes
/// several files or none, and how it relates to the target.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct FileInputCandidate {
    pub(crate) index: usize,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) hidden: bool,
    #[serde(default)]
    pub(crate) multiple: bool,
    #[serde(default)]
    pub(crate) disabled: bool,
    #[serde(default, rename = "self")]
    pub(crate) itself: bool,
    #[serde(default)]
    pub(crate) labelled: bool,
    pub(crate) depth: Option<u32>,
}

/// Why the chosen input took the files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileInputVia {
    Itself,
    Label,
    Nearest,
    OnlyOnPage,
}

impl FileInputVia {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Itself => "the target itself",
            Self::Label => "the input its label controls",
            Self::Nearest => "the nearest file input",
            Self::OnlyOnPage => "the only file input on the page",
        }
    }
}

/// The outcome of the search: one input and why, none, or several equally
/// near.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileInputChoice<'a> {
    Chosen(&'a FileInputCandidate, FileInputVia),
    None,
    Ambiguous(Vec<&'a FileInputCandidate>),
}

/// Picks the input in order: the target itself, the input its label
/// controls, then the file inputs under the nearest ancestor that holds
/// any. One there is used; several there are refused. An ancestor that is
/// the whole document makes the one input found the only one on the page.
pub(crate) fn choose(
    candidates: &[FileInputCandidate],
    document_depth: Option<u32>,
) -> FileInputChoice<'_> {
    if let Some(found) = candidates.iter().find(|candidate| candidate.itself) {
        return FileInputChoice::Chosen(found, FileInputVia::Itself);
    }
    if let Some(found) = candidates.iter().find(|candidate| candidate.labelled) {
        return FileInputChoice::Chosen(found, FileInputVia::Label);
    }
    let Some(nearest) = candidates
        .iter()
        .filter_map(|candidate| candidate.depth)
        .min()
    else {
        return FileInputChoice::None;
    };
    let at: Vec<&FileInputCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.depth == Some(nearest))
        .collect();
    match at.as_slice() {
        [only] => {
            let via = if document_depth.is_some_and(|depth| nearest >= depth) {
                FileInputVia::OnlyOnPage
            } else {
                FileInputVia::Nearest
            };
            FileInputChoice::Chosen(only, via)
        }
        _ => FileInputChoice::Ambiguous(at),
    }
}

#[cfg(test)]
#[path = "file_input_choice_tests.rs"]
mod tests;
