use std::collections::HashMap;

use tabref_page::{Block, BlockKind, Item, cut_chars};

use super::cut_stats::CutStats;
use super::line_text::{LineText, is_prose};
use super::view_line::ViewLine;
use super::view_options::ViewOptions;

/// The longest line a block with nested blocks folds into.
pub(crate) const LINE_MAX: usize = 240;
const PROSE_MAX: usize = 700;
const READ_PROSE_MAX: usize = 4_000;
const MAX_PARTS: usize = 12;
const PRE_LINES: usize = 40;
const CHROME_LANDMARKS: &[&str] = &["header", "nav", "footer", "aside"];

/// Turns an outline into view lines. A block whose content fits one line
/// prints as one line; anything larger prints its parts in order. Siblings
/// that share a shape key are marked: a one-line repeat as `- `, a larger
/// one as `(k/N) ` with its other lines indented, so a reader can count
/// repeated sections without reading them all.
pub(crate) struct BlockRenderer<'a> {
    pub(crate) options: &'a ViewOptions,
    pub(crate) lines: Vec<ViewLine>,
    pub(crate) stats: CutStats,
    chrome: Option<usize>,
    chrome_groups: usize,
}

impl<'a> BlockRenderer<'a> {
    pub(crate) fn new(options: &'a ViewOptions) -> Self {
        Self {
            options,
            lines: Vec::new(),
            stats: CutStats::default(),
            chrome: None,
            chrome_groups: 0,
        }
    }

    pub(crate) fn push(&mut self, depth: usize, text: String, heading: Option<u8>) {
        if text.trim().is_empty() {
            return;
        }
        self.lines.push(ViewLine {
            depth,
            text,
            heading,
            chrome: self.chrome,
        });
    }

    pub(crate) fn render_block(&mut self, block: &Block, depth: usize) {
        match &block.kind {
            BlockKind::Heading(level) => {
                let text = self.pack_unbounded(block);
                let marks = "#".repeat(usize::from((*level).clamp(1, 6)));
                self.push(depth, format!("{marks} {text}"), Some(*level));
            }
            BlockKind::List => self.render_list(block, depth),
            BlockKind::Table => self.render_table(block, depth),
            BlockKind::Pre => self.render_pre(block, depth),
            BlockKind::Landmark(_) | BlockKind::Dialog | BlockKind::Frame | BlockKind::Details => {
                self.render_labelled(block, depth);
            }
            _ => match self.pack(block) {
                Some(line) => self.push(depth, format!("{}{line}", prefix(&block.kind)), None),
                None => self.render_children(block, depth),
            },
        }
    }

    fn render_labelled(&mut self, block: &Block, depth: usize) {
        let title = title_of(block);
        let outer = self.chrome;
        if let BlockKind::Landmark(name) = &block.kind {
            if CHROME_LANDMARKS.contains(&name.as_str()) && self.chrome.is_none() {
                self.chrome_groups += 1;
                self.chrome = Some(self.chrome_groups);
            }
        }
        match self.pack(block).filter(|line| line.chars().count() <= LINE_MAX) {
            Some(line) => self.push(depth, format!("{title}: {line}"), None),
            None => {
                self.push(depth, format!("{title}:"), None);
                self.render_children(block, depth + 1);
            }
        }
        self.chrome = outer;
    }

    fn render_pre(&mut self, block: &Block, depth: usize) {
        let mut text = String::new();
        for item in &block.items {
            match item {
                Item::Text(run) => text.push_str(run),
                Item::Break => text.push('\n'),
                Item::Control(control) => text.push_str(&control.name),
                Item::Block(inner) => {
                    text.push('\n');
                    text.push_str(&self.pack_unbounded(inner));
                }
            }
        }
        let lines: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
        for line in lines.iter().take(PRE_LINES) {
            self.push(depth, format!("| {}", line.trim_end()), None);
        }
        if lines.len() > PRE_LINES {
            self.push(depth, format!("| … {} more lines", lines.len() - PRE_LINES), None);
        }
    }

    /// Prints the content of `block` part by part: each run of inline
    /// content as a line, each nested block in turn, repeats marked.
    pub(crate) fn render_children(&mut self, block: &Block, depth: usize) {
        let mut totals: HashMap<&str, usize> = HashMap::new();
        for inner in block.blocks() {
            if let Some(key) = inner.key.as_deref() {
                *totals.entry(key).or_default() += 1;
            }
        }
        let mut seen: HashMap<&str, usize> = HashMap::new();
        let mut run: Vec<&Item> = Vec::new();
        for item in &block.items {
            let Item::Block(inner) = item else {
                run.push(item);
                continue;
            };
            self.flush_run(&mut run, depth);
            let total = inner.key.as_deref().and_then(|key| totals.get(key)).copied().unwrap_or(0);
            let before = self.lines.len();
            self.render_block(inner, depth);
            if total >= 2 {
                let key = inner.key.as_deref().unwrap_or_default();
                let place = seen.entry(key).or_default();
                *place += 1;
                self.mark_repeat(before, *place, total, depth);
            }
        }
        self.flush_run(&mut run, depth);
    }

    fn mark_repeat(&mut self, before: usize, place: usize, total: usize, depth: usize) {
        let produced = self.lines.len() - before;
        let Some(first) = self.lines.get_mut(before) else {
            return;
        };
        if produced == 1 {
            if !first.text.starts_with("- ") && first.heading.is_none() {
                first.text.insert_str(0, "- ");
            }
            return;
        }
        first.text.insert_str(0, &format!("({place}/{total}) "));
        for line in self.lines.iter_mut().skip(before + 1) {
            if line.depth >= depth {
                line.depth += 1;
            }
        }
    }

    fn flush_run(&mut self, run: &mut Vec<&Item>, depth: usize) {
        if run.is_empty() {
            return;
        }
        let items: Vec<Item> = run.drain(..).cloned().collect();
        let mut line = LineText::new(self.options, is_prose(&items));
        for item in &items {
            line.push_item(item);
        }
        let text = line.finish();
        let limit = self.prose_limit();
        self.push(depth, cut_chars(&text, limit), None);
    }

    fn prose_limit(&self) -> usize {
        if self.options.reading { READ_PROSE_MAX } else { PROSE_MAX }
    }

    /// The block on one line, or `None` when it holds a table, a heading,
    /// a landmark or too much to fit. Inline content alone always fits, cut
    /// to the prose limit.
    pub(crate) fn pack(&self, block: &Block) -> Option<String> {
        self.pack_with(block, true)
    }

    /// The block on one line however long, for a heading or a table cell.
    pub(crate) fn pack_unbounded(&self, block: &Block) -> String {
        self.pack_with(block, false).unwrap_or_default()
    }

    fn pack_with(&self, block: &Block, bounded: bool) -> Option<String> {
        let mut line = LineText::new(self.options, is_prose(&block.items));
        let mut parts = 0usize;
        for item in &block.items {
            let Item::Block(inner) = item else {
                line.push_item(item);
                continue;
            };
            if bounded && (folds_never(&inner.kind) || parts >= MAX_PARTS) {
                return None;
            }
            let text = match &inner.kind {
                BlockKind::List if bounded => self.pack_list(inner)?,
                _ => self.pack_with(inner, bounded)?,
            };
            let separator = match inner.kind {
                BlockKind::Cell | BlockKind::HeaderCell => " | ",
                _ => " · ",
            };
            line.push_part(separator, &format!("{}{text}", prefix_in_line(&inner.kind)));
            parts += 1;
        }
        let text = line.finish();
        if bounded && parts > 0 && text.chars().count() > LINE_MAX {
            return None;
        }
        Some(if bounded { cut_chars(&text, self.prose_limit()) } else { text })
    }

    fn pack_list(&self, block: &Block) -> Option<String> {
        let items: Vec<&Block> = block.blocks().collect();
        if items.len() > 8 {
            return None;
        }
        let parts: Option<Vec<String>> = items.iter().map(|item| self.pack(item)).collect();
        let joined = parts?.join(" · ");
        (joined.chars().count() <= LINE_MAX).then_some(joined)
    }
}

fn folds_never(kind: &BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Heading(_) | BlockKind::Table | BlockKind::Landmark(_) | BlockKind::Dialog | BlockKind::Frame | BlockKind::Pre
    )
}

fn prefix(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::ListItem => "- ",
        BlockKind::Quote => "> ",
        _ => "",
    }
}

fn prefix_in_line(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::Quote => "> ",
        _ => "",
    }
}

/// How a landmark, dialog, frame or disclosure is titled: `nav`,
/// `nav "Primary"`, `dialog "Cookies"`, `frame example.com`.
pub(crate) fn title_of(block: &Block) -> String {
    let base = match &block.kind {
        BlockKind::Landmark(name) => name.clone(),
        BlockKind::Dialog => "dialog".to_owned(),
        BlockKind::Frame => "frame".to_owned(),
        BlockKind::Details => "details".to_owned(),
        _ => String::new(),
    };
    match (&block.kind, block.label.as_deref()) {
        (BlockKind::Frame, Some(origin)) => format!("{base} {origin}"),
        (BlockKind::Details, Some(label)) => format!("{base} ({label})"),
        (_, Some(label)) => format!("{base} \"{label}\""),
        (_, None) => base,
    }
}
