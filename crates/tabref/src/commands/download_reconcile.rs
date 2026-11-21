use serde_json::json;
use tabref_core::{AppError, Deadline};
use tabref_page::{DownloadRecord, DownloadState, cancel_download, now_ms};

use super::{current_batch, refresh_kept};
use crate::commands::download_folder::{download_cap, download_folder};
use crate::commands::download_ledger::DownloadLedger;
use crate::session::Session;

pub(crate) fn reconcile(
    session: &mut Session,
    deadline: Deadline,
) -> Result<Vec<DownloadRecord>, AppError> {
    let folder = match download_folder(session) {
        Ok(folder) => folder,
        Err(mut error) => {
            let canceled = DownloadLedger::of(session).recovering_with(|kept| {
                let mut canceled = Vec::new();
                for entry in kept
                    .iter_mut()
                    .filter(|entry| entry.watched && entry.record.running())
                {
                    if cancel_download(session.connection(), &entry.record.guid, deadline).is_ok() {
                        entry.record.finish(DownloadState::Canceled, now_ms());
                        canceled.push(entry.record.guid.clone());
                    }
                }
                canceled
            });
            let canceled = match canceled {
                Ok(canceled) => canceled,
                Err(tracking) => {
                    error.message.push_str(&format!(
                        "; download tracking also failed: {}",
                        tracking.message
                    ));
                    return Err(error);
                }
            };
            error.message.push_str(&format!(
                "; canceled {} watched download(s) because their folder cannot be checked safely",
                canceled.len()
            ));
            error.details =
                Some(json!({"folder_error":error.details, "canceled_downloads":canceled}));
            return Err(error);
        }
    };
    let cap = download_cap(session);
    DownloadLedger::of(session).recovering_with(|kept| {
        refresh_kept(session.connection(), kept, cap, folder.as_deref(), deadline);
        kept.iter_mut()
            .filter_map(|entry| {
                if entry.reported || !matches!(entry.record.state, DownloadState::TooLarge { .. }) {
                    return None;
                }
                entry.reported = true;
                entry.batch = current_batch();
                Some(entry.record.clone())
            })
            .collect()
    })
}
