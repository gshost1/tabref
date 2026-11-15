use tabref_page::{Block, Control, Item, squash};

use super::control_text::{control_reading, control_token};
use super::escape::{escape_href, escape_in_token, escape_text};
use super::view_options::ViewOptions;

const PROSE_OWN_WORDS: usize = 4;
const PROSE_ALL_WORDS: usize = 6;
const FOLDED_LINK_WORDS: usize = 3;

/// One output line being assembled from text runs, controls and the lines
/// of folded child blocks. Text keeps the spacing the page had, with its
/// brackets escaped so it cannot pass for a control; a control is set off
/// by a space. In prose a link of up to three words prints as `[e12 word]`,
/// its ref without its address, so a paragraph full of links stays a
/// sentence; longer links (titles), buttons and fields print whole.
pub(crate) struct LineText<'a> {
    out: String,
    hrefs: Vec<String>,
    options: &'a ViewOptions,
    prose: bool,
    after_control: bool,
    shortened: bool,
}

impl<'a> LineText<'a> {
    pub(crate) fn new(options: &'a ViewOptions, prose: bool) -> Self {
        Self {
            out: String::new(),
            hrefs: Vec::new(),
            options,
            prose,
            after_control: false,
            shortened: false,
        }
    }

    pub(crate) fn is_blank(&self) -> bool {
        self.out.trim().is_empty()
    }

    pub(crate) fn push_item(&mut self, item: &Item) {
        match item {
            Item::Text(run) => {
                if self.after_control && run.starts_with(char::is_alphanumeric) {
                    self.out.push(' ');
                }
                self.out.push_str(&escape_text(run));
                self.after_control = false;
            }
            Item::Break => {
                self.out.push_str(" / ");
                self.after_control = false;
            }
            Item::Control(control) => self.push_control(control),
            Item::Block(_) => {}
        }
    }

    pub(crate) fn push_control(&mut self, control: &Control) {
        let reading = self.options.reading() && !control.has_state("collapsed");
        if reading {
            self.push_word(&escape_text(&control_reading(control)));
            self.after_control = true;
            return;
        }
        let short = control.name.split_whitespace().count() <= FOLDED_LINK_WORDS;
        if self.prose && control.is_link() && short && !control.name.is_empty() {
            let token = prose_link(control, self.options);
            self.push_word(&token);
            self.after_control = true;
            return;
        }
        let repeated = control
            .href
            .as_ref()
            .is_some_and(|href| self.hrefs.contains(href));
        if repeated && control.ref_text.is_none() {
            return;
        }
        if let Some(href) = &control.href {
            self.hrefs.push(href.clone());
        }
        let (token, cut) = control_token(control, self.options, !repeated);
        self.shortened |= cut;
        self.push_word(&token);
        self.after_control = true;
    }

    /// Whether a control token on the line was cut.
    pub(crate) fn shortened(&self) -> bool {
        self.shortened
    }

    /// Appends `text` as its own word.
    pub(crate) fn push_word(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if !self.out.is_empty() && !self.out.ends_with([' ', '(', '/']) {
            self.out.push(' ');
        }
        self.out.push_str(text);
        self.after_control = false;
    }

    /// Appends a folded child line after `separator`.
    pub(crate) fn push_part(&mut self, separator: &str, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        if !self.is_blank() {
            self.out.push_str(separator);
        }
        self.out.push_str(text);
        self.after_control = false;
    }

    pub(crate) fn finish(self) -> String {
        squash(&self.out)
    }
}

/// A short link inside prose: `[e12 word]` with its ref, or, with no ref,
/// `[word -> /path]`. `TABREF_PROSE_LINKS=href` prints the address instead
/// of the ref, and `=word` the bare word, for measuring.
fn prose_link(control: &Control, options: &ViewOptions) -> String {
    let name = escape_in_token(&control.name);
    let href = control
        .href
        .as_deref()
        .map(|href| escape_href(&tabref_page::short_href(&options.page_url, href)));
    let style = std::env::var("TABREF_PROSE_LINKS").unwrap_or_default();
    match (style.as_str(), &control.ref_text, href) {
        ("word", _, _) => name,
        ("href", _, Some(href)) | (_, None, Some(href)) => format!("[{name} -> {href}]"),
        (_, Some(ref_text), _) => format!("[{} {name}]", options.refs.print(ref_text)),
        _ => name,
    }
}

/// Whether the content of `items` (and of the blocks inside them) reads as
/// prose: at least four words of its own text and six words in all.
pub(crate) fn is_prose(items: &[Item]) -> bool {
    let (mut own, mut links) = (0usize, 0usize);
    measure(items, &mut own, &mut links);
    own >= PROSE_OWN_WORDS && own + links >= PROSE_ALL_WORDS
}

fn measure(items: &[Item], own: &mut usize, links: &mut usize) {
    for item in items {
        match item {
            Item::Text(run) => *own += words(run),
            Item::Control(control) if control.is_link() => *links += words(&control.name),
            Item::Block(Block { items, .. }) => measure(items, own, links),
            _ => {}
        }
    }
}

fn words(text: &str) -> usize {
    text.split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count()
}

/// For each item, the text a run keeps once the name of the control just
/// before or after it is taken off its edge, as a `<label>` around a field
/// repeats it; `None` for an item that stays as it is.
pub(crate) fn label_echoes(items: &[Item]) -> Vec<Option<String>> {
    let name_at = |position: usize| match items.get(position) {
        Some(Item::Control(control)) if !control.name.trim().is_empty() => {
            Some(squash(&control.name))
        }
        _ => None,
    };
    items
        .iter()
        .enumerate()
        .map(|(position, item)| {
            let Item::Text(run) = item else {
                return None;
            };
            let mut text = squash(run);
            let before = position.checked_sub(1).and_then(name_at);
            let after = name_at(position + 1);
            let mut changed = false;
            if let Some(name) = before {
                if let Some(rest) = strip_prefix_ignoring_case(&text, &name) {
                    text = rest;
                    changed = true;
                }
            }
            if let Some(name) = after {
                if let Some(rest) = strip_suffix_ignoring_case(&text, &name) {
                    text = rest;
                    changed = true;
                }
            }
            changed.then(|| {
                format!(
                    " {} ",
                    text.trim_matches(|c: char| c.is_whitespace() || c == ':')
                )
            })
        })
        .collect()
}

fn strip_prefix_ignoring_case(text: &str, prefix: &str) -> Option<String> {
    let head = text.get(..prefix.len())?;
    let rest = &text[prefix.len()..];
    (head.eq_ignore_ascii_case(prefix) && (rest.is_empty() || rest.starts_with([' ', ':'])))
        .then(|| rest.to_owned())
}

fn strip_suffix_ignoring_case(text: &str, suffix: &str) -> Option<String> {
    let base = text.trim_end_matches(':').trim_end();
    let cut = base.len().checked_sub(suffix.len())?;
    let (kept, tail) = (base.get(..cut)?, base.get(cut..)?);
    (tail.eq_ignore_ascii_case(suffix) && (kept.is_empty() || kept.ends_with(' ')))
        .then(|| kept.to_owned())
}
