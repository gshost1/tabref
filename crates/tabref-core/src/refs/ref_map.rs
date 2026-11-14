use serde::{Deserialize, Serialize};

use super::ref_entry::RefEntry;

/// The refs of one snapshot, in allocation order: `@eN` is `refs[N - 1]`.
/// `tab_id` is the page target the snapshot read and `url` the address it
/// showed then.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefMap {
    pub tab_id: String,
    pub url: String,
    pub refs: Vec<RefEntry>,
}

impl RefMap {
    pub fn new(tab_id: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            tab_id: tab_id.into(),
            url: url.into(),
            refs: Vec::new(),
        }
    }

    /// Records `entry` and returns its element number.
    pub fn push(&mut self, entry: RefEntry) -> u32 {
        self.refs.push(entry);
        u32::try_from(self.refs.len()).unwrap_or(u32::MAX)
    }

    pub fn get(&self, element: u32) -> Option<&RefEntry> {
        let index = usize::try_from(element).ok()?.checked_sub(1)?;
        self.refs.get(index)
    }

    pub fn len(&self) -> usize {
        self.refs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.refs.is_empty()
    }
}
