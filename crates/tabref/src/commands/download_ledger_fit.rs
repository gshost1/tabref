use tabref_core::AppError;

use super::{KEPT, MAX_BYTES};
use crate::commands::kept_download::KeptDownload;

const MAX_PATH_BYTES: usize = 1024;

fn evictable(entry: &KeptDownload) -> bool {
    !entry.record.running()
        && entry.reported
        && entry
            .cleanup
            .as_ref()
            .is_none_or(|cleanup| cleanup.pending_paths.is_empty())
}

pub(super) fn encode(records: &mut Vec<KeptDownload>) -> Result<Vec<u8>, AppError> {
    for entry in records.iter_mut() {
        super::bound_fields(entry);
    }
    while records.len() > KEPT {
        let Some(index) = records.iter().position(evictable) else {
            break;
        };
        records.remove(index);
    }
    loop {
        let long_path = records.iter().any(|entry| {
            entry
                .record
                .path
                .iter()
                .chain(entry.cleanup.iter().flat_map(|cleanup| {
                    cleanup.pending_paths.iter().chain(&cleanup.abandoned_paths)
                }))
                .any(|path| path.len() > MAX_PATH_BYTES)
        });
        let bytes = serde_json::to_vec(records)?;
        if !long_path && bytes.len() as u64 <= MAX_BYTES {
            return Ok(bytes);
        }
        if let Some(index) = records.iter().position(evictable) {
            records.remove(index);
            continue;
        }
        let reason = if long_path {
            "an actionable path exceeds 1024 bytes and cannot safely be shortened"
        } else {
            "protected watches and pending removals exceed 64 KiB"
        };
        return Err(AppError::invalid_args(format!("Cannot persist download tracking: {reason}; existing ledger preserved; new downloads may not be watched"))
            .with_details(serde_json::json!({"kind":"download_ledger_full","max_bytes":MAX_BYTES,"max_path_bytes":MAX_PATH_BYTES})));
    }
}
