use std::collections::HashMap;

use tabref_page::{Block, BlockKind, Item, cut_chars};

use super::cut_stats::CutStats;
use super::line_text::{LineText, is_prose, label_echoes};
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
            BlockKind::Landmark(_) | BlockKind::Dialog | BlockKind::Frame => {
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
        match self
            .pack(block)
            .filter(|line| line.chars().count() <= LINE_MAX)
        {
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
        let lines: Vec<&str> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        for line in lines.iter().take(PRE_LINES) {
            self.push(depth, format!("| {}", line.trim_end()), None);
        }
        if lines.len() > PRE_LINES {
            self.push(
                depth,
                format!("| … {} more lines", lines.len() - PRE_LINES),
                None,
            );
        }
    }

    /// Prints the content of `block` part by part: each run of inline
    /// content as a line, each nested block in turn. A nested block of the
    /// same shape as its parent (a reply inside a comment) is indented.
    /// Repeats are marked once the whole group is printed.
    pub(crate) fn render_children(&mut self, block: &Block, depth: usize) {
        let mut totals: HashMap<&str, usize> = HashMap::new();
        for inner in block.blocks() {
            if let Some(key) = inner.key.as_deref() {
                *totals.entry(key).or_default() += 1;
            }
        }
        let echoes = label_echoes(&block.items);
        let mut members: Vec<(&str, usize, usize, bool)> = Vec::new();
        let mut run: Vec<usize> = Vec::new();
        for (position, item) in block.items.iter().enumerate() {
            let Item::Block(inner) = item else {
                run.push(position);
                continue;
            };
            self.flush_run(block, &echoes, &mut run, depth);
            let nested = inner.key.is_some() && inner.key == block.key;
            let before = self.lines.len();
            self.render_block(inner, if nested { depth + 1 } else { depth });
            let repeated = inner
                .key
                .as_deref()
                .filter(|key| totals.get(key).copied().unwrap_or(0) >= 2);
            if let Some(key) = repeated {
                members.push((
                    key,
                    before,
                    self.lines.len(),
                    inner.blocks().next().is_some(),
                ));
            }
        }
        self.flush_run(block, &echoes, &mut run, depth);
        let mut keys: Vec<&str> = members.iter().map(|member| member.0).collect();
        keys.sort_unstable();
        keys.dedup();
        for key in keys {
            let group: Vec<&(&str, usize, usize, bool)> = members
                .iter()
                .filter(|member| member.0 == key && member.2 > member.1)
                .collect();
            let structured = group.iter().any(|member| member.3);
            let ranges: Vec<(usize, usize)> =
                group.iter().map(|member| (member.1, member.2)).collect();
            self.mark_group(&ranges, depth, structured);
        }
    }

    /// Marks the members of one repeated group: `(k/N) ` before each
    /// member's first line, its other lines indented, when the members have
    /// parts of their own (a sense with its examples, a comment with its
    /// replies); else `- ` before each one-line member. A group whose members
    /// open with a heading is left alone: the headings already divide it.
    fn mark_group(&mut self, ranges: &[(usize, usize)], depth: usize, structured: bool) {
        let total = ranges.len();
        if total < 2
            || ranges
                .iter()
                .any(|(start, _)| self.lines[*start].heading.is_some())
        {
            return;
        }
        let multi = structured || ranges.iter().any(|(start, end)| end - start > 1);
        for (place, (start, end)) in ranges.iter().enumerate() {
            if !multi {
                let first = &mut self.lines[*start];
                if !first.text.starts_with("- ") {
                    first.text.insert_str(0, "- ");
                }
                continue;
            }
            self.lines[*start]
                .text
                .insert_str(0, &format!("({}/{total}) ", place + 1));
            for line in self.lines.iter_mut().take(*end).skip(start + 1) {
                if line.depth >= depth {
                    line.depth += 1;
                }
            }
        }
    }

    fn flush_run(&mut self, block: &Block, echoes: &[bool], run: &mut Vec<usize>, depth: usize) {
        if run.is_empty() {
            return;
        }
        let items: Vec<Item> = run
            .drain(..)
            .filter(|position| !echoes.get(*position).copied().unwrap_or(false))
            .filter_map(|position| block.items.get(position).cloned())
            .collect();
        let mut line = LineText::new(self.options, is_prose(&items));
        for item in &items {
            line.push_item(item);
        }
        let text = line.finish();
        let limit = self.prose_limit();
        self.push(depth, cut_chars(&text, limit), None);
    }

    fn prose_limit(&self) -> usize {
        if self.options.reading {
            READ_PROSE_MAX
        } else {
            PROSE_MAX
        }
    }

    /// The block on one line, or `None` when it holds a table, a heading,
    /// a landmark or too much to fit. Inline content alone always fits, cut
    /// to the prose limit.
    pub(crate) fn pack(&self, block: &Block) -> Option<String> {
        self.pack_with(block, true, false)
    }

    /// The block on one line however long, for a heading or a table cell.
    pub(crate) fn pack_unbounded(&self, block: &Block) -> String {
        self.pack_with(block, false, false).unwrap_or_default()
    }

    fn pack_with(&self, block: &Block, bounded: bool, prose_around: bool) -> Option<String> {
        let prose = prose_around || is_prose(&block.items);
        let mut line = LineText::new(self.options, prose);
        let mut parts = 0usize;
        let echoes = label_echoes(&block.items);
        for (position, item) in block.items.iter().enumerate() {
            let Item::Block(inner) = item else {
                if !echoes.get(position).copied().unwrap_or(false) {
                    line.push_item(item);
                }
                continue;
            };
            let nested = inner.key.is_some() && inner.key == block.key;
            if bounded && (folds_never(&inner.kind) || parts >= MAX_PARTS || nested) {
                return None;
            }
            let text = match &inner.kind {
                BlockKind::List if bounded => self.pack_list(inner)?,
                _ => self.pack_with(inner, bounded, prose)?,
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
        Some(if bounded {
            cut_chars(&text, self.prose_limit())
        } else {
            text
        })
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
        BlockKind::Heading(_)
            | BlockKind::Table
            | BlockKind::Landmark(_)
            | BlockKind::Dialog
            | BlockKind::Frame
            | BlockKind::Pre
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

/// How a landmark, dialog or frame is titled: `nav`,
/// `nav "Primary"`, `dialog "Cookies"`, `frame example.com`.
pub(crate) fn title_of(block: &Block) -> String {
    let base = match &block.kind {
        BlockKind::Landmark(name) => name.clone(),
        BlockKind::Dialog => "dialog".to_owned(),
        BlockKind::Frame => "frame".to_owned(),
        _ => String::new(),
    };
    match (&block.kind, block.label.as_deref()) {
        (BlockKind::Frame, Some(origin)) => format!("{base} {origin}"),
        (_, Some(label)) => format!("{base} \"{label}\""),
        (_, None) => base,
    }
}

#[cfg(test)]
#[path = "block_renderer_tests.rs"]
mod tests;
