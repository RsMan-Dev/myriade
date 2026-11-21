//! Shearing (`Shearable.shear`), one rule for every species that has it. The species' `ShearDef` says what its "ready" state is, what the
//! shears change, which loot table falls, the sound and the height the stacks leave from; nothing here names a species.
//! - `SHEAR` (params `ShearDef`): equips the species' state and `Shearing`, and answers a player's shears.
//! - `shear_entity`: the shearing itself (state, loot, sound), also the dispenser's and a mod's way in (`player: None`: no one's tool).
//! The rule adds to the player's side what the host owns: the tool's wear and the `minecraft:shear` game event by `HostEvent`.
use crate::cow::{with_level_random, Body, GrowAt, Pos};
use crate::goals::Events;
use crate::item::spawn_at_location;
use crate::loot::roll_table;
use crate::rules::{params, Rule};
use bevy_ecs::prelude::*;
use horizon_core::random::{LegacyRandom, RandomSource};

pub const SHEAR_EVENT: &str = "minecraft:shear";

/// What a species does when sheared.
#[derive(Clone, Copy)]
pub struct ShearDef {
    /// Poses the species' state when the creature is created (the wool, the pumpkin, the markers).
    pub equip: fn(&mut World, Entity),
    /// `readyForShearing`: the shears take now.
    pub ready: fn(&World, Entity) -> bool,
    /// The state the shears change, then the effects only the species has. False: nothing happened (a mod refused the conversion).
    pub apply: fn(&mut World, Entity) -> bool,
    /// The loot table that falls, by the state before the change; None: nothing is rolled.
    pub table: fn(&World, Entity) -> Option<String>,
    pub sound: &'static str,
    /// Height above the feet the stacks leave from; None: the creature's height.
    pub height: Option<f64>,
    /// A particle burst at half the body's height once the change is done (the mooshroom's explosion); like the loot it waits for a
    /// conversion a mod may refuse.
    pub puff: Option<&'static str>,
}

/// The burst and loot of a shearing whose conversion the host was asked about (`conversion::resolve` runs them, or drops them on a refusal).
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct PendingShear {
    puff: Option<(&'static str, [f64; 3])>,
    table: Option<(String, [f64; 3])>,
}
impl PendingShear {
    pub fn finish(self, w: &mut World) {
        finish(w, self.puff, self.table);
    }
}

fn finish(w: &mut World, puff: Option<(&'static str, [f64; 3])>, table: Option<(String, [f64; 3])>) {
    if let Some((name, at)) = puff {
        w.resource_mut::<Events>().host.particles(name, at, [0.0; 3], 0.0, 1);
    }
    if let Some((table, at)) = table {
        drop_table(w, &table, at);
    }
}

/// The species' definition, kept on the creature for the dispenser and the mods, which reach it without the rule's params.
#[derive(Component, Clone, Copy)]
#[component(storage = "SparseSet")]
pub struct Shearing(pub &'static ShearDef);

pub static SHEAR: Rule = Rule {
    name: "shear",
    equip: |w, e, p| {
        let def: &'static ShearDef = params::<ShearDef>(p);
        (def.equip)(w, e);
        w.entity_mut(e).insert(Shearing(def));
    },
    attrs: crate::rules::no_attrs,
    interact,
};

fn interact(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync), i: &crate::behaviours::Interaction) -> bool {
    let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    if i.item != crate::goals::shears() || !shear_entity(w, e, Some(i.player)) {
        return false;
    }
    let host = &mut w.resource_mut::<Events>().host;
    host.held_worn(i.player, i.item, 1);
    host.player_game_event(i.player, SHEAR_EVENT, at);
    true
}

/// `Shearable.shear` on `e`: false when it is not ready. As in the jar the sound comes first, then the species' change; the burst and the
/// loot follow only when the change happened (`finalizeConversion`), or, when a mod is asked about a conversion, once it allows it. The
/// tool's wear and the player's game event belong to the caller (the rule), the dispenser has its own.
pub fn shear_entity(w: &mut World, e: Entity, player: Option<u32>) -> bool {
    let Some(def) = w.get::<Shearing>(e).map(|s| s.0) else { return false };
    if !(def.ready)(w, e) {
        return false;
    }
    let table = (def.table)(w, e);
    // Read before `apply`: a conversion replaces the entity.
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let body = w.get::<Body>(e).map_or(1.0, |b| f64::from(b.height));
    let height = def.height.unwrap_or(body);
    let place = [at.x, at.y, at.z];
    let events = &mut w.resource_mut::<Events>().host;
    events.sound(def.sound, place, 1.0, 1.0);
    if player.is_none() {
        events.game_event(SHEAR_EVENT, place);
    }
    if !(def.apply)(w, e) {
        return true;
    }
    let puff = def.puff.map(|n| (n, [at.x, at.y + body * 0.5, at.z]));
    let table = table.map(|t| (t, [at.x, at.y + height, at.z]));
    if w.get::<crate::conversion::ConvertAsked>(e).is_some() {
        w.entity_mut(e).insert(PendingShear { puff, table });
    } else {
        finish(w, puff, table);
    }
    true
}

/// Rolls `table` and throws each stack from `at` (`Entity.spawnAtLocation`).
pub fn drop_table(w: &mut World, table: &str, at: [f64; 3]) {
    let seed = with_level_random(w, |r| r.next_long());
    let stacks = roll_table(w, table, seed);
    drop_stacks(w, stacks, at);
}

/// Throws each stack from `at` as `Entity.spawnAtLocation` does.
pub fn drop_stacks(w: &mut World, stacks: Vec<horizon_client_lib::items::ItemStack>, at: [f64; 3]) {
    let throw = with_level_random(w, |r| r.next_long());
    let mut random = LegacyRandom::new(throw);
    for stack in stacks {
        spawn_at_location(w, stack, horizon_core::geometry::Vec3::new(at[0], at[1], at[2]), &mut random);
    }
}

/// An adult: the base of most `ready` tests.
pub fn adult(w: &World, e: Entity) -> bool {
    w.get::<GrowAt>(e).is_none()
}
