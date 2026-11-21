//! An entity lets a loot table fall as its own gift (`Mob.dropFromGiftLootTable`): the stacks are thrown from its place as
//! `Entity.spawnAtLocation` throws them, then (only if something fell) the sound and the game event. A species names the table in
//! a `GiftDrop`; what starts the drop is a rule, a deadline of the wheel (`after`) or an event, never a scan:
//! - `PERIODIC_GIFT`: a timer of `base + nextInt(range)` ticks that drops and starts again (`Armadillo.customServerAiStep`).
//! - `GROW_GIFT`: a drop when a baby grows up (`Turtle.ageBoundaryReached`, called from `farm::on_grown`).
//! - `drop_gift` itself, for a module that has its own moment (the hen's egg, the panda's sneeze).
use crate::cow::{after, now_of, Pos, Rng};
use crate::goals::Events;
use crate::item::spawn_at_location;
use crate::loot::roll_table;
use crate::rules::Rule;
use bevy_ecs::prelude::*;
use horizon_core::random::LegacyRandom;

/// The sound of a gift: `volume` and a pitch of `(r1 - r2) * 0.2 + 1` from the entity's own draws.
#[derive(Clone, Copy, Debug)]
pub struct GiftSound {
    pub name: &'static str,
    pub volume: f32,
}
/// What falls and what goes with it. `needs_mob_drops`: nothing falls while the `mob_drops` rule is off (`shouldDropLoot`).
#[derive(Clone, Copy, Debug)]
pub struct GiftDrop {
    pub table: &'static str,
    pub sound: Option<GiftSound>,
    pub game_event: Option<&'static str>,
    pub needs_mob_drops: bool,
}

/// Rolls `g.table` for `e` and throws the stacks from its feet; true when anything fell.
pub fn drop_gift(w: &mut World, e: Entity, g: &GiftDrop) -> bool {
    if g.needs_mob_drops && !crate::blocks::world_of(w).mob_drops {
        return false;
    }
    let Some(at) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let Some((seed, throw, a, b)) = w.get_mut::<Rng>(e).map(|mut r| {
        let seed = i64::from(r.next_u32()) << 32 | i64::from(r.next_u32());
        (seed, i64::from(r.next_u32()), r.next_f64() as f32, r.next_f64() as f32)
    }) else {
        return false;
    };
    let stacks = roll_table(w, g.table, seed);
    if stacks.is_empty() {
        return false;
    }
    let mut random = LegacyRandom::new(throw);
    for stack in stacks {
        spawn_at_location(w, stack, at, &mut random);
    }
    let place = [at.x, at.y, at.z];
    let mut events = w.resource_mut::<Events>();
    if let Some(s) = g.sound {
        events.host.sound(s.name, place, s.volume, (a - b) * 0.2 + 1.0);
    }
    if let Some(name) = g.game_event {
        events.host.game_event_by(e, name, place, false, false, false);
    }
    true
}

/// The parameters of `PERIODIC_GIFT`: the wait is `base + nextInt(range)` ticks of the entity's own time.
#[derive(Clone, Copy, Debug)]
pub struct Periodic {
    pub base: i32,
    pub range: i32,
    pub gift: &'static GiftDrop,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct PeriodicGift(pub Periodic);

/// The rule `periodic_gift` (params `Periodic`): the first wait is drawn and armed at creation, each deadline drops and arms the next.
pub static PERIODIC_GIFT: Rule = Rule { name: "periodic_gift", equip: equip_periodic, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn wait(w: &mut World, e: Entity, p: &Periodic) -> i32 {
    w.get_mut::<Rng>(e).map_or(p.base, |mut r| r.next_int(p.range) + p.base)
}
fn equip_periodic(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    let p = *crate::rules::params::<Periodic>(p);
    w.entity_mut(e).insert(PeriodicGift(p));
    let t = wait(w, e, &p);
    arm_gift(w, e, t);
}
fn periodic_due(w: &mut World, e: Entity) {
    let Some(PeriodicGift(p)) = w.get::<PeriodicGift>(e).copied() else { return };
    // A deadline left behind by a reloaded wait (`rearm_gift`) is not due.
    if w.get::<GiftDue>(e).is_some_and(|d| d.0 > now_of(w, e)) {
        return;
    }
    drop_gift(w, e, p.gift);
    let t = wait(w, e, &p);
    arm_gift(w, e, t);
}
/// The tick the periodic gift is due (`Armadillo.scuteTime`, saved as the ticks left under `scute_time`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct GiftDue(pub u64);
fn arm_gift(w: &mut World, e: Entity, ticks: i32) {
    let at = now_of(w, e) + u64::try_from(ticks).unwrap_or(0);
    w.entity_mut(e).insert(GiftDue(at));
    after(w, e, ticks, periodic_due);
}
/// A saved wait (`scute_time`) replaces the drawn one.
pub fn rearm_gift(w: &mut World, e: Entity, ticks: i32) {
    arm_gift(w, e, ticks);
}

#[derive(Component, Clone, Copy, Debug)]
pub struct GrowGift(pub &'static GiftDrop);
/// The rule `grow_gift` (params `GiftDrop`): the gift falls when the baby grows up.
pub static GROW_GIFT: Rule = Rule { name: "grow_gift", equip: equip_grow, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn equip_grow(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(GrowGift(crate::rules::params::<GiftDrop>(p)));
}
/// A baby that grew up (called by `farm::on_grown`).
pub fn on_grown(w: &mut World, e: Entity) {
    if let Some(GrowGift(g)) = w.get::<GrowGift>(e).copied() {
        drop_gift(w, e, g);
    }
}

/// `BehaviorUtils.throwItem`: the stack leaves `e` at its eye minus 0.3, at 0.3 blocks per tick toward `target` (taken from its feet),
/// with the pickup delay of a fresh item (10 ticks).
pub fn throw_toward(w: &mut World, e: Entity, stack: horizon_client_lib::items::ItemStack, target: horizon_core::geometry::Vec3) {
    let Some(me) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let eye = w.get::<crate::goals::Look>(e).map_or(1.62, |l| l.eye);
    let from = horizon_core::geometry::Vec3::new(me.x, me.y + eye - 0.3, me.z);
    let v = crate::rules::social_gift_hero::throw_velocity(me, target);
    crate::item::drop_item(w, stack, from, v, 10);
}
/// `PiglinAi.throwItems`: the stacks go toward one block above the nearest player it can target within `sight` blocks, else toward a
/// land spot `h` / `v` blocks about (`LandRandomPos.getPos`), else its own place; one swing of the off hand when there is anything.
pub fn throw_toward_player_or_nearby(w: &mut World, e: Entity, stacks: Vec<horizon_client_lib::items::ItemStack>, sight: f64, h: i32, v: i32) {
    use horizon_core::geometry::Vec3;
    if stacks.is_empty() {
        return;
    }
    let Some(me) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let area = crate::area::reach_box(w, e, sight);
    let nearest = crate::area::players_in(w, area)
        .into_iter()
        .filter(|&id| crate::targeting::can_target(w, e, crate::targeting::Who::Player(id)))
        .filter_map(|id| {
            let p = crate::targeting::info(w, crate::targeting::Who::Player(id))?.pos;
            Some((p, (p.x - me.x).powi(2) + (p.y - me.y).powi(2) + (p.z - me.z).powi(2)))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(p, _)| p);
    let target = nearest.or_else(|| crate::spots::pick_for(w, e, crate::spots::Pick::Land { h, v }).map(|a| Vec3::new(a[0], a[1], a[2]))).unwrap_or(me);
    w.resource_mut::<Events>().host.swing(e, true);
    for stack in stacks {
        throw_toward(w, e, stack, Vec3::new(target.x, target.y + 1.0, target.z));
    }
}
