use crate::*;
use std::collections::{BTreeMap, HashMap};
use std::sync::{LazyLock, RwLock};

#[derive(Clone, Default)]
pub struct Names {
    pub entries: Vec<Request>,
    pub slots: HashMap<String, u32>,
    /// Typed identifier lookup without formatting a string on metadata reads.
    pub identifiers: BTreeMap<horizon_core::identifier::Identifier, u32>,
    pub utf16_slots: HashMap<Vec<u16>, u32>,
    pub world_fluids: Vec<Option<u32>>,
    /// Slots in the completed registry's registration order, when a completion
    /// recorded one. Empty while the table holds only mod requests.
    pub order: Vec<u32>,
}

#[derive(Clone)]
pub struct Request {
    pub id: String,
    pub mods: Vec<&'static str>,
}

impl Names {
    pub fn request(&mut self, mod_id: &'static str, id: &str) -> Result<u32, String> {
        // Match the registry boundary's identifier classification, including the default namespace.
        let id = if id.contains(':') {
            id.to_owned()
        } else {
            format!("minecraft:{id}")
        };
        let identifier = horizon_core::identifier::Identifier::parse_namespaced(&id)
            .ok_or_else(|| format!("mod {mod_id}: invalid registry key {id:?}"))?;
        self.request_parsed(mod_id, id, identifier)
    }

    /// Request a name already validated by the owning identifier boundary.
    pub fn request_identifier(
        &mut self,
        mod_id: &'static str,
        identifier: &horizon_core::identifier::Identifier,
    ) -> Result<u32, String> {
        self.request_parsed(mod_id, identifier.to_string(), identifier.clone())
    }

    fn request_parsed(
        &mut self,
        mod_id: &'static str,
        id: String,
        identifier: horizon_core::identifier::Identifier,
    ) -> Result<u32, String> {
        let slot = if let Some(slot) = self.slots.get(&id) {
            *slot
        } else {
            let slot = u32::try_from(self.entries.len())
                .map_err(|_| "registry key slots exhausted".to_owned())?;
            self.slots.insert(id.clone(), slot);
            self.entries.push(Request {
                id,
                mods: Vec::new(),
            });
            slot
        };
        self.identifiers.insert(identifier, slot);
        let mods = &mut self.entries[slot as usize].mods;
        if !mods.contains(&mod_id) {
            mods.push(mod_id);
        }
        Ok(slot)
    }

    pub fn id(&self, slot: u32) -> Option<&str> {
        self.entries
            .get(slot as usize)
            .map(|entry| entry.id.as_str())
    }

    /// Complete registry identities without changing requested slots. Publish the returned
    /// table only after every forward reference has resolved.
    pub fn complete<'a, E>(
        &self,
        ids: impl IntoIterator<Item = &'a str>,
        unknown: impl FnMut(&'static str, &str) -> E,
    ) -> Result<Self, Vec<E>> {
        let all: Vec<_> = ids.into_iter().collect();
        let known: std::collections::BTreeSet<_> = all.iter().copied().collect();
        self.resolve(|id| known.contains(id).then_some(()), unknown)?;
        let mut names = self.clone();
        // Slots keep their historical sorted completion order, so adding an
        // entry to one registry generation never renumbers another.
        for id in &known {
            names
                .request("horizon", id)
                .expect("registry owns valid ids");
        }
        let mut seen = std::collections::BTreeSet::new();
        names.order = all
            .into_iter()
            .filter(|id| seen.insert(*id))
            .map(|id| {
                *names
                    .slots
                    .get(id)
                    .expect("completed registry owns every id")
            })
            .collect();
        Ok(names)
    }

    pub fn resolve<T, E>(
        &self,
        mut resolve: impl FnMut(&str) -> Option<T>,
        mut unknown: impl FnMut(&'static str, &str) -> E,
    ) -> Result<Vec<T>, Vec<E>> {
        let mut values = Vec::with_capacity(self.entries.len());
        let mut errors = Vec::new();
        for entry in &self.entries {
            match resolve(&entry.id) {
                Some(value) => values.push(value),
                None => errors.extend(entry.mods.iter().map(|mod_id| unknown(mod_id, &entry.id))),
            }
        }
        if errors.is_empty() {
            Ok(values)
        } else {
            Err(errors)
        }
    }
}

/// A particle's resolved engine-independent provider identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    /// An index supplied by the game's vanilla particle registry.
    Vanilla(u32),
    /// A mod's named sprite provider, addressed by its retained name.
    Custom,
}

// Tag names are process-lifetime identities; only membership belongs to reloadable data.
// Reuse the same slot and identifier tables as registered keys, without a per-world cache.
static TAG_NAMES: LazyLock<RwLock<Names>> = LazyLock::new(RwLock::default);

/// Borrow a retained tag name while its table is read-locked. No string is allocated.
pub fn with_tag_name<R>(slot: u32, read: impl FnOnce(Option<&str>) -> R) -> R {
    match TAG_NAMES.read() {
        Ok(names) => read(names.id(slot)),
        Err(_) => read(None),
    }
}

/// The vanilla dimensions, at the slots of `DimensionKey`'s constants.
const VANILLA_DIMENSIONS: [&str; 3] = [
    "minecraft:overworld",
    "minecraft:the_nether",
    "minecraft:the_end",
];

/// The dimensions past vanilla's the game has met, in slot order. Only grows: a key handed out
/// stays valid until exit, and both of a process's `Game`s number a dimension alike.
#[derive(Default)]
struct ExtraDimensions {
    names: Vec<&'static str>,
    slots: HashMap<&'static str, u32>,
}

static EXTRA_DIMENSIONS: LazyLock<RwLock<ExtraDimensions>> = LazyLock::new(RwLock::default);

/// The key of dimension `id`, numbering it when the game meets it for the first time.
pub fn dimension_slot(id: &str) -> u32 {
    if let Some(slot) = VANILLA_DIMENSIONS.iter().position(|name| *name == id) {
        return slot as u32;
    }
    let first = VANILLA_DIMENSIONS.len() as u32;
    if let Some(slot) = EXTRA_DIMENSIONS.read().unwrap().slots.get(id) {
        return first + slot;
    }
    let mut extra = EXTRA_DIMENSIONS.write().unwrap();
    if let Some(slot) = extra.slots.get(id) {
        return first + slot;
    }
    let slot = extra.names.len() as u32;
    let name: &'static str = Box::leak(id.to_owned().into_boxed_str());
    extra.names.push(name);
    extra.slots.insert(name, slot);
    first + slot
}

/// The name of a dimension's key, None for a slot the game never gave out.
pub fn dimension_id(slot: u32) -> Option<&'static str> {
    let slot = slot as usize;
    VANILLA_DIMENSIONS.get(slot).copied().or_else(|| {
        let extra = EXTRA_DIMENSIONS.read().unwrap();
        extra.names.get(slot - VANILLA_DIMENSIONS.len()).copied()
    })
}
