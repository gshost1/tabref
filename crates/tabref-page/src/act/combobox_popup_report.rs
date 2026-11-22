use crate::act::page_mark::BlockText;
use crate::act::text_diff::text_lines;
use crate::{PageMark, TextChange};
use serde_json::Value;

/// A popup visible before the pick, located in the action watch's text block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::act) struct PopupText {
    block: u64,
    watch: String,
    id: Option<u64>,
    lines: Vec<String>,
}

pub(in crate::act) fn omit_display_change(old: &str, new: &str, changes: &mut Vec<TextChange>) {
    let old = text_lines(old).join(" ");
    let new = text_lines(new).join(" ");
    if old == new || old.is_empty() || new.is_empty() {
        return;
    }
    let mut matches = changes.iter().enumerate().filter_map(|(index, change)| {
        let next = changes.get(index + 1);
        match (change, next) {
            (TextChange::Changed { before, after }, _) if replaces(before, after, &old, &new) => {
                Some(index..index + 1)
            }
            (TextChange::Removed(line), Some(TextChange::Changed { before, after }))
                if line == &old && replaces(before, after, "", &new) =>
            {
                Some(index..index + 2)
            }
            (TextChange::Changed { before, after }, Some(TextChange::Added(line)))
                if line == &new && replaces(before, after, &old, "") =>
            {
                Some(index..index + 2)
            }
            (TextChange::Removed(before), Some(TextChange::Added(after)))
                if before == &old && after == &new =>
            {
                Some(index..index + 2)
            }
            _ => None,
        }
    });
    if let Some(range) = matches.next()
        && matches.next().is_none()
    {
        changes.drain(range);
    }
}

fn replaces(before: &str, after: &str, old: &str, new: &str) -> bool {
    let before: Vec<_> = before.split_whitespace().collect();
    let after: Vec<_> = after.split_whitespace().collect();
    let old: Vec<_> = old.split_whitespace().collect();
    let new: Vec<_> = new.split_whitespace().collect();
    let Some(room) = before.len().checked_sub(old.len()) else {
        return false;
    };
    if room + new.len() != after.len() {
        return false;
    }
    (0..=room).any(|at| {
        before[..at] == after[..at]
            && before[at..at + old.len()] == old
            && after[at..at + new.len()] == new
            && before[at + old.len()..] == after[at + new.len()..]
    })
}

pub(super) const WATCHED_TEXT: &str = r"function() {
  const doc = this.ownerDocument;
  const view = doc.defaultView;
  let top;
  try { top = view.top[Symbol.for('tabref.watch')]; } catch { return null; }
  if (!top || !this.isConnected) return null;
  const root = this.getRootNode();
  const block = root !== doc ? top.ids.get(root) : view === view.top ? 0 : top.ids.get(view.frameElement);
  if (block === undefined) return null;
  return { block, watch: top.epoch, id: top.ids.get(this), text: this.innerText };
}";

impl PopupText {
    pub(super) fn from_reply(reply: &Value) -> Option<Self> {
        Some(Self {
            block: reply["block"].as_u64()?,
            watch: reply["watch"].as_str()?.to_owned(),
            id: reply["id"].as_u64(),
            lines: text_lines(reply["text"].as_str().unwrap_or_default()),
        })
    }

    pub(in crate::act) fn omit_from(&self, mark: &mut PageMark) {
        if let Some(id) = self.id {
            mark.popups
                .retain(|popup| popup["watch"] != self.watch || popup["id"] != id);
        }
        if self.lines.is_empty() {
            return;
        }
        let Some((_, BlockText::Readable(text))) = mark
            .blocks
            .iter_mut()
            .find(|(block, _)| *block == self.block)
        else {
            return;
        };
        let mut lines = text_lines(text);
        let mut occurrences = lines
            .windows(self.lines.len())
            .enumerate()
            .filter(|(_, run)| *run == self.lines)
            .map(|(index, _)| index);
        if let Some(start) = occurrences.next()
            && occurrences.next().is_none()
        {
            lines.drain(start..start + self.lines.len());
            *text = lines.join("\n");
        }
    }
}
