//! The spawn charges of the biomes (`MobSpawnSettings.MobSpawnCost`) and the potential they make (`PotentialCalculator`): a spawn must keep
//! the energy change of its place within its budget (`SpawnState.canSpawn`). The field is read from the bearers on demand, never per entity.
use crate::cow::{Pos, SpeciesId, SpeciesTable};
use crate::spawn_biome::{biome_at, zoom_of};
use crate::spawning::Spawner;
use bevy_ecs::{lifecycle::HookContext, prelude::*, world::DeferredWorld};
use myriade_engine_lib::biome::SpawnCost;
use myriade_engine_lib::mobs::MobWorld;

/// A mob that charges the potential of its place (`SpawnState.afterSpawn` adds it). Sparse: only the mobs born in a biome with a cost
/// for their species carry it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_add = bearer_changed, on_remove = bearer_changed)]
pub struct ChargeBearer {
    pub charge: f64,
}

/// The potential field: the charges of the bearers at their block. `version` counts the bearers that came or went; `built` is the
/// version the points were read at (`None`: never read).
#[derive(Resource, Default)]
pub struct Charges {
    points: Vec<([i32; 3], f64)>,
    version: u64,
    built: Option<u64>,
}

fn bearer_changed(mut w: DeferredWorld, _ctx: HookContext) {
    if let Some(mut c) = w.get_resource_mut::<Charges>() {
        c.version += 1;
    }
}

/// Reads the bearers' charges where they stand, if a bearer came or went since the last reading. Only the bearers are queried.
pub fn refresh(w: &mut World) {
    let c = w.resource::<Charges>();
    if c.built == Some(c.version) {
        return;
    }
    let version = c.version;
    let mut q = w.query::<(&ChargeBearer, &Pos)>();
    let points: Vec<_> = q.iter(w).filter(|(b, _)| b.charge != 0.0).map(|(b, p)| (block_of(&p.0), b.charge)).collect();
    let mut c = w.resource_mut::<Charges>();
    c.points = points;
    c.built = Some(version);
}

/// `createState` counts a creature that is not persistent (its category is not MISC) and adds its biome's charge with it: the caps count
/// it and its bearer is posed. Called where a creature is made from its NBT, and where `release` counts it again.
pub fn count(w: &mut World, e: Entity) {
    if w.get::<crate::despawn::Persistent>(e).is_some() {
        return;
    }
    crate::spawning::recount(w, e);
    let Some(id) = w.get::<SpeciesId>(e).map(|s| s.0) else { return };
    let Some(p) = w.get::<Pos>(e).map(|p| block_of(&p.0)) else { return };
    let name = w.resource::<SpeciesTable>().0[id as usize].name;
    // No level open (a restore made outside a frame): no biome to read, so no charge.
    let Some(mw) = w.get_resource::<crate::cow::Level>().map(|l| l.mw.0) else { return };
    if let Some(cost) = cost_at(w, mw, name, p) {
        w.entity_mut(e).insert(ChargeBearer { charge: cost.charge });
    }
}

fn block_of(p: &horizon_core::geometry::Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

/// The spawn cost of a species in the biome of a block (`MobSpawnSettings.getMobSpawnCost`), `None` where that biome has none.
pub fn cost_at(w: &World, mw: &MobWorld<'_>, name: &str, at: [i32; 3]) -> Option<SpawnCost> {
    let zoom = w.get_resource::<Spawner>().map_or_else(|| zoom_of(mw), |s| s.zoom);
    mw.biomes.get(biome_at(mw, zoom, at[0], at[1], at[2])).spawn_costs.get(name).copied()
}

impl Charges {
    /// The version of the bearers' set: a reading is current when it was made at this version.
    pub fn version(&self) -> u64 {
        self.version
    }
    /// `PotentialCalculator.getPotentialEnergyChange`: each point's charge over its distance to `pos` (infinite on the point itself), summed,
    /// times `charge`.
    fn energy_change(&self, pos: [i32; 3], charge: f64) -> f64 {
        if charge == 0.0 {
            return 0.0;
        }
        let sum: f64 = self
            .points
            .iter()
            .map(|(p, c)| {
                let d2 = f64::from(p[0] - pos[0]).powi(2) + f64::from(p[1] - pos[1]).powi(2) + f64::from(p[2] - pos[2]).powi(2);
                if d2 == 0.0 { f64::INFINITY } else { c / d2.sqrt() }
            })
            .sum();
        sum * charge
    }
    /// `SpawnState.canSpawn`: the energy change of a spawn at `pos` stays within its budget. Needs a current reading (`refresh`).
    pub fn can_spawn(&self, pos: [i32; 3], cost: SpawnCost) -> bool {
        self.energy_change(pos, cost.charge) <= cost.energy_budget
    }
    /// `SpawnState.afterSpawn`, when the birth since the reading at `before` is the only change: its charge joins the field, which stays
    /// current. Otherwise the next `refresh` reads the bearers again.
    pub fn after_birth(&mut self, pos: [i32; 3], charge: f64, before: u64) {
        if self.built == Some(before) && self.version == before + 1 {
            if charge != 0.0 {
                self.points.push((pos, charge));
            }
            self.built = Some(self.version);
        }
    }
}
