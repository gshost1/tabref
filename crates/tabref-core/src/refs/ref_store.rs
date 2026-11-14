use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::ref_entry::RefEntry;
use super::ref_id::RefId;
use super::ref_map::RefMap;
use super::snapshot_id::validate_snapshot_id;
use crate::AppError;
use crate::storage::private_file::{read_bounded, write_atomic};
use crate::storage::state_root::resolve_state_root;

const LATEST_FILE: &str = "latest_snapshot_id";
const MAX_REFMAP_BYTES: u64 = 8 * 1024 * 1024;
const MAX_STATE_BYTES: u64 = 64 * 1024;
const MAX_SAVED_SNAPSHOTS: usize = 128;
const PRUNE_LOW_WATER: usize = 96;

/// The snapshots tabref saved, under `<state root>/snapshots/<id>/refmap.json`
/// with `latest_snapshot_id` naming the newest, the layout agent-desktop
/// uses. Small named state files live beside them.
#[derive(Debug, Clone)]
pub struct RefStore {
    base_dir: PathBuf,
}

impl RefStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    /// The store at the configured state root (`TABREF_HOME` or `~/.tabref`).
    pub fn open() -> Result<Self, AppError> {
        Ok(Self::new(resolve_state_root()?))
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Saves `map` as `snapshot_id`, points `latest_snapshot_id` at it and
    /// drops the oldest snapshots once more than 128 are kept.
    pub fn save(&self, snapshot_id: &str, map: &RefMap) -> Result<(), AppError> {
        validate_snapshot_id(snapshot_id)?;
        let json = serde_json::to_vec(map)?;
        write_atomic(&self.snapshot_path(snapshot_id), &json)?;
        write_atomic(&self.base_dir.join(LATEST_FILE), snapshot_id.as_bytes())?;
        self.prune(snapshot_id);
        Ok(())
    }

    pub fn load(&self, snapshot_id: &str) -> Result<RefMap, AppError> {
        validate_snapshot_id(snapshot_id)?;
        match read_bounded(&self.snapshot_path(snapshot_id), MAX_REFMAP_BYTES) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                Err(AppError::snapshot_not_found(snapshot_id))
            }
            Err(error) => Err(error.into()),
        }
    }

    /// The entry `ref_id` names, with the map it came from. An element
    /// number the snapshot never handed out is `STALE_REF`.
    pub fn entry(&self, ref_id: &RefId) -> Result<(RefMap, RefEntry), AppError> {
        let map = self.load(&ref_id.snapshot_id)?;
        let entry = map
            .get(ref_id.element)
            .cloned()
            .ok_or_else(|| AppError::stale_ref(&ref_id.to_string()))?;
        Ok((map, entry))
    }

    pub fn latest_id(&self) -> Option<String> {
        let text = self.read_state(LATEST_FILE)?;
        let id = text.trim().to_owned();
        validate_snapshot_id(&id).ok().map(|()| id)
    }

    /// The contents of the small state file `name`, if it exists.
    pub fn read_state(&self, name: &str) -> Option<String> {
        let bytes = read_bounded(&self.base_dir.join(name), MAX_STATE_BYTES).ok()?;
        String::from_utf8(bytes).ok()
    }

    pub fn write_state(&self, name: &str, contents: &str) -> Result<(), AppError> {
        write_atomic(&self.base_dir.join(name), contents.as_bytes()).map_err(AppError::from)
    }

    pub fn remove_state(&self, name: &str) {
        let _ = std::fs::remove_file(self.base_dir.join(name));
    }

    fn snapshots_dir(&self) -> PathBuf {
        self.base_dir.join("snapshots")
    }

    fn snapshot_path(&self, snapshot_id: &str) -> PathBuf {
        self.snapshots_dir().join(snapshot_id).join("refmap.json")
    }

    fn prune(&self, keep: &str) {
        let Ok(entries) = std::fs::read_dir(self.snapshots_dir()) else {
            return;
        };
        let mut dated: Vec<(std::time::SystemTime, PathBuf)> = entries
            .flatten()
            .filter(|entry| entry.file_name() != keep)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| {
                let modified = entry
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                (modified, entry.path())
            })
            .collect();
        if dated.len() < MAX_SAVED_SNAPSHOTS {
            return;
        }
        dated.sort();
        let excess = dated.len().saturating_sub(PRUNE_LOW_WATER - 1);
        for (_, path) in dated.into_iter().take(excess) {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

#[cfg(test)]
#[path = "ref_store_tests.rs"]
mod tests;
