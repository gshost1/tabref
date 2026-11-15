//! What the frame reader does with each reply: a target's auto-attach
//! reports its iframe targets, which are read at once; its frame list sends
//! for its in-process iframes; a DOM capture or a frame owner may show that a
//! frame can hold nothing, which leaves it out early.

use serde_json::{Value, json};
use tabref_core::AppError;

use super::child_targets::take_attached_children;
use super::frame_dom::FrameDom;
use super::frame_reader::{FrameReader, loses_only_the_frame};
use super::frame_skip_reason::FrameSkipReason;
use super::pending_read::PendingRead;
use super::target_read::parse_nodes;
use super::target_state::TargetState;
use crate::CdpConnection;
use tabref_cdp::{is_protocol_error, parse_frame_tree};

type Outcome = Result<Value, AppError>;

impl FrameReader {
    pub(crate) fn on_reply(
        &mut self,
        connection: &mut CdpConnection,
        read: PendingRead,
        outcome: Outcome,
    ) -> Result<(), AppError> {
        match read {
            PendingRead::AutoAttach(target) => self.on_auto_attach(connection, target, outcome),
            PendingRead::FrameTree(target) => self.on_frame_tree(connection, target, outcome),
            PendingRead::Dom(target) => self.on_dom(connection, target, outcome),
            PendingRead::Ax { target, frame_id } => {
                self.on_ax(connection, (target, frame_id), outcome)
            }
            PendingRead::Owner { target, frame_id } => {
                let owner = match outcome {
                    Ok(reply) => reply.get("backendNodeId").and_then(Value::as_i64),
                    Err(error) if loses_only_the_frame(&error) => None,
                    Err(error) => return Err(error),
                };
                self.targets[target].owners.insert(frame_id.clone(), owner);
                self.classify(connection, target, &frame_id);
                Ok(())
            }
        }
    }

    fn on_auto_attach(
        &mut self,
        connection: &mut CdpConnection,
        target: usize,
        outcome: Outcome,
    ) -> Result<(), AppError> {
        match outcome {
            Ok(_) => {}
            Err(error) if target == 0 => return Err(error),
            Err(_) => return Ok(()),
        }
        let parent = self.targets[target].session.clone();
        for child in take_attached_children(connection, &parent) {
            if child.waiting {
                if let Ok(id) = connection.send(
                    "Runtime.runIfWaitingForDebugger",
                    json!({}),
                    Some(&child.session.session_id),
                    self.budget.snapshot(),
                ) {
                    connection.abandon(id);
                }
            }
            if self.budget.is_spent(self.page_done) {
                let id = child.session.target_id.clone();
                self.leave_out(&id, child.url, FrameSkipReason::TimedOut);
                continue;
            }
            let frame_id = child.session.target_id.clone();
            let state = TargetState::new(
                child.session,
                Some((target, child.parent_frame_id)),
                child.url,
            );
            self.targets.push(state);
            let index = self.targets.len() - 1;
            self.start_target(connection, index)?;
            let owner = PendingRead::Owner {
                target,
                frame_id: frame_id.clone(),
            };
            self.send(
                connection,
                "DOM.getFrameOwner",
                json!({ "frameId": frame_id }),
                owner,
            )?;
        }
        Ok(())
    }

    fn on_frame_tree(
        &mut self,
        connection: &mut CdpConnection,
        target: usize,
        outcome: Outcome,
    ) -> Result<(), AppError> {
        let frames = match outcome.and_then(|reply| parse_frame_tree(&reply)) {
            Ok(frames) => frames,
            Err(error) if target == 0 || !loses_only_the_frame(&error) => return Err(error),
            Err(_) => {
                self.fail_target(connection, target, FrameSkipReason::Unreadable);
                return Ok(());
            }
        };
        let inner: Vec<String> = frames
            .iter()
            .skip(1)
            .map(|frame| frame.frame_id.clone())
            .collect();
        self.targets[target].frames = Some(frames);
        for frame_id in inner {
            if self.budget.is_spent(self.page_done) {
                let url = self.targets[target].frame_url(&frame_id);
                self.leave_out(&frame_id, url, FrameSkipReason::TimedOut);
                continue;
            }
            let params = json!({ "frameId": frame_id });
            let ax = PendingRead::Ax {
                target,
                frame_id: frame_id.clone(),
            };
            self.send(
                connection,
                "Accessibility.getFullAXTree",
                params.clone(),
                ax,
            )?;
            let owner = PendingRead::Owner { target, frame_id };
            self.send(connection, "DOM.getFrameOwner", params, owner)?;
        }
        Ok(())
    }

    fn on_dom(
        &mut self,
        connection: &mut CdpConnection,
        target: usize,
        outcome: Outcome,
    ) -> Result<(), AppError> {
        let dom = match outcome {
            Ok(capture) => FrameDom::from_capture(&capture),
            Err(error) if is_protocol_error(&error) => FrameDom::default(),
            Err(error) if target == 0 || !loses_only_the_frame(&error) => return Err(error),
            Err(_) => {
                self.fail_target(connection, target, FrameSkipReason::Unreadable);
                return Ok(());
            }
        };
        self.targets[target].dom = Some(dom);
        let owned: Vec<String> = self.targets[target].owners.keys().cloned().collect();
        for frame_id in owned {
            self.classify(connection, target, &frame_id);
        }
        Ok(())
    }

    fn on_ax(
        &mut self,
        connection: &mut CdpConnection,
        (target, frame_id): (usize, String),
        outcome: Outcome,
    ) -> Result<(), AppError> {
        let error = match outcome {
            Ok(reply) => {
                let nodes = parse_nodes(reply)?;
                self.targets[target].ax.insert(frame_id, nodes);
                return Ok(());
            }
            Err(error) => error,
        };
        let root = frame_id.is_empty();
        if target == 0 && (root || !is_protocol_error(&error)) {
            return Err(error);
        }
        if !loses_only_the_frame(&error) {
            return Err(error);
        }
        if root {
            self.fail_target(connection, target, FrameSkipReason::Unreadable);
        } else {
            let url = self.targets[target].frame_url(&frame_id);
            self.leave_out(&frame_id, url, FrameSkipReason::Unreadable);
        }
        Ok(())
    }

    /// Leaves out `frame_id`, whose owner element lives in `target`, when
    /// the owner's box shows the frame can hold nothing; once both the owner
    /// and the capture of `target` are known.
    fn classify(&mut self, connection: &mut CdpConnection, target: usize, frame_id: &str) {
        if self.closed.contains(frame_id) {
            return;
        }
        let state = &self.targets[target];
        let Some(Some(backend)) = state.owners.get(frame_id) else {
            return;
        };
        let Some(boxes) = state.dom.as_ref().and_then(|dom| dom.boxes.as_ref()) else {
            return;
        };
        let Some(reason) = boxes.skip_reason(*backend) else {
            return;
        };
        let child = (1..self.targets.len())
            .find(|index| self.targets[*index].session.target_id == frame_id);
        match child {
            Some(child) => self.fail_target(connection, child, reason),
            None => {
                let url = self.targets[target].frame_url(frame_id);
                self.leave_out(frame_id, url, reason);
                let frame = frame_id.to_owned();
                self.abandon_where(connection, |read| {
                    matches!(read, PendingRead::Ax { target: t, frame_id: f } if *t == target && *f == frame)
                });
            }
        }
    }
}
