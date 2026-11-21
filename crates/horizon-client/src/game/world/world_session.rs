//! Opening and closing a local world: entering it on the server thread, and saving and leaving
//! it on exit.
use super::*;
use horizon_client::save::level_mods;

impl Game<'_> {
    /// Enter a local world from the client: it runs on the client's server thread.
    pub(super) fn open_world(
        &mut self,
        folder: &str,
        name: Option<&str>,
        seed: Option<i64>,
        creative: bool,
        create: Option<&crate::game::world_options::WorldOptions>,
    ) -> Result<()> {
        self.open_world_as(folder, name, seed, creative, create, false)
    }

    /// A world the title listed (Play, or a native mod's request answered Yes): it opens only while the
    /// save still exists, and nothing is created for a folder that is gone.
    pub(super) fn open_existing_world(&mut self, folder: &str) -> Result<()> {
        self.open_world_as(folder, None, None, false, None, true)
    }

    /// `open_world`, with `existing_only` set for a save that must already be there (see
    /// `open_existing_world`). It is checked here, before anything is read or written, and again when a
    /// deferred entry resumes, since the save may have gone in between.
    pub(super) fn open_world_as(
        &mut self,
        folder: &str,
        name: Option<&str>,
        seed: Option<i64>,
        creative: bool,
        create: Option<&crate::game::world_options::WorldOptions>,
        existing_only: bool,
    ) -> Result<()> {
        let saves = std::env::current_dir()?.join("saves");
        if refuses_missing_save(&saves, folder, existing_only) {
            log::warn!("Horizon: \"{folder}\" is no longer a save; nothing opened");
            return Ok(());
        }
        world_loading::world_data_used();
        if self.defer_world_entry(PendingWorldEntry::Local {
            folder: folder.to_owned(),
            name: name.map(str::to_owned),
            seed,
            creative,
            create: create.cloned().map(Box::new),
            existing_only,
        })? {
            return Ok(());
        }
        // A world saved with mods that are not loaded now asks first (arismoko/horizon#3856), unless the player
        // already chose to open it anyway; the question reads the world and writes nothing.
        if create.is_none() {
            let dir = std::env::current_dir()?.join("saves").join(folder);
            if !level_mods::is_confirmed(&dir) {
                let missing = self.ask_missing_mods(&dir);
                if !missing.is_empty() {
                    for m in &missing {
                        log::warn!("Horizon: {}", level_mods::describe(m));
                    }
                }
            }
        }
        self.join_world_rest()?;
        // Create World's data packs, in the world before it opens.
        if let Some(dir) = create.and_then(|o| o.pack_dir.as_deref()) {
            let world = std::env::current_dir()?.join("saves").join(folder);
            let packs = create.map_or(&[][..], |o| &o.data_packs[..]);
            if let Err(e) = horizon_client::datapack::install(dir, &world, packs) {
                log::warn!("Horizon: data packs for {folder}: {e}");
            }
            let _ = std::fs::remove_dir_all(dir);
        }
        // The world's pack selection (`DataPackConfig`), for the pack stack
        // to read: the picked packs on, the feature packs left out off.
        // Only when `install` did not run (no Data Packs screen opened): it
        // writes the file itself, and its `disabled` must not be lost.
        if let Some(o) = create.filter(|o| o.pack_dir.is_none()) {
            let world = std::env::current_dir()?.join("saves").join(folder);
            if let Err(e) = std::fs::create_dir_all(&world) {
                log::warn!("Horizon: data packs for {folder}: {}: {e}", world.display());
            } else {
                let disabled: Vec<String> =
                    horizon_client::datapack::feature_packs(&self.vanilla_root())
                        .into_iter()
                        .map(|p| p.id)
                        .filter(|id| !o.data_packs.contains(id))
                        .collect();
                let json = serde_json::json!({"enabled": o.data_packs, "disabled": disabled});
                let file = world.join("datapacks.json");
                if let Err(e) = std::fs::write(&file, json.to_string()) {
                    log::warn!("Horizon: data packs for {folder}: {}: {e}", file.display());
                }
            }
        }
        // A new world opened without the Create screen still gets the game folder's packs.
        if create.is_none() {
            let game_dir = std::env::current_dir()?;
            let world = game_dir.join("saves").join(folder);
            if let Err(e) = horizon_client::datapack::install_new_world_packs(&game_dir, &world) {
                log::warn!("Horizon: data packs for {folder}: {e}");
            }
        }
        // IntegratedServer.initServer's line, which other launchers read in `latest.log` to
        // offer the world again.
        horizon_client::logging::write_as(
            "Server thread",
            log::Level::Info,
            &format!(
                "Starting integrated minecraft server version {}",
                horizon_core::VANILLA_VERSION
            ),
        );
        self.open_integrated(folder, name, seed, creative, create)
    }

    /// Load authoritative world state synchronously, without client entry or
    /// integrated-server selection: a test-only convenience over the same two
    /// steps production startup calls separately (`prepare_world_and_rules`,
    /// then `finish_open_world`).
    #[cfg(test)]
    pub(super) fn load_world(&mut self, request: world_loading::WorldRequest) -> Result<()> {
        let prepared = self.prepare_world_and_rules(request)?;
        self.finish_open_world(prepared)
    }

    pub(super) fn save_for_exit(&mut self) -> Result<()> {
        // Native application termination can bypass CloseRequested. Finish any
        // save screen synchronously here, before the process can go away.
        while self.pending_save.is_some() {
            self.poll_closing_save()?;
            if self.pending_save.is_some() {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        if self.saved_for_exit {
            return Ok(());
        }

        // Native application termination can bypass the close request, so make the same
        // client-side worker abort explicit before the game-owned streaming state is dropped.
        self.streaming.abort_generation();
        if let Some(server) = self.integrated.take() {
            // The server thread saves the world.
            server.stop()?;
            self.saved_for_exit = true;
            return Ok(());
        }
        self.close_container();
        {
            self.award_custom_stat("leave_game", 1);
        }
        self.lan_close(false);
        self.free_player_beds()?;
        self.save_game()?;
        self.saved_for_exit = true;
        Ok(())
    }

    pub(super) fn close_world(&mut self) -> Result<()> {
        self.destroying.clear();
        self.post_effects.clear();
        self.spectated_post_effect = None;
        self.begin_closing_save(false)
    }
}

/// Whether an open must refuse `saves/<folder>` because it must be an existing save and is not one: nothing
/// is read or written for it. A save is one the title's list recognizes (`is_save_folder`), so an empty
/// folder is not one either; a link out of `saves/` is left to Play's symlink alert, and a mod's request
/// checks containment itself (`world_request_entry`).
fn refuses_missing_save(saves: &std::path::Path, folder: &str, existing_only: bool) -> bool {
    let dir = saves.join(folder);
    existing_only && !(dir.join("level.dat").is_file() || dir.join("level.dat_old").is_file())
}

#[cfg(test)]
mod existing_save_tests {
    use super::refuses_missing_save;
    use std::path::{Path, PathBuf};

    /// Every entry under `root`, with its bytes, so a change to any file or folder shows.
    fn tree(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let mut entries: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            entries.sort();
            for path in entries {
                let bytes = if path.is_dir() {
                    None
                } else {
                    Some(std::fs::read(&path).unwrap())
                };
                if path.is_dir() {
                    stack.push(path.clone());
                }
                out.push((path, bytes));
            }
        }
        out
    }

    #[test]
    fn a_save_that_is_gone_is_refused_and_the_saves_folder_is_left_as_it_was() {
        let root = std::env::temp_dir().join(format!("existing-save-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let saves = root.join("saves");
        std::fs::create_dir_all(saves.join("empty")).unwrap();
        std::fs::create_dir_all(saves.join("other")).unwrap();
        std::fs::write(saves.join("other").join("notes.txt"), b"keep").unwrap();
        let before = tree(&saves);
        // Missing, and present but without level.dat (an empty folder): both refused, and nothing created.
        assert!(refuses_missing_save(&saves, "gone", true));
        assert!(refuses_missing_save(&saves, "empty", true));
        assert_eq!(tree(&saves), before);
        assert!(!saves.join("gone").exists());
        // The flag off keeps an open's behaviour for a folder that is not a save: nothing refused here.
        assert!(!refuses_missing_save(&saves, "gone", false));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
