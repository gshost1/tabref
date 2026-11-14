use tabref_page::{Block, BlockKind, cut_chars};

use super::block_renderer::{BlockRenderer, LINE_MAX};
use super::row_window::DEFAULT_ROWS;

const COUNTED_LIST: usize = 6;
const CELL_MAX: usize = 300;

impl BlockRenderer<'_> {
    /// A list on one line when it is short (a menu), else one line per item
    /// under an `N items:` count when it has six or more. A list longer than
    /// the row window prints the window and says how to see the rest.
    pub(crate) fn render_list(&mut self, block: &Block, depth: usize) {
        let items: Vec<&Block> = block.blocks().collect();
        if items.len() <= 8 {
            let packed: Option<Vec<String>> = items.iter().map(|item| self.pack(item)).collect();
            if let Some(parts) = packed {
                let joined = parts.join(" · ");
                if items.len() > 1 && joined.chars().count() <= LINE_MAX && parts.iter().all(|part| part.chars().count() <= 60) {
                    self.push(depth, joined, None);
                    return;
                }
            }
        }
        if items.len() >= COUNTED_LIST {
            self.push(depth, format!("{} items:", items.len()), None);
        }
        let windowed = items.len() > DEFAULT_ROWS;
        for (position, item) in items.iter().enumerate() {
            if windowed && !self.options.rows.contains(position + 1) {
                continue;
            }
            let before = self.lines.len();
            self.render_block(item, depth);
            if let Some(first) = self.lines.get_mut(before) {
                if !first.text.starts_with("- ") && first.heading.is_none() {
                    first.text.insert_str(0, "- ");
                }
            }
            for line in self.lines.iter_mut().skip(before + 1) {
                line.depth = line.depth.max(depth + 1);
            }
        }
        if windowed {
            self.note_window(items.len(), depth, "items");
        }
    }

    /// A data table as an `N rows:` line with its header cells, then one
    /// line per row, cells joined by ` | ` and each row's controls in it. A
    /// table used for layout (one column, or cells holding long text) prints
    /// as plain blocks.
    pub(crate) fn render_table(&mut self, block: &Block, depth: usize) {
        let mut rows = Vec::new();
        collect_rows(block, &mut rows);
        let widest = rows.iter().map(|row| row.blocks().count()).max().unwrap_or(0);
        let cells: Vec<Vec<String>> = rows
            .iter()
            .map(|row| row.blocks().map(|cell| self.pack_unbounded(cell)).collect())
            .collect();
        let layout = widest < 2 || cells.iter().flatten().any(|cell| cell.chars().count() > CELL_MAX);
        if rows.is_empty() || layout {
            self.render_children(block, depth);
            return;
        }
        let header = rows
            .first()
            .is_some_and(|row| row.blocks().all(|cell| cell.kind == BlockKind::HeaderCell));
        let body_rows = rows.len() - usize::from(header);
        let caption = block
            .blocks()
            .find(|inner| inner.kind == BlockKind::Paragraph)
            .map(|caption| format!(" \"{}\"", cut_chars(&self.pack_unbounded(caption), 80)))
            .unwrap_or_default();
        let head = if header {
            cells.first().map(|cells| cells.join(" | ")).unwrap_or_default()
        } else {
            String::new()
        };
        let noun = if body_rows == 1 { "row" } else { "rows" };
        self.push(depth, format!("table{caption} {body_rows} {noun}: {head}").trim_end().to_owned(), None);
        let windowed = body_rows > DEFAULT_ROWS;
        for (position, row) in cells.iter().skip(usize::from(header)).enumerate() {
            if windowed && !self.options.rows.contains(position + 1) {
                continue;
            }
            let text = row.iter().map(|cell| cut_chars(cell, CELL_MAX)).collect::<Vec<_>>().join(" | ");
            self.push(depth + 1, text, None);
        }
        if windowed {
            self.note_window(body_rows, depth + 1, "rows");
        }
    }

    fn note_window(&mut self, total: usize, depth: usize, noun: &str) {
        let window = self.options.rows;
        let shown = (1..=total).filter(|row| window.contains(*row)).count();
        let cut = total - shown;
        if cut == 0 {
            return;
        }
        self.stats.rows_cut += cut;
        let last_shown = (1..=total).rev().find(|row| window.contains(*row));
        let next = match last_shown {
            Some(last) if last < total => last + 1,
            _ => 1,
        };
        self.stats.next_row.get_or_insert(next);
        let text = if next > 1 {
            format!("… {cut} more {noun} ({total} in all): tabref snap --rows {next}-")
        } else {
            format!("… {cut} {noun} before these ({total} in all): tabref snap --rows 1-")
        };
        self.push(depth, text, None);
    }
}

fn collect_rows<'a>(block: &'a Block, rows: &mut Vec<&'a Block>) {
    for inner in block.blocks() {
        match inner.kind {
            BlockKind::Row => rows.push(inner),
            BlockKind::Table => {}
            _ => collect_rows(inner, rows),
        }
    }
}
