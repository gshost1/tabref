//! The connection to the user's browser that is still waiting for its
//! user's approval, kept between commands, and the error that says so.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::Duration;

use tabref_cdp::PendingApproval;
use tabref_core::{AppError, Deadline, DeliverySemantics, ErrorCode};

use super::user_chrome_record::UserChromeRecord;

/// The connection to the user's browser that is still waiting for its
/// user's approval, kept by the daemon between commands so the prompt the
/// browser shows for it stays the one that counts: the next command waits on
/// it instead of opening another connection, which would open one more
/// prompt. Keyed by profile, browser process and port.
static WAITING: Mutex<Option<Waiting>> = Mutex::new(None);

pub(crate) struct Waiting {
    pub(crate) key: WaitingKey,
    pub(crate) pending: PendingApproval,
}

pub(crate) type WaitingKey = (PathBuf, u32, u16);

/// One connection attempt to the user's browser at a time in this process,
/// so a second one resumes the connection the first left waiting instead of
/// opening a second prompt.
static CONNECTING: Mutex<()> = Mutex::new(());

pub(crate) fn connecting_turn(deadline: Deadline) -> Result<MutexGuard<'static, ()>, AppError> {
    loop {
        match CONNECTING.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(TryLockError::Poisoned(poisoned)) => return Ok(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) if deadline.is_expired() => {
                return Err(AppError::new(
                    ErrorCode::Timeout,
                    "Another tabref command was connecting to your browser until the deadline",
                )
                .with_suggestion("Wait for it to finish, or pass a longer --timeout")
                .with_details(serde_json::json!({ "kind": "user_chrome_busy" }))
                .with_disposition(DeliverySemantics::not_delivered()));
            }
            Err(TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

/// The kept connection for `key`; any kept for another browser or port is
/// dropped (`None` drops whatever is kept).
pub(crate) fn take_waiting(key: Option<&WaitingKey>) -> Option<PendingApproval> {
    let mut slot = WAITING.lock().ok()?;
    let waiting = slot.take()?;
    (Some(&waiting.key) == key).then_some(waiting.pending)
}

pub(crate) fn put_waiting(waiting: Waiting) {
    if let Ok(mut slot) = WAITING.lock() {
        *slot = Some(waiting);
    }
}

/// The error for a connection the user's browser has not approved yet.
/// `kept`: the daemon keeps it waiting, so the prompt on screen is still the
/// one that counts and the next command opens no new one; otherwise it was
/// dropped (see `tabref_cdp`'s own `cdp_approval_pending`).
pub(crate) fn waiting_for_approval(
    record: &UserChromeRecord,
    port: u16,
    waited: Duration,
    kept: bool,
    resumed: bool,
) -> AppError {
    let browser = &record.name;
    let seconds = waited.as_secs();
    let (message, suggestion) = if kept {
        (
            format!(
                "Your {browser} is asking you 'Allow remote debugging?' for tabref's connection \
                 and has not been answered{}",
                if resumed {
                    format!(" (the same prompt, open for {seconds} s; no new prompt was opened)")
                } else {
                    String::new()
                }
            ),
            format!(
                "tabref keeps this connection waiting (until its daemon has had no command for \
                 {} min), so the prompt on screen is the one that counts: have it answered with \
                 Allow (by the user, or with agent-desktop: 'snapshot --app \"{browser}\" \
                 --surface sheet -i', then click Allow), then run the command again; it opens no \
                 new prompt. A longer --timeout lets one command wait for the answer",
                crate::daemon::idle_timeout().as_secs().div_ceil(60)
            ),
        )
    } else {
        (
            format!(
                "Your {browser} did not approve tabref's connection before the deadline: it is \
                 asking you 'Allow remote debugging?'"
            ),
            format!(
                "Do not just retry: each new connection opens one more prompt, and this one's \
                 prompt stays on screen but approves nothing now. Cancel the prompts left on \
                 screen, run the command once with --timeout 60000, and while it waits have its \
                 prompt answered with Allow (by the user, or with agent-desktop: 'snapshot --app \
                 \"{browser}\" --surface sheet -i', then click Allow)"
            ),
        )
    };
    AppError::new(ErrorCode::PermDenied, message)
        .with_suggestion(suggestion)
        .with_details(serde_json::json!({
            "kind": "cdp_approval_pending",
            "port": port,
            "elapsed_ms": waited.as_millis(),
            "prompt_kept": kept,
            "retryable": true,
        }))
        .with_disposition(DeliverySemantics::not_delivered())
}
