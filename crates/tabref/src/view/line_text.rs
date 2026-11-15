use tabref_page::{Block, Control, Item, squash};

use super::control_text::{control_reading, control_text};
use super::view_options::ViewOptions;

const PROSE_OWN_WORDS: usize = 4;
const PROSE_ALL_WORDS: usize = 6;
const FOLDED_LINK_WORDS: usize = 3;

/// One output line being assembled from text runs, controls and the lines
/// of folded child blocks. Text keeps the spacing the page had; a control
/// is set off by a space. In prose (a line whose own text outweighs its
/// link text) links read as plain words, so a paragraph full of links stays
/// a sentence; buttons and fields keep their refs everywhere.
pub(crate) struct LineText<'a> {
    out: String,
    hrefs: Vec<String>,
    options: &'a ViewOptions,
    prose: bool,
    after_control: bool,
}

impl<'a> LineText<'a> {
    pub(crate) fn new(options: &'a ViewOptions, prose: bool) -> Self {
        Self {
            out: String::new(),
            hrefs: Vec::new(),
            options,
            prose,
            after_control: false,
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
                self.out.push_str(run);
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
        let reading = self.options.reading && !control.has_state("collapsed");
        let short = control.name.split_whitespace().count() <= FOLDED_LINK_WORDS;
        let as_word =
            reading || (self.prose && control.is_link() && short && !control.name.is_empty());
        if as_word {
            self.push_word(&control_reading(control));
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
        let token = control_text(control, self.options, !repeated);
        self.push_word(&token);
        self.after_control = true;
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

/// For each item, whether it is a text run that only repeats the name of
/// the control beside it, as a `<label>` around a field does.
pub(crate) fn label_echoes(items: &[Item]) -> Vec<bool> {
    let name_at = |position: usize| match items.get(position) {
        Some(Item::Control(control)) => Some(normalized(&control.name)),
        _ => None,
    };
    items
        .iter()
        .enumerate()
        .map(|(position, item)| {
            let Item::Text(run) = item else {
                return false;
            };
            let text = normalized(run);
            !text.is_empty()
                && (name_at(position + 1).as_deref() == Some(text.as_str())
                    || position.checked_sub(1).and_then(name_at).as_deref() == Some(text.as_str()))
        })
        .collect()
}

fn normalized(text: &str) -> String {
    squash(text).trim_end_matches(':').trim().to_lowercase()
}
