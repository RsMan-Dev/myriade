//! The save screen freezes simulation, but keeps pumping window events while
//! snapshots are prepared in small batches and the region workers drain.
use super::Game;
use horizon_client::Result;
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

// Bound the work in both sparse and block-entity-heavy worlds. A single chunk
// is indivisible; check the time budget after each snapshot, not each batch.
const SNAPSHOTS_PER_FRAME: usize = 16;
const SNAPSHOT_BUDGET: Duration = Duration::from_millis(2);

impl Game<'_> {
    /// Save the level's chunk tickets: `/forceload`'s in vanilla's `chunk_tickets.dat`, the
    /// mods' in Horizon's own `mod_chunk_tickets.dat`.
    pub(super) fn save_chunk_tickets(&self) -> Result<()> {
        self.save
            .save_chunk_tickets(self.streaming.commanded(), self.tickets.portal_tickets())?;
        self.save
            .save_mod_tickets(self.streaming.mod_tickets(), |slot| {
                mod_api::TicketController::from_slot(slot).map(|c| c.id())
            })
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

        // Stop client-side terrain work before the integrated server is shut down. The
        // server's own stop path aborts its workers, but the window's streaming registration
        // is dropped only after the event loop exits and would otherwise wait for queued work.
        self.streaming.abort_generation();
        if let Some(integrated) = &self.integrated {
            integrated.request_stop();
            self.pending_save = Some(ClosingSave {
                quit,
                close: true,
                reply: None,
                started: Instant::now(),
                stage: Stage::Server,
            });
        } else {
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
        }
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

    /// `true` means the window can exit. The session lock stays in Game until
    /// every snapshot has been queued and every writer has synced its files.
    pub(super) fn poll_closing_save(&mut self) -> Result<bool> {
        let Some(mut save) = self.pending_save.take() else {
            return Ok(false);
        };
        let result = (|| -> Result<bool> {
            match &mut save.stage {
                Stage::Quit => Ok(true),
                Stage::Server => {
                    let server = self
                        .integrated
                        .as_ref()
                        .ok_or("the integrated server stopped with an error")?;
                    if !server.stopped() {
                        return Ok(false);
                    }
                    // Drop/join only after completion; leave_server's teardown
                    // can then run without blocking on the server's writes.
                    if save.quit {
                        self.integrated.take().unwrap().stop()?;
                    }
                    Ok(true)
                }
                Stage::Generation => {
                    if save.started.elapsed() > Duration::from_secs(30) {
                        return Err("chunk generation did not finish before saving".into());
                    }
                    let deadline = Instant::now() + Duration::from_millis(2);
                    loop {
                        match self.streaming.prepare_poll_save(&mut self.world, deadline) {
                            horizon_client::streaming::SaveStep::Pending => break,
                            horizon_client::streaming::SaveStep::Full(chunk) => {
                                self.native_late_full_chunk_snapshot(&chunk);
                                self.streaming.save_finished(chunk);
                            }
                            horizon_client::streaming::SaveStep::Protos(protos) => {
                                self.native_proto_snapshots(&protos);
                                if self.streaming.submit_protos(protos) {
                                    save.stage = Stage::Chunks(
                                        self.world.modified.iter().copied().collect(),
                                    );
                                }
                                break;
                            }
                        }
                    }
                    Ok(false)
                }
                Stage::Chunks(keys) => {
                    let started = Instant::now();
                    for _ in 0..SNAPSHOTS_PER_FRAME {
                        let Some(&key) = keys.front() else { break };
                        let chunks = self.with_entities(self.world.chunks.get(&key));
                        self.native_full_chunk_snapshots(&chunks);
                        self.save.save_later(
                            &chunks,
                            self.pack.assets.registry,
                            world_biomes!(self),
                        )?;
                        self.world.modified.remove(&key);
                        keys.pop_front();
                        if started.elapsed() >= SNAPSHOT_BUDGET {
                            break;
                        }
                    }
                    if keys.is_empty() {
                        self.spawn_loaded_entities();
                        save.stage = Stage::Entities(self.world.chunks.keys().copied().collect());
                    }
                    Ok(false)
                }
                Stage::Entities(keys) => {
                    let started = Instant::now();
                    for _ in 0..SNAPSHOTS_PER_FRAME {
                        let Some(&key) = keys.front() else { break };
                        let chunks = self.entity_chunks(&HashSet::from([key]), false);
                        self.save.save_entities_later(chunks)?;
                        keys.pop_front();
                        if started.elapsed() >= SNAPSHOT_BUDGET {
                            break;
                        }
                    }
                    if keys.is_empty() {
                        self.prune_entity_memory();
                        self.save_raids();
                        self.save.save_attachments(&self.world.attachments)?;
                        self.save_chunk_tickets()?;
                        self.save_game_metadata()?;
                        save.stage = Stage::Writes;
                    }
                    Ok(false)
                }
                Stage::Writes => {
                    let players = self.finish_player_autosaves(false)?;
                    Ok(self.save.writes_finished()? && players)
                }
            }
        })();
        match result {
            Ok(true) => {
                if let Some(reply) = save.reply.take() {
                    let _ = reply.send(serde_json::json!({ "ok": true }).to_string());
                }
                if save.quit && !save.close {
                    // A window close during a control save still has to close
                    // containers and save the resulting inventory changes.
                    self.begin_closing_save(true)?;
                    return Ok(false);
                }
                if save.quit {
                    self.saved_for_exit = true;
                    Ok(true)
                } else {
                    if save.close {
                        self.finish_close_world()?;
                    }
                    self.clock.reset();
                    self.previous = Instant::now();
                    Ok(false)
                }
            }
            other => {
                if let Err(error) = &other
                    && let Some(reply) = save.reply.take()
                {
                    let _ =
                        reply.send(serde_json::json!({ "error": error.to_string() }).to_string());
                }
                // Keep ownership (and the lock) on errors too. Native shutdown
                // sees the same durable-write failure instead of declaring success.
                self.pending_save = Some(save);
                other
            }
        }
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
                horizon_client::streaming::SaveStep::Full(chunk) => {
                    self.native_late_full_chunk_snapshot(&chunk);
                    self.streaming.save_finished(chunk);
                }
                horizon_client::streaming::SaveStep::Protos(protos) => {
                    self.native_proto_snapshots(&protos);
                    self.streaming.finish_save(protos, wait);
                    break;
                }
                horizon_client::streaming::SaveStep::Pending => unreachable!(),
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
        if mod_api::__private::listened(mod_api::HookId::LevelSave) {
            self.native_level_save(self.dim.id());
        }
        Ok(())
    }
}

impl Game<'_> {
    pub(in crate::game) fn block_entity_tag(&self, pos: [i32; 3]) -> Option<horizon_core::nbt::Tag> {
        let e = self.world.block_entities.get(&(pos[0], pos[1], pos[2]))?;
        let name = self.data_block_name(pos);
        let mut tag =
            horizon_client::save::block_entity_encode::encode((pos[0], pos[1], pos[2]), e, &name);
        if let horizon_core::nbt::Tag::Compound(c) = &mut tag {
            for key in ["id", "x", "y", "z", "keepPacked"] {
                c.remove(&key.into());
            }
        }
        Some(tag)
    }
}
