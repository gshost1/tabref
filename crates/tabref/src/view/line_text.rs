use tabref_page::{Block, Control, Item, squash};

use super::control_text::{control_reading, control_text};
use super::view_options::ViewOptions;

const PROSE_MIN_TEXT: usize = 40;

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
}

impl<'a> LineText<'a> {
    pub(crate) fn new(options: &'a ViewOptions, prose: bool) -> Self {
        Self {
            out: String::new(),
            hrefs: Vec::new(),
            options,
            prose,
        }
    }

    pub(crate) fn is_blank(&self) -> bool {
        self.out.trim().is_empty()
    }

    pub(crate) fn push_item(&mut self, item: &Item) {
        match item {
            Item::Text(run) => self.out.push_str(run),
            Item::Break => self.out.push_str(" / "),
            Item::Control(control) => self.push_control(control),
            Item::Block(_) => {}
        }
    }

    pub(crate) fn push_control(&mut self, control: &Control) {
        let reading = self.options.reading && !control.has_state("collapsed");
        let as_word = reading || (self.prose && control.is_link() && !control.name.is_empty());
        if as_word {
            self.push_word(&control_reading(control));
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
    }

    pub(crate) fn finish(self) -> String {
        squash(&self.out)
    }
}

/// Whether the content of `items` (and of the blocks inside them) reads as
/// prose: at least 40 characters of its own text, and more of it than of
/// link names.
pub(crate) fn is_prose(items: &[Item]) -> bool {
    let (mut text, mut links) = (0usize, 0usize);
    measure(items, &mut text, &mut links);
    text >= PROSE_MIN_TEXT && text >= links
}

fn measure(items: &[Item], text: &mut usize, links: &mut usize) {
    for item in items {
        match item {
            Item::Text(run) => *text += run.trim().chars().count(),
            Item::Control(control) if control.is_link() => *links += control.name.chars().count(),
            Item::Block(Block { items, .. }) => measure(items, text, links),
            _ => {}
        }
    }
}
