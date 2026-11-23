//! Worldgen overrides of the vanilla data: biome modifiers over the data
//! packs, and the carver, biome and structure tables they read.
//!
//! The vanilla root is the base root itself: nothing here writes a content
//! root, and with no modifier saved the packs stand as they are.
use crate::Result;
use serde::{Deserialize, Serialize, de};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq)]
enum J {
    Null,
    Bool(bool),
    Num(serde_json::Number),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}
impl<'de> Deserialize<'de> for J {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> de::Visitor<'de> for V {
            type Value = J;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON")
            }
            fn visit_unit<E>(self) -> std::result::Result<J, E> {
                Ok(J::Null)
            }
            fn visit_bool<E>(self, v: bool) -> std::result::Result<J, E> {
                Ok(J::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> std::result::Result<J, E> {
                Ok(J::Num(v.into()))
            }
            fn visit_u64<E>(self, v: u64) -> std::result::Result<J, E> {
                Ok(J::Num(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<J, E> {
                serde_json::Number::from_f64(v)
                    .map(J::Num)
                    .ok_or_else(|| E::custom("not a JSON number"))
            }
            fn visit_str<E>(self, v: &str) -> std::result::Result<J, E> {
                Ok(J::Str(v.to_owned()))
            }
            fn visit_string<E>(self, v: String) -> std::result::Result<J, E> {
                Ok(J::Str(v))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<J, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = a.next_element()? {
                    out.push(v);
                }
                Ok(J::Arr(out))
            }
            fn visit_map<A: de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<J, A::Error> {
                let mut out = Vec::new();
                while let Some(kv) = a.next_entry()? {
                    out.push(kv);
                }
                Ok(J::Obj(out))
            }
        }
        d.deserialize_any(V)
    }
}
impl Serialize for J {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            J::Null => s.serialize_unit(),
            J::Bool(b) => s.serialize_bool(*b),
            J::Num(n) => n.serialize(s),
            J::Str(t) => s.serialize_str(t),
            J::Arr(a) => {
                let mut seq = s.serialize_seq(Some(a.len()))?;
                for v in a {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            J::Obj(o) => {
                let mut map = s.serialize_map(Some(o.len()))?;
                for (k, v) in o {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
        }
    }
}
impl J {
    fn get(&self, key: &str) -> Option<&J> {
        match self {
            J::Obj(o) => o.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn remove(&mut self, key: &str) {
        if let J::Obj(o) = self {
            o.retain(|(k, _)| k != key);
        }
    }
    fn get_mut(&mut self, key: &str) -> Option<&mut J> {
        match self {
            J::Obj(o) => o.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    /// Sets `key`, in place if present, else at the end.
    fn set(&mut self, key: &str, value: J) {
        if let Some(v) = self.get_mut(key) {
            *v = value;
        } else if let J::Obj(o) = self {
            o.push((key.to_owned(), value));
        }
    }
    /// The value at a `/`-separated path ("" is the document).
    fn at(&mut self, path: &str) -> Option<&mut J> {
        path.split('/')
            .filter(|p| !p.is_empty())
            .try_fold(self, |j, key| j.get_mut(key))
    }
    fn f64(v: f64) -> J {
        serde_json::Number::from_f64(v).map_or(J::Null, J::Num)
    }
    fn as_u32(&self) -> Option<u32> {
        match self {
            J::Num(n) => n.as_u64().and_then(|v| u32::try_from(v).ok()),
            _ => None,
        }
    }
    fn as_str(&self) -> Option<&str> {
        match self {
            J::Str(s) => Some(s),
            _ => None,
        }
    }
    /// Renames strings and, with `ids`, moves state ids, everywhere inside.
    fn rename(&mut self, r: &Rename, ids: bool) {
        match self {
            J::Str(s) => {
                if let Some(new) = r.string(s) {
                    *s = new;
                }
            }
            J::Num(n) if ids => {
                if let Some(new) = n
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .and_then(|id| r.id(id))
                {
                    *n = new.into();
                }
            }
            J::Arr(a) => a.iter_mut().for_each(|v| v.rename(r, ids)),
            J::Obj(o) => o.iter_mut().for_each(|(_, v)| v.rename(r, ids)),
            _ => {}
        }
    }
}

/// One id moving to another: `minecraft:iron_block` → `ruby:ruby_block`,
/// with its state range for blocks.
fn read_json(path: &Path) -> Result<J> {
    Ok(serde_json::from_slice(&fs::read(path)?).map_err(|e| format!("{}: {e}", path.display()))?)
}

pub use horizon_items::content_namespaces::{namespaces, tag_namespaces};
/// Block states a world can hold. Chunk sections widen their global palette
/// past vanilla's 16 bits to fit the registry (crate::chunk::set_block_states);
/// this caps it at 20 bits, about 29 times vanilla's 35,723 states. Mods past
/// it are refused, not wrapped onto other blocks.
const BIOME_TAGS: &str = "worldgen/biome_tags.json";
/// Worldgen registries a content mod may add to, and the bundle the
/// worldgen loader reads each from (id → the file's text): what structure
/// mods hold, and a mod's own biomes (never vanilla's: `worldgen_entry`) and
/// the carvers they name. Noise and density functions change the terrain and
/// are not read from mods.
const BIOMES: &str = "worldgen/biomes.json";
const FEATURE_TAGS: &str = "worldgen/feature_tags.json";
const PLACED_FEATURES: &str = "worldgen/placed_features.json";
const CARVERS: &str = "worldgen/carvers.json";

/// Rewrites the biome bundle with the mods' biome modifiers. `biome_doc`,
/// `biome_tags` and `placed` are the bundles as the mods left them (`None`: the base's).
fn place_in_biomes(
    base: &Path,
    overlay: &Overlay<'_>,
    modifiers: &[biomes::Modifier],
    enabled_mods: Vec<String>,
    biome_doc: Option<J>,
    biome_tags: Option<J>,
    placed: Option<J>,
    tags: &BTreeMap<String, TagMerge>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let bundle = |doc: Option<J>, name: &str| -> Result<BTreeMap<String, String>> {
        let doc = match doc {
            Some(doc) => doc,
            None if base.join(name).exists() => read_json(&base.join(name))?,
            None => return Ok(BTreeMap::new()),
        };
        let J::Obj(entries) = doc else {
            return Err(format!("{name}: not an object").into());
        };
        Ok(entries
            .into_iter()
            .filter_map(|(k, v)| match v {
                J::Str(text) => Some((k, text)),
                _ => None,
            })
            .collect())
    };
    if !base.join(BIOMES).exists() {
        return Err(format!("biome modifiers need {BIOMES}").into());
    }
    let mut doc = match biome_doc {
        Some(doc) => doc,
        None => read_json(&base.join(BIOMES))?,
    };
    let biome_tags = bundle(biome_tags, BIOME_TAGS)?;
    let placed: BTreeSet<String> = bundle(placed, PLACED_FEATURES)?.into_keys().collect();
    let feature_tags = bundle(None, FEATURE_TAGS)?;
    let (entity_categories, entity_tags) = modifier_entities(base, overlay, tags)?;
    // The carvers bundle with the mods' own carvers, written before this
    // (`WORLDGEN_BUNDLES`), so a modifier may add one of them.
    let mut carvers = out_path(overlay, CARVERS);
    if !carvers.exists() {
        carvers = base.join(CARVERS);
    }
    let carver_ids = biomes::carver_ids(&carvers)?;
    let mut saved = SavedModifiers {
        carver_ids,
        carver_tags: modifier_tags(base, overlay, tags, "worldgen/carver")?,
        enabled_mods,
        modifiers: modifiers.iter().map(|m| m.source.clone()).collect(),
        unmodified: BTreeMap::new(),
        feature_tags,
        entity_categories,
        entity_tags,
    };
    let before = doc.clone();
    for (id, text) in saved.apply(&mut doc, modifiers, &biome_tags, &placed, warnings)? {
        if let Some(J::Str(old)) = before.get(&id) {
            saved.unmodified.insert(id.clone(), old.clone());
        }
        if let Some((ns, path)) = id.split_once(':') {
            let file = format!("data/{ns}/worldgen/biome/{path}.json");
            overlay.write(&file, text.as_bytes())?;
        }
    }
    biomes::write_state(overlay, &saved, &doc, modifiers)
}

fn modifier_entities(
    base: &Path,
    overlay: &Overlay<'_>,
    tags: &BTreeMap<String, TagMerge>,
) -> Result<(BTreeMap<String, String>, BTreeMap<String, String>)> {
    let entities_path = out_path(overlay, "entity-types.json");
    let entity_categories: BTreeMap<String, String> = match entities_path.exists() {
        true => match read_json(&entities_path)? {
            J::Obj(entries) => entries
                .into_iter()
                .map(|(k, v)| {
                    let cat = v
                        .get("category")
                        .and_then(J::as_str)
                        .unwrap_or("misc")
                        .to_owned();
                    (k, cat)
                })
                .collect(),
            _ => return Err("entity-types.json: not an object".into()),
        },
        false => BTreeMap::new(),
    };
    let entity_tags = modifier_tags(base, overlay, tags, "entity_type")?;
    Ok((entity_categories, entity_tags))
}

fn modifier_tags(
    base: &Path,
    overlay: &Overlay<'_>,
    tags: &BTreeMap<String, TagMerge>,
    registry: &str,
) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for ns in fs::read_dir(base.join("data"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let dir = ns.path().join("tags").join(registry);
        if !dir.is_dir() {
            continue;
        }
        for file in &horizon_world::json_folder::read(&dir)?.files {
            result.insert(
                format!("{}:{}", ns.file_name().to_string_lossy(), file.name),
                std::str::from_utf8(file.bytes()?)?.to_owned(),
            );
        }
    }
    let prefix = format!("/tags/{registry}/");
    for file in tags.keys() {
        if let Some((ns, path)) = file
            .strip_prefix("data/")
            .and_then(|s| s.split_once(&prefix))
            .and_then(|(ns, p)| Some((ns, p.strip_suffix(".json")?)))
        {
            result.insert(
                format!("{ns}:{path}"),
                fs::read_to_string(out_path(overlay, file))?,
            );
        }
    }
    Ok(result)
}

#[path = "mod_content_structures.rs"]
mod structures;

/// In a content root with biome modifiers: what applying them again over a
/// world's data packs takes (`biome_modifiers_over_packs`).
const MODIFIERS: &str = "biome-modifiers.json";

#[derive(Serialize, Deserialize)]
struct SavedModifiers {
    enabled_mods: Vec<String>,
    /// Each modifier's mod, file and text, in the order they apply.
    modifiers: Vec<[String; 3]>,
    /// The biomes the modifiers changed, as they were before.
    unmodified: BTreeMap<String, String>,
    feature_tags: BTreeMap<String, String>,
    entity_categories: BTreeMap<String, String>,
    entity_tags: BTreeMap<String, String>,
    #[serde(default)]
    carver_ids: BTreeSet<String>,
    #[serde(default)]
    carver_tags: BTreeMap<String, String>,
}

impl SavedModifiers {
    fn apply(
        &self,
        doc: &mut J,
        modifiers: &[biomes::Modifier],
        biome_tags: &BTreeMap<String, String>,
        placed: &BTreeSet<String>,
        warnings: &mut Vec<String>,
    ) -> Result<Vec<(String, String)>> {
        let biome_ids: BTreeSet<String> = match &*doc {
            J::Obj(o) => o.iter().map(|(k, _)| k.clone()).collect(),
            _ => return Err(format!("{BIOMES}: not an object").into()),
        };
        let entity_ids: BTreeSet<String> = self.entity_categories.keys().cloned().collect();
        let registries = biomes::Registries::of(
            &biome_ids,
            biome_tags,
            placed,
            &self.feature_tags,
            (&entity_ids, &self.entity_tags),
            (&self.carver_ids, &self.carver_tags),
        );
        biomes::apply(
            doc,
            modifiers,
            &registries,
            &self.entity_categories,
            warnings,
        )
    }
}

/// A world's data packs (`packs`) with the content root `root`'s biome
/// modifiers applied over them. Modifiers apply once every data
/// pack is in, to the final biomes and tags: a pack's file for a biome
/// (Terralith's `minecraft:plains`) keeps the mods' features and spawns, and
/// a pack's biome its tags put under a modifier's selector gets them too.
/// `None` when the root's biomes stand: no modifiers, or nothing declared and packs that bring
/// no biome and no tag. `declared` are mods' code modifiers (mod, file, text), applied last.
pub fn biome_modifiers_over_packs(
    root: &Path,
    packs: &horizon_world::worldgen::overrides::WorldgenOverrides,
    declared: &[[String; 3]],
) -> Result<Option<horizon_world::worldgen::overrides::WorldgenOverrides>> {
    let Some(bytes) = biomes::saved_bytes(root, packs, declared) else {
        return Ok(None);
    };
    let mut saved: SavedModifiers =
        serde_json::from_slice(&bytes).map_err(|e| format!("{MODIFIERS}: {e}"))?;
    let read = |name: &str| -> Result<BTreeMap<String, String>> {
        match fs::read(root.join(name)) {
            Ok(b) => Ok(serde_json::from_slice(&b).map_err(|e| format!("{name}: {e}"))?),
            Err(_) => Ok(BTreeMap::new()),
        }
    };
    let mut warnings = Vec::new();
    let mut modifiers = Vec::new();
    for [m, file, text] in saved.modifiers.iter().chain(declared) {
        modifiers.extend(biomes::Modifier::read(m, file, text, &mut warnings)?);
    }
    // What the generator reads without the modifiers over the packs.
    let packed = packs.apply("biomes", read(BIOMES)?);
    let mut start = packed.clone();
    for (id, text) in &saved.unmodified {
        if packs.entries("biomes").all(|(p, _)| p != id) {
            start.insert(id.clone(), text.clone());
        }
    }
    let mut doc = J::Obj(start.into_iter().map(|(id, t)| (id, J::Str(t))).collect());
    // Pack tag files after the root's, as the worldgen loader merges them.
    let mut biome_tags = read(BIOME_TAGS)?;
    for (id, pack, json) in packs.tags("biome_tags") {
        let tag: J = match serde_json::from_str(json) {
            Ok(tag) => tag,
            Err(e) => {
                warnings.push(format!("{pack}: tag {id}: {e}"));
                continue;
            }
        };
        let (values, remove) = (tag_list(&tag, "values"), tag_list(&tag, "remove"));
        let old = match tag.get("replace") {
            Some(J::Bool(true)) => None,
            _ => biome_tags.get(id).map(String::as_str),
        };
        let layers = [load_conditions::TagLayers { values, remove }];
        let merged = merge_tag(
            old,
            &layers,
            id,
            &horizon_world::conditions::LoadConditions::new(
                saved.enabled_mods.iter().map(String::as_str),
            ),
        )?;
        biome_tags.insert(id.to_owned(), merged);
    }
    let placed = packs
        .apply("placed_features", read(PLACED_FEATURES)?)
        .into_keys()
        .collect();
    carver_registry::refresh(&mut saved, packs)?;
    saved.apply(&mut doc, &modifiers, &biome_tags, &placed, &mut warnings)?;
    for warning in warnings {
        log::warn!("Horizon: biome modifiers over data packs: {warning}");
    }
    let mut out = packs.clone();
    if let J::Obj(entries) = doc {
        for (id, text) in entries {
            if let J::Str(text) = text
                && packed.get(&id) != Some(&text)
            {
                out.set("biomes", &id, text);
            }
        }
    }
    Ok(Some(out))
}

#[path = "mod_content_carvers.rs"]
mod carver_registry;

#[path = "mod_content_biomes.rs"]
pub mod biomes;

/// The overlay being written: `out` mirrors `base` through links, and a
/// directory becomes real only when something inside it is written.
struct Overlay<'a> {
    written: std::sync::Mutex<std::collections::BTreeSet<String>>,
    base: &'a Path,
    out: &'a Path,
}
impl Overlay<'_> {
    fn new<'a>(base: &'a Path, out: &'a Path) -> Result<Overlay<'a>> {
        if fs::symlink_metadata(out).is_ok() {
            fs::remove_dir_all(out)?;
        }
        fs::create_dir_all(out)?;
        let overlay = Overlay {
            base,
            out,
            written: Default::default(),
        };
        overlay.link_children(Path::new(""))?;
        Ok(overlay)
    }
    fn link_children(&self, rel: &Path) -> Result<()> {
        let Ok(read) = fs::read_dir(self.base.join(rel)) else {
            return Ok(());
        };
        for entry in read.flatten() {
            let rel = rel.join(entry.file_name());
            // The base's files in one file: it describes the base, not this root (and
            // would be copied where links are not made).
            if rel == Path::new(horizon_core::prepared_files::FILE_NAME) {
                continue;
            }
            link(&self.base.join(&rel), &self.out.join(&rel))?;
        }
        Ok(())
    }
    /// Makes `rel`'s directories real, each holding links to the base's.
    fn dir(&self, rel: &Path) -> Result<()> {
        let mut at = PathBuf::new();
        for part in rel.iter() {
            at.push(part);
            let path = self.out.join(&at);
            let meta = fs::symlink_metadata(&path);
            if meta
                .as_ref()
                .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
            {
                continue;
            }
            if meta.is_ok() {
                // A junction (Windows) is removed as a directory, its target untouched.
                #[cfg(windows)]
                fs::remove_dir(&path)?;
                #[cfg(not(windows))]
                fs::remove_file(&path)?;
            }
            fs::create_dir(&path)?;
            self.link_children(&at)?;
        }
        Ok(())
    }
    fn write(&self, rel: &str, bytes: &[u8]) -> Result<()> {
        let name = rel;
        let rel = Path::new(rel);
        if let Some(parent) = rel.parent() {
            self.dir(parent)?;
        }
        let path = self.out.join(rel);
        if fs::symlink_metadata(&path).is_ok() {
            fs::remove_file(&path)?;
        }
        fs::write(path, bytes)?;
        self.written
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(name.to_owned());
        Ok(())
    }
}

fn out_path(overlay: &Overlay<'_>, rel: &str) -> PathBuf {
    overlay.out.join(rel)
}
#[cfg(unix)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}
#[cfg(not(unix))]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    if target.is_dir() {
        fs::create_dir_all(at)?;
        for entry in fs::read_dir(target)?.flatten() {
            link(&entry.path(), &at.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        link_file(target, at, |target, at| fs::hard_link(target, at))
    }
}
/// A file of the overlay where symlinks need a privilege: a hard link to the base's file, which costs a name where
/// a copy costs the file's bytes (the base is ~28,000 files, 0.6 GiB), or a copy when no link can be made (another
/// volume, a filesystem without links, the NTFS limit on links to one file). A link shares the base file's data,
/// which is safe because the overlay is only ever replaced name by name (`Overlay::write` removes the name before it
/// writes), never written in place. A symlink is copied through, as before: linked, it would stay a symlink.
#[cfg(any(not(unix), test))]
fn link_file(
    target: &Path,
    at: &Path,
    hard_link: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let symlink = fs::symlink_metadata(target).is_ok_and(|m| m.file_type().is_symlink());
    if !symlink && hard_link(target, at).is_ok() {
        return Ok(());
    }
    fs::copy(target, at).map(|_| ())
}

mod load_conditions;
use load_conditions::{TagLayers, TagMerge, merge_tag, tag_list};
pub const BLOCK_ENTITY_TYPES: &str = "horizon-block-entity-types.json";

