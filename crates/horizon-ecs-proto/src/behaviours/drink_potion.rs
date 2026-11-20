//! `Witch.aiStep` drinking: now and then (a draw per condition, in the `Mob`'s order) it drinks water breathing, fire resistance, healing or
//! swiftness for `duration` ticks, slower by `speed_penalty` meanwhile, and the potion acts when the drink ends. `Drinking` is a standalone
//! component (`isDrinkingPotion`): `ranged_attack` does not throw while it is there. No goal: the module only ticks the `Drinker`s.
//! Not done: the potion in the hand, the sounds, the fire damage as the cause of the fire-resistance draw (only `Burning`).
use super::*;
use crate::cow::*;
use crate::effects::{self, Kind};
use crate::goals::*;
use crate::targeting;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Potion {
    WaterBreathing,
    FireResistance,
    Healing,
    Swiftness,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Drinker(Params);
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Drinking {
    pub potion: Potion,
    pub left: i32,
    pub saved_speed: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub duration: i32,
    pub speed_penalty: f64,
    pub effect_ticks: i32,
    pub heal: f32,
}
impl Params {
    pub const DEFAULT: Self = Self { duration: 32, speed_penalty: 0.25, effect_ticks: 3600, heal: 4.0 };
}

/// The drinker has a reason to roll (a target, a fire, a fluid, a wound): only these are visited by `tick`; the others sleep and
/// draw nothing. Laid by the hooks of `Target`, `Burning` and `InFluid`, by a damage and at the first arming; taken off by the
/// tick when no reason is left. (The `Mob` draws its four rolls every tick whatever the state: a roll whose condition is false
/// has no effect, so only the order of the draws on the creature's own stream differs, not what a player can see.)
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Thirsty;
/// `on_add` hook of the components that make a drinker thirsty.
pub fn thirst_hook(mut w: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let e = ctx.entity;
    if w.get::<Drinker>(e).is_some() && w.get::<Thirsty>(e).is_none() {
        w.commands().entity(e).insert(Thirsty);
    }
}
/// A reason to roll: the target (the swiftness), the fire, a fluid column (the water breathing), a wound.
fn has_reason(w: &World, e: Entity) -> bool {
    w.get::<targeting::Target>(e).is_some()
        || w.get::<Burning>(e).is_some()
        || w.get::<InFluid>(e).is_some()
        || w.get::<SpeciesId>(e).zip(w.get::<Health>(e)).is_some_and(|(s, h)| h.0 < w.resource::<SpeciesTable>().0[s.0 as usize].max_health)
}
fn arm(w: &mut World, e: Entity, _: Inst) {
    if has_reason(w, e) {
        w.entity_mut(e).insert(Thirsty);
    }
}
fn damaged(w: &mut World, e: Entity, _: Inst, _: &Damaged) {
    w.entity_mut(e).insert(Thirsty);
}

pub static MODULE: Module = Module { name: "drink_potion", flags: 0, equip, arm, damaged, can_use: never, tick: Some(tick), requires: &[Cap::Body, Cap::Goals, Cap::Targets], ..Module::DEFAULT };
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Drinker(*params::<Params>(p)));
}
/// How many drinkers `tick` visited (the tests of the rest cost read it; before `Thirsty` every drinker was visited every tick).
pub static VISITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn tick(w: &mut World) {
    let es: Vec<(Entity, Params)> = w.query_filtered::<(Entity, &Drinker), With<Thirsty>>().iter(w).map(|(e, d)| (e, d.0)).collect();
    VISITS.fetch_add(es.len() as u64, std::sync::atomic::Ordering::Relaxed);
    for (e, p) in es {
        if w.get::<Health>(e).is_none_or(|h| h.0 <= 0.0) {
            continue;
        }
        match w.get::<Drinking>(e).copied() {
            Some(mut d) => {
                d.left -= 1;
                if d.left >= 0 {
                    w.entity_mut(e).insert(d);
                } else {
                    finish(w, e, d, p);
                }
            }
            None => {
                if !has_reason(w, e) {
                    w.entity_mut(e).remove::<Thirsty>();
                    continue;
                }
                if let Some(potion) = choose(w, e) {
                    let Some(mut b) = w.get_mut::<Body>(e) else { continue };
                    let saved_speed = b.speed;
                    b.speed = (saved_speed - p.speed_penalty).max(0.0);
                    w.entity_mut(e).insert(Drinking { potion, left: p.duration, saved_speed });
                }
            }
        }
    }
}
fn finish(w: &mut World, e: Entity, d: Drinking, p: Params) {
    match d.potion {
        Potion::WaterBreathing => {
            effects::add(w, e, Kind::WaterBreathing, 0, p.effect_ticks);
        }
        Potion::FireResistance => {
            effects::add(w, e, Kind::FireResistance, 0, p.effect_ticks);
        }
        Potion::Swiftness => {
            effects::add(w, e, Kind::Speed, 0, p.effect_ticks);
        }
        Potion::Healing => {
            let max = w.get::<SpeciesId>(e).map_or(f32::MAX, |s| w.resource::<SpeciesTable>().0[s.0 as usize].max_health);
            if let Some(mut h) = w.get_mut::<Health>(e) {
                h.0 = (h.0 + p.heal).min(max);
            }
        }
    }
    if let Some(mut b) = w.get_mut::<Body>(e) {
        b.speed = d.saved_speed;
    }
    w.entity_mut(e).remove::<Drinking>();
}
/// The draws of `Witch.aiStep`, in its order, each condition after its own draw (so a failed draw spends no later one).
fn choose(w: &mut World, e: Entity) -> Option<Potion> {
    let mut roll = |w: &mut World, p: f64| w.get_mut::<Rng>(e).is_some_and(|mut r| r.next_f64() < p);
    let in_water = w.get::<FluidState>(e).is_some_and(|f| f.eye_in_water);
    if roll(w, 0.15) && in_water && !effects::has(w, e, Kind::WaterBreathing) {
        return Some(Potion::WaterBreathing);
    }
    if roll(w, 0.15) && w.get::<Burning>(e).is_some() && !effects::has(w, e, Kind::FireResistance) {
        return Some(Potion::FireResistance);
    }
    let hurt = w.get::<SpeciesId>(e).zip(w.get::<Health>(e)).is_some_and(|(s, h)| h.0 < w.resource::<SpeciesTable>().0[s.0 as usize].max_health);
    if roll(w, 0.05) && hurt {
        return Some(Potion::Healing);
    }
    if roll(w, 0.5) && !effects::has(w, e, Kind::Speed) && far_target(w, e) {
        return Some(Potion::Swiftness);
    }
    None
}
/// `target_far` of the `Mob` (`ai.rs:2334`): a target farther than 11 blocks.
fn far_target(w: &World, e: Entity) -> bool {
    let (Some(t), Some(p)) = (w.get::<targeting::Target>(e), w.get::<Pos>(e)) else { return false };
    targeting::info(w, t.who).is_some_and(|i| {
        let (dx, dy, dz) = (i.pos.x - p.0.x, i.pos.y - p.0.y, i.pos.z - p.0.z);
        dx * dx + dy * dy + dz * dz > 121.0
    })
}
