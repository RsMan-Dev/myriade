//! Tag-file merging for the worldgen overrides.
use super::*;

/// Evaluate a tag layer before its values or replace flag participate in a merge.
pub(super) fn tag_conditions_hold(
    doc: &J,
    conditions: &horizon_world::conditions::LoadConditions,
    path: &str,
) -> Result<bool> {
    if doc.get("neoforge:conditions").is_none() {
        return Ok(true);
    }
    let allowed = conditions.allows_file(&serde_json::to_value(doc)?, path);
    conditions.check_errors()?;
    Ok(allowed)
}

/// A tag file's `key` list (`values`, `remove`); empty where it has none.
pub(super) fn tag_list<'a>(tag: &'a J, key: &str) -> &'a [J] {
    match tag.get(key) {
        Some(J::Arr(list)) => list,
        _ => &[],
    }
}

/// A tag file's accepted values with `more` appended (no repeats), and
/// `remove` after its own `remove` entries (NeoForge's tag `remove` list).
/// The mods' layers of one tag, in load order: each layer's `values` and,
/// after them, its `remove` entries (`TagLoader`: a file adds, then removes).
pub(super) struct TagLayers<'a> {
    pub(super) values: &'a [J],
    pub(super) remove: &'a [J],
}

/// `vanilla`'s tag file over what the mods made of it: the mods' layers join
/// its `values` as ordered entries, a removal carried as `{"id": ..., "remove":
/// true}` (`TagLoader.EntryWithSource.remove`), so the folder walks adds and
/// removals in the mods' load order instead of all adds before all removals.
pub(super) fn merge_tag(
    text: Option<&str>,
    layers: &[TagLayers<'_>],
    what: &str,
    _conditions: &horizon_world::conditions::LoadConditions,
) -> Result<String> {
    let mut doc = match text {
        Some(t) => serde_json::from_str(t).map_err(|e| format!("{what}: {e}"))?,
        None => J::Obj(vec![("values".into(), J::Arr(Vec::new()))]),
    };
    // Tags do not evaluate load conditions.
    doc.remove("neoforge:conditions");
    // The vanilla file's own `remove` entries apply right after its values,
    // before the mods' layers: they join `values` as removal objects.
    let vanilla_remove = match doc.get("remove") {
        Some(J::Arr(remove)) => Some(remove.clone()),
        _ => None,
    };
    doc.remove("remove");
    let Some(J::Arr(values)) = doc.get_mut("values") else {
        return Err(format!("{what}: no values").into());
    };
    if let Some(remove) = vanilla_remove {
        for r in &remove {
            let removal = match r {
                J::Str(id) => J::Obj(vec![
                    ("id".into(), J::Str(id.clone())),
                    ("remove".into(), J::Bool(true)),
                ]),
                other => {
                    let mut entry = other.clone();
                    entry.set("remove", J::Bool(true));
                    entry
                }
            };
            values.push(removal);
        }
    }
    for layer in layers {
        for v in layer.values {
            values.push(v.clone());
        }
        for r in layer.remove {
            let removal = match r {
                J::Str(id) => J::Obj(vec![
                    ("id".into(), J::Str(id.clone())),
                    ("remove".into(), J::Bool(true)),
                ]),
                other => {
                    let mut entry = other.clone();
                    entry.set("remove", J::Bool(true));
                    entry
                }
            };
            values.push(removal);
        }
    }
    doc.remove("remove");
    Ok(serde_json::to_string_pretty(&doc)?)
}

/// What the mods do to one tag file, in load order (a data pack's
/// TagLoader): each adds its values, and one that replaces the tag first
/// drops vanilla's entries and the mods' before it.
#[derive(Default)]
pub(super) struct TagMerge {
    /// A mod replaced the tag: vanilla's entries are left out.
    pub(super) replaced: bool,
    /// The mods whose values are in `values`.
    pub(super) mods: Vec<String>,
    /// Each mod file's additions and, after them, its removals, in load
    /// order (`TagLoader`: one file adds its values, then its `remove`
    /// entries apply). A replace drops the layers below it.
    pub(super) layers: Vec<(Vec<J>, Vec<J>)>,
}

impl TagMerge {
    /// Adds one mod's tag file. A replace that drops entries (vanilla's in a
    /// `shared` tag, one not in the mod's namespaces, or other mods') warns.
    pub(super) fn add(
        &mut self,
        id: &str,
        file: &str,
        replace: bool,
        shared: bool,
        values: &[J],
    ) -> Option<String> {
        let mut warning = None;
        if replace {
            let mut dropped: Vec<String> = Vec::new();
            if shared && !self.replaced {
                dropped.push("vanilla's entries".into());
            }
            if !self.mods.is_empty() {
                dropped.push(format!("those of {}", self.mods.join(", ")));
            }
            if !dropped.is_empty() {
                warning = Some(format!(
                    "mod {id} replaces {file}: {} are left out, as a data pack's replace \
                     does; the mods after it in load order still add to it",
                    dropped.join(" and ")
                ));
            }
            self.replaced = true;
            self.mods.clear();
            self.layers.clear();
        }
        if !self.mods.iter().any(|m| m == id) {
            self.mods.push(id.to_owned());
        }
        self.layers.push((values.to_vec(), Vec::new()));
        warning
    }

    /// The layers' entries for [`merge_tag`].
    pub(super) fn layers(&self) -> Vec<TagLayers<'_>> {
        self.layers
            .iter()
            .map(|(values, remove)| TagLayers { values, remove })
            .collect()
    }
}

