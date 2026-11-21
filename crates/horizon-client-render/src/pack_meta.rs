//! Which folders and zips of a data-pack folder are packs: their links checked against the game's
//! `allowed_symlinks.txt`, and their `pack.mcmeta` read. Only what the data-pack discovery needs.
use std::{
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
pub(crate) fn parse_meta(bytes: &[u8]) -> Option<serde_json::Value> {
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
