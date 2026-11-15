use std::collections::VecDeque;

use super::cdp_event::CdpEvent;

const MAX_BUFFERED_EVENTS: usize = 1024;

/// The events a connection read while it waited for something else, oldest
/// first. Once it holds 1024, the oldest is dropped and counted.
#[derive(Debug, Default)]
pub(crate) struct EventBuffer {
    events: VecDeque<CdpEvent>,
    dropped: u64,
}

impl EventBuffer {
    pub(crate) fn push(&mut self, event: CdpEvent) {
        if self.events.len() >= MAX_BUFFERED_EVENTS {
            self.events.pop_front();
            self.dropped += 1;
        }
        self.events.push_back(event);
    }

    pub(crate) fn take_first(
        &mut self,
        matches: impl FnMut(&CdpEvent) -> bool,
    ) -> Option<CdpEvent> {
        let index = self.events.iter().position(matches)?;
        self.events.remove(index)
    }

    pub(crate) fn take_all(&mut self) -> Vec<CdpEvent> {
        self.events.drain(..).collect()
    }

    pub(crate) fn drain(&mut self, mut matches: impl FnMut(&CdpEvent) -> bool) -> Vec<CdpEvent> {
        let mut drained = Vec::new();
        self.events.retain(|event| {
            if matches(event) {
                drained.push(event.clone());
                return false;
            }
            true
        });
        drained
    }

    pub(crate) fn discard(&mut self, mut matches: impl FnMut(&CdpEvent) -> bool) {
        self.events.retain(|event| !matches(event));
    }

    pub(crate) fn dropped(&self) -> u64 {
        self.dropped
    }
}
