//! The trial spawner's configs as the host reads and writes them: the codec of `TrialSpawnerConfig` (`decode`, `encode`), its targets,
//! the registry of the data pack's configs, and the spawn egg that replaces both configs. The data is `myriade_data::trial_config`.
use crate::mobs::spawner::{self, SpawnDataExt};
use crate::mobs::trial_spawner::identifier;
use myriade_data::trial_config::{SpawnData, TrialConfig};
use horizon_core::nbt::Tag;
use std::{collections::HashMap, path::Path, sync::OnceLock};

/// `TrialSpawnerConfig`'s codec and targets, over the data in `myriade_data::trial_config`.
pub trait TrialConfigExt: Sized {
    /// `TrialSpawnerConfig.DIRECT_CODEC`; None when the tag is not a
    /// compound. Read as `TagValueInput` does, keeping the partial result:
    /// a field that fails to decode takes its default, and the list entries
    /// that fail are dropped.
    fn decode(tag: &Tag) -> Option<Self>;
    /// `TrialSpawnerConfig.DIRECT_CODEC`'s encoding: the fields that differ
    /// from their defaults (`optionalFieldOf` with a default).
    fn encode(&self) -> Tag;
    /// `calculateTargetTotalMobs`.
    fn target_total_mobs(&self, additional_players: i32) -> i32;
    /// `calculateTargetSimultaneousMobs`.
    fn target_simultaneous_mobs(&self, additional_players: i32) -> i32;
}
impl TrialConfigExt for TrialConfig {
    fn decode(tag: &Tag) -> Option<Self> {
        let Tag::Compound(_) = tag else {
            return None;
        };
        let d = Self::default();
        let float = |k: &str, default: f32| {
            tag.get(k)
                .and_then(float)
                .filter(|v| *v >= 0.0)
                .unwrap_or(default)
        };
        let int = |k: &str, default: i32, lo: i32, hi: i32| {
            spawner::number(tag.get(k))
                .filter(|v| (i64::from(lo)..=i64::from(hi)).contains(v))
                .map_or(default, |v| v as i32)
        };
        let list = |k: &str| match tag.get(k) {
            Some(Tag::List(list)) => Some(list.iter()),
            _ => None,
        };
        let spawn_potentials = list("spawn_potentials").map_or_else(Vec::new, |l| {
            l.filter_map(|e| Some((SpawnData::decode(e.get("data")?)?, weight(e.get("weight"))?)))
                .collect()
        });
        let loot_tables_to_eject =
            list("loot_tables_to_eject").map_or(d.loot_tables_to_eject, |l| {
                l.filter_map(|e| Some((identifier(e.get("data"))?, weight(e.get("weight"))?)))
                    .collect()
            });
        Some(Self {
            spawn_range: int("spawn_range", d.spawn_range, 1, 128),
            total_mobs: float("total_mobs", d.total_mobs),
            simultaneous_mobs: float("simultaneous_mobs", d.simultaneous_mobs),
            total_mobs_added_per_player: float(
                "total_mobs_added_per_player",
                d.total_mobs_added_per_player,
            ),
            simultaneous_mobs_added_per_player: float(
                "simultaneous_mobs_added_per_player",
                d.simultaneous_mobs_added_per_player,
            ),
            ticks_between_spawn: int("ticks_between_spawn", d.ticks_between_spawn, 0, i32::MAX),
            spawn_potentials,
            loot_tables_to_eject,
            items_to_drop_when_ominous: identifier(tag.get("items_to_drop_when_ominous"))
                .unwrap_or(d.items_to_drop_when_ominous),
        })
    }
    fn encode(&self) -> Tag {
        let d = Self::default();
        let mut out = Vec::new();
        if self.spawn_range != d.spawn_range {
            out.push(("spawn_range", Tag::Int(self.spawn_range)));
        }
        for (key, value, default) in [
            ("total_mobs", self.total_mobs, d.total_mobs),
            (
                "simultaneous_mobs",
                self.simultaneous_mobs,
                d.simultaneous_mobs,
            ),
            (
                "total_mobs_added_per_player",
                self.total_mobs_added_per_player,
                d.total_mobs_added_per_player,
            ),
            (
                "simultaneous_mobs_added_per_player",
                self.simultaneous_mobs_added_per_player,
                d.simultaneous_mobs_added_per_player,
            ),
        ] {
            if value != default {
                out.push((key, Tag::Float(value.to_bits())));
            }
        }
        if self.ticks_between_spawn != d.ticks_between_spawn {
            out.push(("ticks_between_spawn", Tag::Int(self.ticks_between_spawn)));
        }
        let weighted = |data: Tag, weight: i32| {
            Tag::compound(vec![("data", data), ("weight", Tag::Int(weight))])
        };
        if !self.spawn_potentials.is_empty() {
            let list = self.spawn_potentials.iter();
            out.push((
                "spawn_potentials",
                Tag::List(list.map(|(data, w)| weighted(data.encode(), *w)).collect()),
            ));
        }
        if self.loot_tables_to_eject != d.loot_tables_to_eject {
            let list = self.loot_tables_to_eject.iter();
            out.push((
                "loot_tables_to_eject",
                Tag::List(
                    list.map(|(id, w)| weighted(Tag::String(id.as_str().into()), *w))
                        .collect(),
                ),
            ));
        }
        if self.items_to_drop_when_ominous != d.items_to_drop_when_ominous {
            out.push((
                "items_to_drop_when_ominous",
                Tag::String(self.items_to_drop_when_ominous.as_str().into()),
            ));
        }
        Tag::compound(out)
    }
    fn target_total_mobs(&self, additional_players: i32) -> i32 {
        (self.total_mobs + self.total_mobs_added_per_player * additional_players as f32).floor()
            as i32
    }
    fn target_simultaneous_mobs(&self, additional_players: i32) -> i32 {
        (self.simultaneous_mobs
            + self.simultaneous_mobs_added_per_player * additional_players as f32)
            .floor() as i32
    }
}

static REGISTRY: OnceLock<HashMap<String, TrialConfig>> = OnceLock::new();

/// `Registries.TRIAL_SPAWNER_CONFIG`: the configs under the resources'
/// `data/<namespace>/trial_spawner`, read once per process.
pub fn load_registry(root: &Path) {
    REGISTRY.get_or_init(|| {
        let mut out = HashMap::new();
        let Ok(namespaces) = std::fs::read_dir(root.join("data")) else {
            return out;
        };
        for ns in namespaces.flatten() {
            let base = ns.path().join("trial_spawner");
            let mut stack = vec![base.clone()];
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                        continue;
                    }
                    let (Ok(rel), Some(ns)) = (
                        path.with_extension("")
                            .strip_prefix(&base)
                            .map(Path::to_path_buf),
                        ns.file_name().to_str().map(str::to_owned),
                    ) else {
                        continue;
                    };
                    let config = std::fs::read(&path)
                        .ok()
                        .and_then(|b| serde_json::from_slice(&b).ok())
                        .and_then(|v| TrialConfig::decode(&crate::scoreboard::json_to_tag(&v)));
                    match config {
                        Some(c) => {
                            let id = rel.to_string_lossy().replace('\\', "/");
                            out.insert(format!("{ns}:{id}"), c);
                        }
                        None => log::warn!("Horizon: bad trial spawner config {}", path.display()),
                    }
                }
            }
        }
        out
    });
}
/// A config of the registry by id.
pub fn registry_config(id: &str) -> Option<TrialConfig> {
    REGISTRY.get()?.get(id).cloned()
}
/// The accepted, decoded trial-spawner registry identities, without cloning configurations.
pub fn registry_contains(id: &str) -> Option<bool> {
    Some(REGISTRY.get()?.contains_key(id))
}
/// The identities of the same immutable registry gameplay reads.
pub fn registry_ids() -> Option<Vec<&'static str>> {
    Some(REGISTRY.get()?.keys().map(String::as_str).collect())
}

fn float(t: &Tag) -> Option<f32> {
    Some(match t {
        Tag::Float(b) => f32::from_bits(*b),
        Tag::Double(b) => f64::from_bits(*b) as f32,
        t => spawner::number(Some(t))? as f32,
    })
}
/// `Weighted.CODEC`'s weight: a non-negative int.
fn weight(t: Option<&Tag>) -> Option<i32> {
    spawner::number(t)
        .filter(|w| (0..=i64::from(i32::MAX)).contains(w))
        .map(|w| w as i32)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::block_entity::{ConfigSlot, TrialSpawner};
    use crate::mobs::trial_spawner::TrialSpawnerExt;

    fn tag(snbt: &str) -> Tag {
        horizon_core::nbt::parse_snbt(snbt).expect("snbt").0
    }

    #[test]
    fn defaults_load_and_save_to_nothing() {
        let s = TrialSpawner::load_with(&tag("{}"), &|_| None);
        assert_eq!(s, TrialSpawner::default());
        assert!(s.save().is_empty());
    }

    #[test]
    fn a_spawn_egg_overrides_both_configs_inline() {
        let breeze = TrialConfig {
            total_mobs: 2.0,
            ..TrialConfig::default()
        };
        let registry = |_: &str| Some(breeze.clone());
        let mut s = TrialSpawner::load_with(
            &tag(
                r#"{normal_config:"minecraft:trial_chamber/breeze/normal",spawn_data:{entity:{}},total_mobs_spawned:3}"#,
            ),
            &registry,
        );
        s.override_entity("minecraft:zombie");
        assert_eq!(s.next_spawn_data, None);
        assert_eq!(s.total_mobs_spawned, 0);
        for slot in [&s.normal, &s.ominous] {
            assert_eq!(slot.config.spawn_potentials.len(), 1);
            assert_eq!(
                slot.config.spawn_potentials[0].0.id().as_deref(),
                Some("minecraft:zombie")
            );
            let saved = slot.saved.as_ref().expect("saved inline");
            assert_eq!(TrialConfig::decode(saved).as_ref(), Some(&slot.config));
        }
        assert_eq!(
            s.normal.saved.as_ref().and_then(|t| t.get("total_mobs")),
            Some(&Tag::Float(2.0f32.to_bits()))
        );
    }

    #[test]
    fn registry_configs_resolve_and_are_saved_by_id() {
        let breeze = TrialConfig {
            total_mobs: 2.0,
            ..TrialConfig::default()
        };
        let registry =
            |id: &str| (id == "minecraft:trial_chamber/breeze/normal").then(|| breeze.clone());
        let s = TrialSpawner::load_with(
            &tag(
                r#"{normal_config:"minecraft:trial_chamber/breeze/normal",required_player_range:10}"#,
            ),
            &registry,
        );
        assert_eq!(s.normal.config.total_mobs, 2.0);
        assert_eq!(s.required_player_range, 10);
        let saved: Vec<_> = s.save().into_iter().map(|(k, _)| k).collect();
        assert_eq!(saved, ["normal_config", "required_player_range"]);
    }

    #[test]
    fn an_invalid_field_takes_its_default_alone() {
        let s = TrialSpawner::load_with(
            &tag(
                r#"{normal_config:"minecraft:nope",required_player_range:200,target_cooldown_length:5}"#,
            ),
            &|_| None,
        );
        assert_eq!(s.normal, ConfigSlot::default());
        assert_eq!(s.required_player_range, 14);
        assert_eq!(s.target_cooldown_length, 5);
        let c = TrialConfig::decode(&tag(
            r#"{spawn_range:0,total_mobs:3.0f,spawn_potentials:[{data:{entity:{id:"minecraft:zombie"}},weight:-1},{data:{entity:{id:"minecraft:husk"}},weight:2}]}"#,
        ))
        .expect("config");
        assert_eq!(c.spawn_range, 4);
        assert_eq!(c.total_mobs, 3.0);
        assert_eq!(c.spawn_potentials.len(), 1);
    }

    #[test]
    fn targets_floor_the_per_player_additions() {
        let c = TrialConfig {
            simultaneous_mobs: 1.0,
            simultaneous_mobs_added_per_player: 0.5,
            ..TrialConfig::default()
        };
        assert_eq!(c.target_simultaneous_mobs(0), 1);
        assert_eq!(c.target_simultaneous_mobs(1), 1);
        assert_eq!(c.target_simultaneous_mobs(2), 2);
        assert_eq!(c.target_total_mobs(3), 12);
    }
}
