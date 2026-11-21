//! The tables the command side reads from the resources: the local player's name, the command
//! registries' export and the block properties `/setblock` completes.
use horizon_client::command::{Dispatcher, Registries};
use horizon_world::registry::{BlockRegistry, BlockStateId};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// The local player's name (offline single player).
pub const PLAYER_NAME: &str = "Player";

/// An owned command tree, initialized even before a world is opened.
pub struct SessionDispatcher(pub Arc<Dispatcher>);
impl Default for SessionDispatcher {
    fn default() -> Self {
        let registry = horizon_client::gamerules::RuleRegistry::vanilla();
        Self(Arc::new(
            horizon_client::command::vanilla::session_dispatcher(&registry, &[]),
        ))
    }
}

static COMMAND_EXPORTS: Mutex<Vec<(PathBuf, Option<Arc<Registries>>)>> = Mutex::new(Vec::new());

/// What the registries take from a folder's `command-registries.json`
/// (vanilla's built-in registries), read once: parsing its 1.6 MB again took
/// about 15 ms of every world opening. Only the part the registries fill from
/// stays, not the parsed export. A dedicated server opens its world once and
/// keeps nothing.
pub(in crate::game) fn command_export(dir: &Path, keep: bool) -> Option<Arc<Registries>> {
    let mut cache = COMMAND_EXPORTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, export)) = cache.iter().find(|(cached, _)| cached == dir) {
        return export.clone();
    }
    let export = std::fs::read(dir.join("command-registries.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .map(|export| Arc::new(Registries::from_export(&export)));
    if keep {
        cache.push((dir.to_owned(), export.clone()));
    }
    export
}

/// Every block's properties and their values, as `/setblock` completes them.
pub(in crate::game) type BlockProperties = BTreeMap<String, Vec<(String, Vec<String>)>>;
static BLOCK_PROPERTIES: Mutex<Vec<(usize, Arc<BlockProperties>)>> = Mutex::new(Vec::new());

/// `BlockProperties` of a block registry, listed once: a registry never
/// changes once built, and walking its tens of thousands of states took
/// about 30 ms of every world opening.
pub(in crate::game) fn block_properties(registry: &'static BlockRegistry) -> Arc<BlockProperties> {
    let key = std::ptr::from_ref(registry) as usize;
    let mut cache = BLOCK_PROPERTIES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, properties)) = cache.iter().find(|(cached, _)| *cached == key) {
        return properties.clone();
    }
    let mut blocks: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for id in 0..registry.len() {
        let Some(state) = registry.state(BlockStateId(id as u32)) else {
            continue;
        };
        let props = blocks.entry(state.name.clone()).or_default();
        for (k, v) in &state.properties {
            let values = props.entry(k.clone()).or_default();
            if !values.contains(v) {
                values.push(v.clone());
            }
        }
    }
    let properties: Arc<BlockProperties> = Arc::new(
        blocks
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect(),
    );
    cache.push((key, properties.clone()));
    properties
}
