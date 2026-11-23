//! The receiving turns: each guest's queued input, in order, and physical input ownership.
use super::*;

impl Hub {
    /// Handle the players in this level's packets (`ServerGamePacketListenerImpl`
    /// for each), before the level's scheduled block ticks so a place landed in
    /// the same tick as a falling block's schedule still sees the support
    /// (arismoko/horizon#4510). `tick_level` then syncs what those packets (and the block ticks)
    /// changed.
    pub fn drain_packets(&mut self, ctx: &mut TickCtx) -> Vec<HostEvent> {
        let mut events = Vec::new();
        let conns: Vec<u32> = self
            .guests
            .iter()
            .filter(|g| self.here(g) || (g.held && self.watches(g)))
            .map(|g| g.conn)
            .collect();
        for conn in conns {
            let Some(i) = self.guests.iter().position(|g| g.conn == conn) else {
                continue;
            };
            // The I/O owner can close the physical endpoint between packets. Do not
            // acknowledge or apply later input while waiting for its Left event.
            if self.discard_closed_input(i) {
                continue;
            }
            if !self.here(&self.guests[i]) && !(self.guests[i].held && self.watches(&self.guests[i])) {
                continue;
            }
            let frozen = self.guests[i].handover.is_some() || self.guests[i].held;
            if frozen {
                self.keep_acknowledgements(i, &mut events);
            }
            let mut held = false;
            self.process_inbox(i, ctx, &mut events, true, &mut held);
            if !frozen && self.here(&self.guests[i]) {
                self.tick_floating(i, &mut events);
            }
        }
        events
    }

    pub(super) fn begin_payload_poll(&mut self) -> Vec<HostEvent> {
        std::mem::take(&mut self.pending_kicks)
    }
}

impl Hub {
    pub(super) fn discard_closed_input(&mut self, i: usize) -> bool {
        // The I/O owner can also close concurrently between packets.
        if !self.guests[i].left_reason.is_empty() {
            let discarded = self.guests.update(i, |g| std::mem::take(&mut g.inbox));
            drop(discarded);
            true
        } else {
            false
        }
    }
}
