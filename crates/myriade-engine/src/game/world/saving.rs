//! The save screen freezes simulation, but keeps pumping window events while
//! snapshots are prepared in small batches and the region workers drain.
use super::Game;
use myriade_engine::Result;
use std::{
    collections::{HashSet, VecDeque},
    time::{Duration, Instant},
};

pub(super) struct ClosingSave {
    pub quit: bool,
    close: bool,
    reply: Option<std::sync::mpsc::Sender<String>>,
    started: Instant,
    stage: Stage,
}
enum Stage {
    Generation,
    Chunks(VecDeque<(i32, i32)>),
    Entities(VecDeque<(i32, i32)>),
    Writes,
    Server,
    Quit,
}

impl Game<'_> {
    /// Save the level's chunk tickets: `/forceload`'s in vanilla's `chunk_tickets.dat`.
    pub(super) fn save_chunk_tickets(&self) -> Result<()> {
        self.save
            .save_chunk_tickets(self.streaming.commanded(), self.tickets.portal_tickets())
    }
    pub(super) fn begin_closing_save(&mut self, quit: bool) -> Result<()> {
        if let Some(save) = &mut self.pending_save {
            save.quit |= quit;
            return Ok(());
        }
        if self.saved_for_exit {
            // A save already made still needs the event loop to finish.
            self.pending_save = Some(ClosingSave {
                quit,
                close: true,
                reply: None,
                started: Instant::now(),
                stage: Stage::Quit,
            });
            return Ok(());
        }

        // Stop terrain work before the server is shut down: the server's own stop path aborts
        // its workers, but the streaming registration is dropped only after the event loop exits.
        self.streaming.abort_generation();
        self.close_container();
        self.award_custom_stat("leave_game", 1);
        self.lan_close(false);
        self.free_player_beds()?;
        self.pending_save = Some(ClosingSave {
            quit,
            close: true,
            reply: None,
            started: Instant::now(),
            stage: Stage::Generation,
        });
        Ok(())
    }

    /// Control saves acknowledge only after durable completion, while their
    /// snapshot work is spread across event-loop turns too.
    pub(super) fn begin_control_save(
        &mut self,
        all: bool,
        reply: std::sync::mpsc::Sender<String>,
    ) -> Result<()> {
        if self.pending_save.is_some() {
            return Err("a world save is already in progress".into());
        }
        if all {
            self.world
                .modified
                .extend(self.world.chunks.keys().copied());
        }
        self.left = false;
        self.right = false;
        self.pending_save = Some(ClosingSave {
            quit: false,
            close: false,
            reply: Some(reply),
            started: Instant::now(),
            stage: Stage::Generation,
        });
        Ok(())
    }
}

impl Game<'_> {
    /// Save the level in place: its changed chunks, its entities and its raids.
    pub(super) fn save_level(&mut self, wait: bool) -> Result<()> {
        self.admit_level_save()?;
        self.save_border();
        self.save_chunk_tickets()?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match self.streaming.prepare_save(&mut self.world, wait, deadline) {
                myriade_engine::streaming::SaveStep::Full(chunk) => {
                    self.native_late_full_chunk_snapshot(&chunk);
                    self.streaming.save_finished(chunk);
                }
                myriade_engine::streaming::SaveStep::Protos(protos) => {
                    self.native_proto_snapshots(&protos);
                    self.streaming.finish_save(protos, wait);
                    break;
                }
                myriade_engine::streaming::SaveStep::Pending => unreachable!(),
            }
        }
        let keys: Vec<_> = self.world.modified.iter().copied().collect();
        self.save.save_attachments(&self.world.attachments)?;
        for key in &keys {
            self.world.modified.remove(key);
        }
        let chunks = self.with_entities(keys.iter().filter_map(|k| self.world.chunks.get(k)));
        self.native_full_chunk_snapshots(&chunks);
        self.save
            .save_later(&chunks, self.pack.assets.registry, world_biomes!(self))?;
        // Prepare entities while the chunk writer runs, then use one barrier.
        if !wait {
            self.world.modified.autosaved(&keys, Instant::now());
        }
        self.save_entities(false)?;
        self.save_raids();
        if wait {
            self.save.flush()?;
        }
        Ok(())
    }
}

impl Game<'_> {
    pub(in crate::game) fn block_entity_tag(&self, pos: [i32; 3]) -> Option<horizon_core::nbt::Tag> {
        let e = self.world.block_entities.get(&(pos[0], pos[1], pos[2]))?;
        let name = self.data_block_name(pos);
        let mut tag =
            myriade_engine::save::block_entity_encode::encode((pos[0], pos[1], pos[2]), e, &name);
        if let horizon_core::nbt::Tag::Compound(c) = &mut tag {
            for key in ["id", "x", "y", "z", "keepPacked"] {
                c.remove(&key.into());
            }
        }
        Some(tag)
    }
}
