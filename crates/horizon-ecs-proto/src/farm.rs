//! Farm-animal behaviours beyond the common goals, as data and events (suivi/ecs-ecarts.md): the sheep's grazing
//! (`EatBlockGoal`) and wool, and the hen's egg and wings. A species takes a behaviour by having its rule in
//! the `rules` of its configuration (`rules.rs`); nothing here knows a species by name.
use crate::cow::*;
use crate::rules::Rule;
pub use crate::behaviours::eat_block::{Graze, Grazing};
use bevy_ecs::prelude::*;

/// `Chicken.eggTime`: `base + nextInt(range)` ticks, counted only while an adult.
#[derive(Clone, Copy, Debug)]
pub struct EggDef {
    pub base: i32,
    pub range: i32,
}
/// The rule `lay_egg` (params `EggDef`): a hen's egg timer, a rule of its tick in `Mob` (`Chicken.aiStep`), not a goal.
pub static LAY_EGG: Rule = Rule { name: "lay_egg", equip: equip_egg, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
/// The rule `wool` (no params): the sheep's wool, sheared until the animal eats again.
pub static WOOL: Rule = Rule { name: "wool", equip: |w, e, _| {
    w.entity_mut(e).insert(Wool::default());
}, attrs: crate::rules::no_attrs, interact: |w, e, _, i| i.item == crate::goals::shears() && shear(w, e) };
/// The parameters of `wing_flap`: the hen's wings and her glide (a fall in the air has its downward speed multiplied by
/// `fall_factor` each tick) and whether `causeFallDamage` answers no.
#[derive(Clone, Copy, Debug)]
pub struct WingParams {
    pub fall_factor: f64,
    pub fall_immune: bool,
}
pub static WING_FLAP: Rule = Rule { name: "wing_flap", equip: equip_wings, attrs: wing_attrs, interact: crate::rules::no_interact };
fn wing_attrs(p: &'static (dyn std::any::Any + Sync)) -> crate::rules::Attrs {
    let p = crate::rules::params::<WingParams>(p);
    crate::rules::Attrs { fall_factor: Some(p.fall_factor), immune: if p.fall_immune { IMMUNE_FALL } else { 0 } }
}
fn equip_wings(w: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Flap::default());
}
fn equip_egg(world: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    let d = *crate::rules::params::<EggDef>(p);
    let left = {
        let mut r = world.get_mut::<Rng>(e).unwrap();
        r.next_int(d.range) + d.base
    };
    world.entity_mut(e).insert(Egg { def: d, left: left as u64, at: u64::MAX });
    // The timer counts only for an adult: a newborn's is armed when it grows (`Grown`).
    if world.get::<GrowAt>(e).is_none() {
        arm_egg(world, e);
    }
}
/// The babies that grew up this tick (written by `due`).
#[derive(Resource, Default)]
pub struct Grown(pub Vec<Entity>);
/// The wool: `sheared` until the animal eats again, and the colour (data, set at spawn).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Wool {
    pub sheared: bool,
    pub color: u8,
}
/// The hen's egg: `left` ticks of an adult's life until the next one; `at` is the tick it is due (MAX: not counting).
#[derive(Component, Clone, Copy, Debug)]
pub struct Egg {
    pub def: EggDef,
    pub left: u64,
    pub at: u64,
}
/// The eggs laid (animal, tick): the egg item itself and the sound are out of scope (no item entities here).
#[derive(Resource, Default)]
pub struct LaidEggs(pub Vec<(Entity, u64)>);

fn arm_egg(w: &mut World, e: Entity) {
    let now = crate::cow::now_of(w, e);
    let Some(mut egg) = w.get_mut::<Egg>(e) else { return };
    egg.at = now + egg.left;
    let at = egg.at;
    crate::cow::arm_at(w, e, at, Due::Egg);
}
/// A deadline of the egg timer: lay (an adult), start again, or wait for the growth (a baby does not count).
pub fn lay_deadline(w: &mut World, e: Entity) {
    let now = crate::cow::now_of(w, e);
    let Some(egg) = w.get::<Egg>(e).copied() else { return };
    if egg.at == u64::MAX || now < egg.at {
        // A deadline of the wheel's reach, not the egg's.
        if egg.at != u64::MAX && now < egg.at {
            let at = egg.at;
            crate::cow::arm_at(w, e, at, Due::Egg);
        }
        return;
    }
    if w.get::<GrowAt>(e).is_some() {
        w.get_mut::<Egg>(e).unwrap().at = u64::MAX;
        return;
    }
    let tick = w.resource::<Clock>().0;
    w.resource_mut::<LaidEggs>().0.push((e, tick));
    let left = {
        let d = egg.def;
        let mut r = w.get_mut::<Rng>(e).unwrap();
        r.next_int(d.range) + d.base
    };
    w.get_mut::<Egg>(e).unwrap().left = left as u64;
    arm_egg(w, e);
}


/// The babies that grew up start counting their egg.
pub fn on_grown(w: &mut World) {
    for e in std::mem::take(&mut w.resource_mut::<Grown>().0) {
        if w.get::<Egg>(e).is_some_and(|g| g.at == u64::MAX) {
            arm_egg(w, e);
        }
    }
}
/// The wings: `flapSpeed` rises in the air and falls on the ground, `flapping` restarts at 1 in the air and decays, and
/// `flap` turns by it (`Chicken.aiStep`); render-only state. Kept while the physics of the hen is active; when it goes
/// to rest, what the decay of the wings would still add is added at once (`flapping` is geometric: the sum is exact).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Flap {
    pub flap: f32,
    pub speed: f32,
    pub flapping: f32,
    /// The wings have been settled (not stepped since the last rest).
    pub settled: bool,
}
fn flap_one(f: &mut Flap, on_ground: bool) {
    f.speed = (f.speed + if on_ground { -1.0 } else { 4.0 } * 0.3).clamp(0.0, 1.0);
    if !on_ground && f.flapping < 1.0 {
        f.flapping = 1.0;
    }
    f.flapping *= 0.9;
    f.flap += f.flapping * 2.0;
    f.settled = false;
}
/// The hen's wings, one step, before the physics of the tick (`Chicken.aiStep` runs before `travel`).
pub fn flap_system(
    mut q: Query<(&Contact, &mut Flap), With<Moving>>,
    mut removed: RemovedComponents<Moving>,
    mut rest: Query<(&Contact, &mut Flap), Without<Moving>>,
) {
    for (c, mut f) in &mut q {
        flap_one(&mut f, c.on_ground);
    }
    // A hen whose physics went inactive: the rest of its wings' decay at once.
    for e in removed.read() {
        if let Ok((c, mut f)) = rest.get_mut(e)
            && c.on_ground
            && !f.settled
        {
            f.speed = 0.0;
            f.flap += f.flapping * 0.9 * 2.0 / (1.0 - 0.9);
            f.flapping = 0.0;
            f.settled = true;
        }
    }
}
/// Registers the systems of this file.
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(flap_system.after(crate::cow::move_control).before(crate::cow::travel));
}
/// Shearing (`Sheep.shear`): an adult with wool loses it; the drops and the sound are out of scope. Returns whether it
/// was shorn.
pub fn shear(world: &mut World, e: Entity) -> bool {
    if world.get::<crate::cow::GrowAt>(e).is_some() {
        return false;
    }
    match world.get_mut::<Wool>(e) {
        Some(mut w) if !w.sheared => {
            w.sheared = true;
            true
        }
        _ => false,
    }
}

/// Sets the ticks of an adult until the hen's next egg (a test imposes the `Mob`'s draw).
pub fn set_egg(w: &mut World, e: Entity, left: u64) {
    w.get_mut::<Egg>(e).unwrap().left = left;
    if w.get::<GrowAt>(e).is_none() {
        arm_egg(w, e);
    }
}
