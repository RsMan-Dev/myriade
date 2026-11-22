//! Opening and closing a local world: entering it on the server thread, and saving and leaving
//! it on exit.
use super::*;

impl Game<'_> {
    /// Load authoritative world state synchronously, without client entry or
    /// integrated-server selection: a test-only convenience over the same two
    /// steps production startup calls separately (`prepare_world_and_rules`,
    /// then `finish_open_world`).
    #[cfg(test)]
    pub(super) fn load_world(&mut self, request: world_loading::WorldRequest) -> Result<()> {
        let prepared = self.prepare_world_and_rules(request)?;
        self.finish_open_world(prepared)
    }
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
