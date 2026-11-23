//! The natural spawning of the dedicated server's level after N ticks, as the parity tool reads it: the population by category
//! (the level's own counts, `SpawnCounts`) and by species (the entities' `SpeciesId`).
use crate::game::Game;
use myriade_engine::Result;
use horizon_ecs_proto::{
    cow::{SpeciesId, species_table},
    spawning::{Category, SpawnCounts},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

impl Game<'_> {
    /// The population after `ticks` ticks (200 by default): by category, the caps of the level applied, and by species.
    pub(super) fn op_natural_spawn(&mut self, op: &Value) -> Result<Value> {
        let ticks = op.get("ticks").and_then(Value::as_u64).unwrap_or(200);
        for _ in 0..ticks {
            self.tick_ecs()?;
        }
        let Some(w) = self.ecs.world_mut() else {
            return Ok(json!({"ticks": ticks, "categories": {}, "species": {}}));
        };
        let counts = w.resource::<SpawnCounts>().global;
        let table = species_table();
        let mut species: BTreeMap<String, u32> = BTreeMap::new();
        let mut query = w.query::<&SpeciesId>();
        for id in query.iter(&*w) {
            *species.entry(table.0[id.0 as usize].name.to_string()).or_default() += 1;
        }
        let categories: BTreeMap<&str, i32> = Category::SPAWNING.into_iter().map(|c| (c.name(), counts[c as usize])).collect();
        Ok(json!({"ticks": ticks, "categories": categories, "species": species}))
    }
}
