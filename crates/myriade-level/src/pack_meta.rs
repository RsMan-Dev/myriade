//! Which folders and zips of a data-pack folder are packs: their links checked against the game's
//! `allowed_symlinks.txt`, and their `pack.mcmeta` read. Only what the data-pack discovery needs.
use myriade_data::pattern::Pattern;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub enum Source {
    Dir(PathBuf),
    Zip(PathBuf),
    Opened(crate::resource_source::ResourceSource),
}

impl Source {
    /// Open this layer once for a load, sharing it across resource reads.
    pub fn open(&self) -> io::Result<crate::resource_source::ResourceSource> {
        match self {
            Self::Dir(root) => Ok(crate::resource_source::ResourceSource::directory(
                root.clone(),
            )),
            Self::Zip(path) => crate::resource_source::ResourceSource::archive(path),
            Self::Opened(source) => Ok(source.clone()),
        }
    }
}

/// The packs (folders and zips with a `pack.mcmeta`) in `dir` with their entry names, by name, as
/// `FolderRepositorySource` finds them.
pub fn folder(dir: &Path) -> Vec<(String, Source)> {
    let validator = DirectoryValidator::game();
    let mut found: Vec<(String, Source)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let path = match detected_pack_path(&e.path(), &validator) {
                Ok(Some(path)) => path,
                Ok(None) => return None,
                Err(err) => {
                    log::warn!(
                        "Failed to read properties of '{}', ignoring: {err}",
                        e.path().display()
                    );
                    return None;
                }
            };
            let source = if path.is_dir() {
                Source::Dir(path)
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
            {
                Source::Zip(path)
            } else {
                return None;
            };
            let bytes = source.open().ok()?.read("pack.mcmeta").ok()?;
            parse_meta(&bytes)?;
            Some((name, source))
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// `LevelStorageSource.ALLOWED_SYMLINKS_CONFIG_NAME`: the game folder's list of
/// symbolic link targets the game accepts.
pub const ALLOWED_SYMLINKS_CONFIG_NAME: &str = "allowed_symlinks.txt";

/// `DirectoryValidator` over vanilla's `PathAllowList`: which symbolic link
/// targets the game accepts. With no entry, no link is, so a link planted in a
/// pack folder cannot point the game at the rest of the disk.
#[derive(Default)]
pub struct DirectoryValidator {
    /// `EntryType.PREFIX`: a target whose text starts with the entry matches.
    prefixes: Vec<String>,
}
impl DirectoryValidator {
    /// `LevelStorageSource.parseValidator`: the entries of `config`, none when
    /// it is missing, unreadable or unparseable. `[glob]` and `[regex]` entries
    /// are skipped with a warning: Horizon has no Java path matcher to compile
    /// them with, so a skipped entry allows less than vanilla's, never more,
    /// except that vanilla denies every link when an entry fails to compile.
    pub fn parse(config: &Path) -> Self {
        let Ok(text) = fs::read_to_string(config) else {
            return Self::default();
        };
        let mut prefixes = Vec::new();
        for line in text.lines() {
            // `ConfigEntry.parse`: blank lines and comments, untrimmed otherwise.
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let Some(rest) = line.strip_prefix('[') else {
                prefixes.push(line.to_owned());
                continue;
            };
            let Some((kind, pattern)) = rest.split_once(']') else {
                // `ConfigEntry.parse` throws on an unterminated type, and
                // `parseValidator` then keeps no entry at all.
                log::error!(
                    "Horizon: {}: unterminated type in {line:?}",
                    config.display()
                );
                return Self::default();
            };
            match kind {
                "prefix" => prefixes.push(pattern.to_owned()),
                "glob" | "regex" => log::warn!(
                    "Horizon: {}: {kind} entry {pattern:?} is not supported and allows nothing",
                    config.display()
                ),
                _ => {
                    // An unsupported type throws in `parse`, as an unterminated
                    // one does: the whole file then allows no link.
                    log::error!(
                        "Horizon: {}: unsupported type in {line:?}",
                        config.display()
                    );
                    return Self::default();
                }
            }
        }
        Self { prefixes }
    }
    /// The game folder's list, as `Minecraft` reads it from
    /// `gameDirPath.resolve("allowed_symlinks.txt")`; the game folder is the
    /// working directory, as for `options.txt`.
    pub fn game() -> Self {
        Self::parse(
            &std::env::current_dir()
                .unwrap_or_default()
                .join(ALLOWED_SYMLINKS_CONFIG_NAME),
        )
    }
    /// `PathAllowList.matches`, on the target as written, not resolved.
    fn allows(&self, target: &Path) -> bool {
        target
            .to_str()
            .is_some_and(|t| self.prefixes.iter().any(|p| t.starts_with(p.as_str())))
    }
}
/// `DirectoryValidator.validateKnownDirectory`: every link under `dir`, never
/// following one, added to `issues` with its target unless the validator
/// allows the target.
fn forbidden_links(
    dir: &Path,
    validator: &DirectoryValidator,
    issues: &mut Vec<(PathBuf, PathBuf)>,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            let target = fs::read_link(&path)?;
            if !validator.allows(&target) {
                issues.push((path, target));
            }
        } else if kind.is_dir() {
            forbidden_links(&path, validator, issues)?;
        }
    }
    Ok(())
}
/// `FolderRepositorySource.discoverPacks`'s warning for a refused entry: the
/// entry, as vanilla names it, and every forbidden link in it.
fn refused(entry: &Path, issues: &[(PathBuf, PathBuf)]) {
    let list: Vec<String> = issues
        .iter()
        .map(|(link, target)| format!("{}->{}", link.display(), target.display()))
        .collect();
    log::warn!(
        "Ignoring potential pack entry: Failed to validate '{}'. Found forbidden symlinks: {}",
        entry.display(),
        list.join(", ")
    );
}
/// `PackDetector.detectPackResources`: the path a pack entry names, once the
/// validator accepts the entry and every link inside it; `None` when it is
/// missing or not a pack, `Err` when it cannot be read. A link is followed one
/// level, to its target as written rather than resolved, as vanilla does.
pub(crate) fn detected_pack_path(
    path: &Path,
    validator: &DirectoryValidator,
) -> io::Result<Option<PathBuf>> {
    let mut kind = match fs::symlink_metadata(path) {
        Ok(meta) => meta.file_type(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None), // `NoSuchFileException`
        Err(err) => return Err(err),
    };
    let mut target = path.to_path_buf();
    if kind.is_symlink() {
        let link = fs::read_link(path)?;
        if !validator.allows(&link) {
            refused(path, &[(path.to_path_buf(), link)]);
            return Ok(None);
        }
        target = link;
        // One level only: the attributes read on the target do not follow a
        // second link, and such an entry is neither a directory nor a file.
        kind = fs::symlink_metadata(&target)?.file_type();
        if kind.is_symlink() {
            return Ok(None);
        }
    }
    if kind.is_dir() {
        let mut issues = Vec::new();
        forbidden_links(&target, validator, &mut issues)?;
        if !issues.is_empty() {
            refused(path, &issues);
            return Ok(None);
        }
    }
    Ok(Some(target))
}

/// The first JSON value of a `pack.mcmeta`: what follows it is ignored, as some packs sign the
/// file with a comment after the closing brace. Gson reads past a UTF-8 byte order mark;
/// serde_json rejects it.
pub fn parse_meta(bytes: &[u8]) -> Option<serde_json::Value> {
    first_json(bytes).ok()
}
/// The first JSON value of a `.mcmeta` file, as `ResourceMetadata.fromJsonStream` reads it:
/// Gson's `TypeAdapter.read` stops after the value, so text after it (packs leave `//` notes
/// there) is never read, and a UTF-8 byte order mark is skipped.
pub(crate) fn first_json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> serde_json::Result<T> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match serde_json::Deserializer::from_slice(bytes)
        .into_iter()
        .next()
    {
        Some(value) => value,
        None => Err(serde::de::Error::custom("empty metadata")),
    }
}

/// A pack's assets as the packs loader layers them: its source, the overlay
/// directories active for this game, and its `filter.block` entries.
#[derive(Clone, Debug)]
pub struct Pack {
    /// `file/<name>` for user packs, the bare name for built-in ones.
    pub id: String,
    pub source: Source,
    /// Overlay directories applied after the base, in order.
    pub overlays: Vec<String>,
    /// `filter.block` entries hiding resources of the packs below this one.
    pub filter: Vec<Filter>,
}

impl Pack {
    /// A mod's asset layer, which has no resource-pack metadata.
    pub fn mod_layer(id: String, source: Source) -> Self {
        Self {
            id,
            source,
            overlays: Vec::new(),
            filter: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Filter {
    pub namespace: Option<Pattern>,
    pub path: Option<Pattern>,
}
impl Filter {
    fn blocks(&self, file: &str) -> bool {
        let Some(rest) = file.strip_prefix("assets/") else {
            return false;
        };
        let Some((namespace, path)) = rest.split_once('/') else {
            return false;
        };
        self.namespace
            .as_ref()
            .is_none_or(|p| p.is_match(namespace))
            && self.path.as_ref().is_none_or(|p| p.is_match(path))
    }
}

/// A file a pack provides: its path (folder packs) and its name inside the pack.
type Provided = (
    Option<PathBuf>,
    crate::resource_source::ResourceSource,
    String,
);
/// Files a pack provides under `assets/`, keyed by their resource name; active
/// overlay directories are applied after the base, in order, later wins.
fn pack_files(
    pack: &Pack,
    source: &crate::resource_source::ResourceSource,
) -> io::Result<BTreeMap<String, Provided>> {
    let mut out = BTreeMap::new();
    for root in std::iter::once(String::new()).chain(
        pack.overlays
            .iter()
            .map(|directory| format!("{directory}/")),
    ) {
        let source = source.subdirectory(root.trim_end_matches('/'))?;
        for inner in source.list("assets/") {
            let path = match &pack.source {
                Source::Dir(directory) => Some(directory.join(&root).join(&inner)),
                Source::Zip(_) | Source::Opened(_) => None,
            };
            out.insert(inner.clone(), (path, source.clone(), inner));
        }
    }
    Ok(out)
}

struct ResolvedPacks {
    files: BTreeMap<String, (usize, Provided)>,
    stacks: BTreeMap<String, Vec<(usize, Provided)>>,
    hidden: BTreeSet<String>,
}

fn resolve(packs: &[Pack]) -> io::Result<ResolvedPacks> {
    let sources: Vec<_> = packs
        .iter()
        .map(|pack| pack.source.open())
        .collect::<io::Result<_>>()?;
    // Final owner of every overridden file.
    let mut files: BTreeMap<String, (usize, Provided)> = BTreeMap::new();
    // Every pack's copy of the files vanilla reads as a resource stack.
    let mut stacks: BTreeMap<String, Vec<(usize, Provided)>> = BTreeMap::new();
    for (i, (p, source)) in packs.iter().zip(&sources).enumerate() {
        // A pack's filter hides what the packs below it provide.
        files.retain(|name, _| !p.filter.iter().any(|f| f.blocks(name)));
        stacks.retain(|name, _| !p.filter.iter().any(|f| f.blocks(name)));
        for (name, provided) in pack_files(p, source)? {
            if merge_kind(&name).is_some() {
                stacks
                    .entry(name.clone())
                    .or_default()
                    .push((i, provided.clone()));
            }
            files.insert(name, (i, provided));
        }
    }
    // FallbackResourceManager: a file's `.mcmeta` counts only from the pack that
    // supplies the file or one above it; vanilla's own sits below every pack.
    let mut hidden: BTreeSet<String> = files
        .iter()
        .filter(|(name, _)| !name.ends_with(".mcmeta"))
        .filter_map(|(name, (owner, _))| {
            let meta = format!("{name}.mcmeta");
            match files.get(&meta) {
                Some((meta_owner, _)) if meta_owner >= owner => None,
                _ => Some(meta),
            }
        })
        .collect();
    // A companion from below the winning colour would describe another surface.
    // A map-only pack above it is intentional and remains valid.
    for (name, (owner, _)) in &files {
        let Some(stem) = name.strip_suffix(".png") else {
            continue;
        };
        if stem.ends_with("_n") || stem.ends_with("_s") {
            continue;
        }
        for suffix in ["_n.png", "_s.png"] {
            let map = format!("{stem}{suffix}");
            if files
                .get(&map)
                .is_none_or(|(map_owner, _)| map_owner < owner)
            {
                hidden.insert(map);
            }
        }
    }
    for name in &hidden {
        files.remove(name);
    }
    Ok(ResolvedPacks {
        files,
        stacks,
        hidden,
    })
}

/// Resolve a pack selection without creating an overlay directory. Files keep
/// their open source; only vanilla's resource-stack merges are held as bytes.
/// Diverges from upstream: jars and zip packs remain in place during loading.
pub fn resources(
    base: crate::resource_source::ResourceSource,
    packs: &[Pack],
) -> io::Result<crate::resource_source::ResourceSource> {
    use crate::resource_source::{Resource, ResourceSource};
    if packs.is_empty() {
        return Ok(base);
    }
    let ResolvedPacks {
        files,
        stacks,
        hidden,
    } = resolve(packs)?;
    let filters: Vec<_> = packs.iter().flat_map(|pack| &pack.filter).collect();
    let merged = merged_resources(&base, packs, &stacks);
    let mut index: BTreeMap<_, _> = base
        .list("")
        .into_iter()
        .filter(|name| {
            !hidden.contains(name)
                && !filters.iter().any(|filter| filter.blocks(name))
                && name != horizon_core::prepared_files::FILE_NAME
        })
        .map(|name| (name.clone(), Resource::file(base.clone(), name)))
        .collect();
    for (name, (_, (_, source, inner))) in files {
        index.insert(name, Resource::file(source, inner));
    }
    for (name, bytes) in merged {
        index.insert(name, Resource::bytes(bytes));
    }
    // The overlay remembers the layer it resolved over: what a file a pack
    // replaces shadows (the atlas falls back to it when a pack texture cannot
    // be decoded, #15523).
    Ok(ResourceSource::layered(index, base))
}

fn merged_resources(
    base: &crate::resource_source::ResourceSource,
    packs: &[Pack],
    stacks: &BTreeMap<String, Vec<(usize, Provided)>>,
) -> BTreeMap<String, Vec<u8>> {
    let filters: Vec<_> = packs.iter().flat_map(|pack| &pack.filter).collect();
    let mut merged = BTreeMap::new();
    for (name, stack) in stacks {
        let mut layers = Vec::new();
        if !filters.iter().any(|filter| filter.blocks(name)) {
            layers.extend(base.read(name).ok());
        }
        for (_, (_, source, inner)) in stack {
            layers.extend(source.read(inner).ok());
        }
        if let Some(kind) = merge_kind(name)
            && let Some(bytes) = merge(kind, &layers)
        {
            merged.insert(name.clone(), bytes);
        }
    }
    merged
}

/// Files vanilla loads from every pack in turn (`getResourceStack`) rather
/// than from the top pack only.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Merge {
    /// `ClientLanguage.appendFrom`: later packs override keys.
    Lang,
    /// `SoundManager.Preparations.handleRegistration`: sounds add up unless
    /// an event says `replace`.
    Sounds,
    /// `FontManager.loadResourceStack`: the top pack's providers come first.
    Font,
    /// `SpriteSourceList.load`: sources from the bottom pack up.
    Atlas,
    /// `BlockStateModelLoader.loadBlockStateDefinitionStack`: every pack's
    /// definition binds on its own, and a higher one replaces only the states
    /// it binds. Kept as its layers ([`BLOCKSTATE_LAYERS`]) for the catalog.
    BlockStates,
}
/// The key of a stacked blockstate file: its definitions, bottom first, each
/// as the pack wrote it.
pub const BLOCKSTATE_LAYERS: &str = "horizon:layers";
/// Whether the packs merger stacks this file's every layer (atlases, lang,
/// sounds, blockstates) instead of letting the topmost one win.
pub fn merge_stacks(name: &str) -> bool {
    merge_kind(name).is_some()
}

fn merge_kind(name: &str) -> Option<Merge> {
    let (_, rest) = name.strip_prefix("assets/")?.split_once('/')?;
    if rest == "sounds.json" {
        return Some(Merge::Sounds);
    }
    let dir = rest.strip_suffix(".json")?.split_once('/')?.0;
    match dir {
        "lang" => Some(Merge::Lang),
        "font" => Some(Merge::Font),
        "atlases" => Some(Merge::Atlas),
        "blockstates" => Some(Merge::BlockStates),
        _ => None,
    }
}

/// One file with the effect of loading `layers` (bottom first) as vanilla
/// does. Layers that do not parse are skipped, as vanilla logs and skips them.
fn merge(kind: Merge, layers: &[Vec<u8>]) -> Option<Vec<u8>> {
    use serde_json::{Map, Value};
    let mut parsed: Vec<Map<String, Value>> = layers
        .iter()
        // SoundManager skips a sounds.json Gson rejects, whole.
        .filter(|b| kind != Merge::Sounds || crate::sound_events::is_valid_file(b))
        .filter_map(|b| {
            match serde_json::from_slice(b) {
                Ok(Value::Object(m)) => Some(m),
                // `Language.loadFromJson` reads with Gson's lenient reader, so
                // a lang file with `#` comments still counts; a file that
                // strict JSON accepts keeps its bytes as they are.
                _ if kind == Merge::Lang => {
                    let mut strings = std::collections::HashMap::new();
                    myriade_data::lang::read_lang_file(b, &mut strings);
                    Some(
                        strings
                            .into_iter()
                            .map(|(k, v)| (k, Value::String(v)))
                            .collect(),
                    )
                }
                _ => None,
            }
        })
        .collect();
    let list = |key: &str, layers: &mut dyn Iterator<Item = &Map<String, Value>>| {
        let items: Vec<Value> = layers
            .filter_map(|l| l.get(key).and_then(Value::as_array))
            .flatten()
            .cloned()
            .collect();
        Value::Object(Map::from_iter([(key.to_owned(), Value::Array(items))]))
    };
    let out = match kind {
        Merge::Lang => Value::Object(parsed.into_iter().flatten().collect()),
        Merge::Sounds => {
            let mut events = Map::new();
            for (event, def) in parsed.into_iter().flatten() {
                if !events.contains_key(&event)
                    || crate::sound_events::json_bool(&def["replace"], false)
                {
                    let mut fresh = Map::new();
                    if let Some(subtitle) = def.get("subtitle") {
                        fresh.insert("subtitle".into(), subtitle.clone());
                    }
                    fresh.insert("sounds".into(), Value::Array(Vec::new()));
                    events.insert(event.clone(), Value::Object(fresh));
                }
                if let (Some(Value::Array(sounds)), Some(Value::Array(more))) = (
                    events.get_mut(&event).and_then(|e| e.get_mut("sounds")),
                    def.get("sounds"),
                ) {
                    sounds.extend(more.iter().cloned());
                }
            }
            Value::Object(events)
        }
        // A pack's file with a provider vanilla refuses is dropped whole; the
        // other packs' files still load.
        Merge::Font => list(
            "providers",
            &mut parsed.iter().rev(),
        ),
        // SpriteSourceList: the bottom file whole, each layer's sources
        // appended to its own, from the bottom up. A layer whose sources
        // are not a list is skipped, as vanilla logs and skips the file.
        Merge::Atlas => {
            let mut layers = parsed
                .into_iter()
                .filter(|l| l.get("sources").is_none_or(Value::is_array));
            let mut doc = layers.next()?;
            let added = layers
                .filter_map(|l| l.get("sources").and_then(Value::as_array).cloned())
                .flatten()
                .collect();
            atlas_append(&mut doc, added)?;
            Value::Object(doc)
        }
        // One definition is the file as it is.
        Merge::BlockStates if parsed.len() < 2 => Value::Object(parsed.pop()?),
        Merge::BlockStates => Value::Object(Map::from_iter([(
            BLOCKSTATE_LAYERS.to_owned(),
            Value::Array(parsed.into_iter().map(Value::Object).collect()),
        )])),
    };
    serde_json::to_vec(&out).ok()
}

/// `SpriteSourceList.load` on one document: it keeps its own fields, and
/// `sources` gains `added` after its own entries. None when `sources` is
/// not a list.
fn atlas_append(
    doc: &mut serde_json::Map<String, serde_json::Value>,
    added: Vec<serde_json::Value>,
) -> Option<()> {
    let sources = doc
        .entry("sources".to_owned())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    sources.as_array_mut()?.extend(added);
    Some(())
}

/// stack of them loads: the base document whole with the added sources
/// after its own. `what` names the file in errors.
pub fn append_atlas_sources(
    base: Option<&str>,
    added: &[serde_json::Value],
    what: &str,
) -> Result<Vec<u8>, String> {
    let mut doc: serde_json::Map<String, serde_json::Value> = match base {
        Some(text) => serde_json::from_str(text).map_err(|e| format!("{what}: {e}"))?,
        None => serde_json::Map::new(),
    };
    atlas_append(&mut doc, added.to_vec()).ok_or_else(|| format!("{what}: no sources"))?;
    serde_json::to_vec(&doc).map_err(|e| format!("{what}: {e}"))
}
