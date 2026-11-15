/// How much of each line a view keeps. `Snap` cuts long paragraphs,
/// `<pre>` blocks, dialog text and option lists, each with a marker that
/// names the command showing the rest; `Full` keeps every line whole with
/// the same line numbering; `Read` is reading text, whole, with links and
/// buttons as plain words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Detail {
    Snap,
    Full,
    Read,
}
