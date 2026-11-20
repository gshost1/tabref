use serde::{Deserialize, Deserializer};

const NAME_LIMIT: usize = 40;

/// A file input the page script found near the upload target: its place in
/// the script's list, how the result names it (`input#cv`,
/// `input.dz-hidden-input`), whether a person can see it, whether it takes
/// several files or none, how it relates to the target, and whether it
/// lies across a form or widget boundary from it. The page writes the
/// name, so only [`safe_name`]'s characters of it are kept.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct FileInputCandidate {
    pub(crate) index: usize,
    #[serde(deserialize_with = "page_name")]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) selector: Option<String>,
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
    #[serde(default)]
    pub(crate) crosses: bool,
}

fn page_name<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    String::deserialize(deserializer).map(|name| safe_name(&name))
}

/// The page's name for an input cut to what a printed line can carry: ASCII
/// letters, digits and `_ . # = [ ] -` only, at most 40 of them, so a page
/// cannot add a line break, a terminal escape or a forged result line.
pub(crate) fn safe_name(name: &str) -> String {
    let kept: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "_.#=[]-".contains(*c))
        .collect();
    if kept.is_empty() {
        return "input[type=file]".to_owned();
    }
    if kept.len() <= NAME_LIMIT {
        return kept;
    }
    format!("{}...", &kept[..NAME_LIMIT])
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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Itself => "target",
            Self::Label => "label",
            Self::Nearest => "nearest",
            Self::OnlyOnPage => "only",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Itself => "the target itself",
            Self::Label => "the input its label controls",
            Self::Nearest => "the nearest file input",
            Self::OnlyOnPage => "the only file input on the page",
        }
    }
}

/// The outcome of the search: one input and why, none, several equally
/// near, or one nearest input that belongs to another form or widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FileInputChoice<'a> {
    Chosen(&'a FileInputCandidate, FileInputVia),
    None,
    Ambiguous(Vec<&'a FileInputCandidate>),
    Fenced(&'a FileInputCandidate),
    OutsideWidget(Vec<&'a FileInputCandidate>),
}

/// Picks the input in order: the target itself, the input its label
/// controls, then the file inputs under the nearest ancestor that holds
/// any eligible input. A form target only considers inputs owned by that
/// form, including inputs associated with it through a `form` attribute.
/// Shadow-root boundaries also apply. A formless target cannot reach
/// unrelated widgets through the document body.
pub(crate) fn choose(
    candidates: &[FileInputCandidate],
    document_depth: Option<u32>,
    has_form: bool,
) -> FileInputChoice<'_> {
    if let Some(found) = candidates.iter().find(|candidate| candidate.itself) {
        return FileInputChoice::Chosen(found, FileInputVia::Itself);
    }
    if let Some(found) = candidates.iter().find(|candidate| candidate.labelled) {
        if has_form && found.crosses {
            return FileInputChoice::Fenced(found);
        }
        return FileInputChoice::Chosen(found, FileInputVia::Label);
    }
    let rank =
        |input: &FileInputCandidate| input.depth.map(|depth| (has_form && input.crosses, depth));
    let Some((outside_form, nearest)) = candidates.iter().filter_map(rank).min() else {
        return FileInputChoice::None;
    };
    let at: Vec<&FileInputCandidate> = candidates
        .iter()
        .filter(|candidate| rank(candidate) == Some((outside_form, nearest)))
        .collect();
    let at_document = document_depth.is_some_and(|depth| nearest >= depth);
    if at_document && !has_form {
        return FileInputChoice::OutsideWidget(at);
    }
    match at.as_slice() {
        [only] if only.crosses => FileInputChoice::Fenced(only),
        [only] => {
            let via = if at_document {
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
