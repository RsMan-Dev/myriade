//! The server's data packs: the vanilla root they read from, and the reload of their converted files (`/reload`).
//! The client's resource pack selection and reload left with the client.
use super::Game;
use horizon_client::datapack::DataPack;
use mod_api::{__private::reload::ReloadFiles, FileToIdConverter, Identifier};
use std::path::PathBuf;

impl Game<'_> {
    pub(super) fn vanilla_root(&self) -> PathBuf {
        if self.session.vanilla_root.as_os_str().is_empty() {
            self.options.resources.clone()
        } else {
            self.session.vanilla_root.clone()
        }
    }
}

/// Reads one converter's directory from `packs`, later packs winning, and
/// collects the converted files: the shared read-and-log loop of the data reloads.
/// `top` is `"data"` or `"assets"`.
pub(super) fn reload_files(
    top: &str,
    packs: &[&DataPack],
    listener: &Identifier,
    converter: &FileToIdConverter,
) -> ReloadFiles {
    let mut selected = std::collections::BTreeMap::new();
    for pack in packs {
        for file in pack.list_under(top, &[converter.directory()]) {
            if let Some(id) = file
                .strip_prefix(&format!("{top}/"))
                .and_then(|path| converter.file_to_id(path))
            {
                selected.insert(id, (*pack, file));
            }
        }
    }
    let mut files = std::collections::BTreeMap::new();
    for (id, (pack, path)) in selected {
        // A file that vanishes or fails to read between listing and reading is one bad
        // entry, as vanilla's `scanDirectory` treats a parse failure: log it, skip it.
        match pack.read(&path) {
            Some(bytes) => {
                files.insert(id, bytes);
            }
            None => log::error!(
                "Couldn't read file '{id}' from '{}/{}' for listener {}",
                pack.id,
                path,
                listener
            ),
        }
    }
    ReloadFiles::new(files)
}
