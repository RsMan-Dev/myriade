//! The checks a start makes on the prepared game files: missing or damaged.

/// Every launch reads the prepared game files (`horizon-prepare`'s output, whose
/// manifest is `horizon-resources.json`): without them, say where they were
/// looked for and how to get them, before a window or a loader starts. `get` is
/// the page a hook sends the player to (`horizon_hooks::engine::get_the_game`).
pub(in crate::game) fn missing_resources(
    root: &std::path::Path,
    get: Option<horizon_hooks::GetTheGame>,
) -> Option<String> {
    let shown = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
    if !root.join("horizon-resources.json").is_file() {
        let launcher = get.map_or(String::new(), |get| {
            format!(
                "Horizon is started from its launcher: download it at {} (Download). It needs \
                 your own Minecraft: Java Edition; the launcher signs you in and prepares the \
                 game files.\nDevelopers: ",
                get.address
            )
        });
        return Some(format!(
            "Horizon's prepared game files are missing: no horizon-resources.json in {}\n\
             {launcher}horizon-prepare --prepare, or pass --resources <folder>",
            shown.display(),
        ));
    }
    damaged_prepared(root).map(|problem| {
        format!(
            "Horizon's prepared game files are damaged: {problem} in {}\n\
             Prepare them again with the launcher, or: horizon-prepare --prepare",
            shown.display()
        )
    })
}

/// Which top-level prepared file a killed preparation or an interrupted copy
/// left unusable. The loader parses these before it can start; naming one
/// beats serde's bare "expected value at line 1 column 1".
pub(in crate::game) fn damaged_prepared(root: &std::path::Path) -> Option<String> {
    let manifest = root.join("horizon-resources.json");
    let bytes = std::fs::read(&manifest).ok()?;
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Some("horizon-resources.json is empty".into());
    }
    if serde_json::from_slice::<serde_json::Value>(&bytes).is_err() {
        return Some("horizon-resources.json is not valid JSON".into());
    }
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.extension() != Some(std::ffi::OsStr::new("json")) {
            continue;
        }
        if std::fs::metadata(&path).is_ok_and(|meta| meta.len() == 0) {
            return Some(format!("{} is empty", entry.file_name().to_string_lossy()));
        }
    }
    None
}
