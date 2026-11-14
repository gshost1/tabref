//! Reads every frame of a page at once. The page target and each
//! out-of-process iframe target get their frame list, DOM capture and root
//! accessibility tree asked for together, over the one connection, and each
//! newly reported iframe target is asked the moment its parent's
//! auto-attach answers, so frames in other renderer processes are read in
//! parallel with the page. Frames whose owner element can show nothing are
//! left out as soon as that is known, without waiting for their reads; the
//! rest are waited for within the frame budget, and a frame still pending
//! then is left out as timed out while the snapshot returns.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde_json::{Value, json};
use tabref_core::{AppError, Deadline};

use super::child_targets::{AUTO_ATTACH, auto_attach_params, discard_attach_events};
use super::frame_budget::FrameBudget;
use super::frame_dom::FrameDom;
use super::frame_skip_reason::FrameSkipReason;
use super::pending_read::PendingRead;
use super::skipped_frame::SkippedFrame;
use super::target_state::TargetState;
use crate::{CdpConnection, PageSession};
use tabref_cdp::{is_protocol_error, is_target_gone};

const RELEASE_CAP_MS: u64 = 250;

pub(crate) struct FrameReader {
    pub(crate) targets: Vec<TargetState>,
    pub(crate) pending: HashMap<u64, PendingRead>,
    pub(crate) budget: FrameBudget,
    pub(crate) left_out: Vec<SkippedFrame>,
    pub(crate) closed: HashSet<String>,
    pub(crate) page_done: Option<Instant>,
    pub(crate) auto_attached: Vec<usize>,
}

impl FrameReader {
    pub(crate) fn new(page: &PageSession, budget: FrameBudget) -> Self {
        Self {
            targets: vec![TargetState::new(page.clone(), None, String::new())],
            pending: HashMap::new(),
            budget,
            left_out: Vec::new(),
            closed: HashSet::new(),
            page_done: None,
            auto_attached: Vec::new(),
        }
    }

    /// Reads until every frame answered, was left out, or missed the
    /// budget. An error means the page itself could not be read.
    pub(crate) fn run(&mut self, connection: &mut CdpConnection) -> Result<(), AppError> {
        self.start_target(connection, 0)?;
        loop {
            let page_pending = self.pending.values().any(PendingRead::is_page_read);
            if !page_pending && self.page_done.is_none() {
                self.page_done = Some(Instant::now());
            }
            if self.pending.is_empty() {
                return Ok(());
            }
            let wait = if page_pending {
                self.budget.snapshot()
            } else {
                self.budget.iframes(self.page_done)
            };
            let ids: Vec<u64> = self.pending.keys().copied().collect();
            match connection.next_reply(&ids, wait) {
                Ok((id, outcome)) => {
                    if let Some(read) = self.pending.remove(&id) {
                        self.on_reply(connection, read, outcome)?;
                    }
                }
                Err(error) if !page_pending && self.budget.missed(&error) => {
                    self.time_out_pending(connection);
                    return Ok(());
                }
                Err(error) => {
                    self.abandon_where(connection, |_| true);
                    return Err(error);
                }
            }
        }
    }

    /// Asks one target for its auto-attached iframes, its frames, its DOM
    /// capture and its root frame's tree, all at once.
    pub(crate) fn start_target(
        &mut self,
        connection: &mut CdpConnection,
        target: usize,
    ) -> Result<(), AppError> {
        self.send(
            connection,
            AUTO_ATTACH,
            auto_attach_params(true),
            PendingRead::AutoAttach(target),
        )?;
        self.auto_attached.push(target);
        self.send(
            connection,
            "Page.getFrameTree",
            json!({}),
            PendingRead::FrameTree(target),
        )?;
        self.send(
            connection,
            "DOMSnapshot.captureSnapshot",
            FrameDom::capture_params(),
            PendingRead::Dom(target),
        )?;
        let root = PendingRead::Ax {
            target,
            frame_id: String::new(),
        };
        self.send(connection, "Accessibility.getFullAXTree", json!({}), root)
    }

    /// Sends `method` to `target`'s session and remembers what it is for. A
    /// child target whose session is already gone is left out instead.
    pub(crate) fn send(
        &mut self,
        connection: &mut CdpConnection,
        method: &str,
        params: Value,
        read: PendingRead,
    ) -> Result<(), AppError> {
        let target = read.target();
        if target > 0 && self.is_closed_target(target) {
            return Ok(());
        }
        let session = self.targets[target].session.session_id.clone();
        match connection.send(method, params, Some(&session), self.budget.snapshot()) {
            Ok(id) => {
                self.pending.insert(id, read);
                Ok(())
            }
            Err(error) if target > 0 && is_target_gone(&error) => {
                self.fail_target(connection, target, FrameSkipReason::Unreadable);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// Leaves `frame_id` out for `reason`, once.
    pub(crate) fn leave_out(&mut self, frame_id: &str, url: String, reason: FrameSkipReason) {
        if self.closed.insert(frame_id.to_owned()) {
            self.left_out.push(SkippedFrame {
                frame_id: frame_id.to_owned(),
                url,
                reason,
            });
        }
    }

    /// Leaves out a whole iframe target, and every target beneath it, and
    /// stops waiting for their reads.
    pub(crate) fn fail_target(
        &mut self,
        connection: &mut CdpConnection,
        target: usize,
        reason: FrameSkipReason,
    ) {
        let state = &self.targets[target];
        let (id, url) = (state.session.target_id.clone(), state.url.clone());
        self.leave_out(&id, url, reason);
        let doomed: HashSet<usize> = (0..self.targets.len())
            .filter(|index| self.descends_from(*index, target))
            .collect();
        self.abandon_where(connection, |read| doomed.contains(&read.target()));
    }

    /// Whether a target, or a target or frame it sits in, was left out.
    pub(crate) fn is_closed_target(&self, target: usize) -> bool {
        let mut current = Some(target);
        while let Some(index) = current {
            let state = &self.targets[index];
            let frame = state.parent.as_ref().and_then(|(_, frame)| frame.as_ref());
            if self.closed.contains(&state.session.target_id)
                || frame.is_some_and(|frame| self.closed.contains(frame))
            {
                return index > 0;
            }
            current = state.parent.as_ref().map(|(parent, _)| *parent);
        }
        false
    }

    pub(crate) fn abandon_where(
        &mut self,
        connection: &mut CdpConnection,
        matches: impl Fn(&PendingRead) -> bool,
    ) {
        let ids: Vec<u64> = self
            .pending
            .iter()
            .filter(|(_, read)| matches(read))
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.pending.remove(&id);
            connection.abandon(id);
        }
    }

    /// Turns auto-attach back off on every target it was turned on for,
    /// which detaches the iframe sessions, and drops the attach events.
    /// Best effort and briefly bounded: the frames are already read. It has
    /// its own short deadline, so a snapshot that ran out of time still
    /// leaves no auto-attach behind on a connection that is kept.
    pub(crate) fn release(&mut self, connection: &mut CdpConnection) {
        let deadline = Deadline::after(RELEASE_CAP_MS).unwrap_or(self.budget.snapshot());
        let mut ids = Vec::new();
        for target in self.auto_attached.iter().rev() {
            let session = &self.targets[*target].session;
            if connection.is_session_detached(&session.session_id) {
                continue;
            }
            if let Ok(id) = connection.send(
                AUTO_ATTACH,
                auto_attach_params(false),
                Some(&session.session_id),
                deadline,
            ) {
                ids.push(id);
            }
        }
        while !ids.is_empty() {
            match connection.next_reply(&ids, deadline) {
                Ok((id, _)) => ids.retain(|pending| *pending != id),
                Err(_) => break,
            }
        }
        for id in ids {
            connection.abandon(id);
        }
        for target in &self.auto_attached {
            discard_attach_events(connection, &self.targets[*target].session);
        }
    }

    fn time_out_pending(&mut self, connection: &mut CdpConnection) {
        let pending: Vec<PendingRead> = self.pending.values().cloned().collect();
        self.abandon_where(connection, |_| true);
        for read in pending {
            match read {
                PendingRead::Ax { target, frame_id } if !frame_id.is_empty() => {
                    let url = self.targets[target].frame_url(&frame_id);
                    self.leave_out(&frame_id, url, FrameSkipReason::TimedOut);
                }
                PendingRead::Ax { target, .. } | PendingRead::FrameTree(target) if target > 0 => {
                    self.fail_target(connection, target, FrameSkipReason::TimedOut);
                }
                _ => {}
            }
        }
    }

    fn descends_from(&self, mut index: usize, ancestor: usize) -> bool {
        loop {
            if index == ancestor {
                return true;
            }
            match self.targets[index].parent {
                Some((parent, _)) => index = parent,
                None => return false,
            }
        }
    }
}

/// Whether a failed read only loses its frame: the browser refused it or
/// the target went away.
pub(crate) fn loses_only_the_frame(error: &AppError) -> bool {
    is_protocol_error(error) || is_target_gone(error)
}

#[cfg(test)]
#[path = "frame_reader_tests.rs"]
mod tests;
