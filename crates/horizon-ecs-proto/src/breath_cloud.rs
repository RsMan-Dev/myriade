//! The area cloud as a sleeping entity (`AreaEffectCloud`, Horizon `dragon.rs` `Breath`): its shape and its arithmetic are
//! `dragon::breath` (`Cloud`, `CloudSpec`, `step`), which this module only schedules. A cloud is inert for `WAIT` ticks: it has
//! no `Live` mark and no system visits it. One wake-up at `WAIT` makes it `Live` (its age set so that this tick's step is `WAIT`,
//! where the radius starts to grow); one wake-up at `WAIT + duration` removes it. Between them, `tick_live` visits it each tick.
//! It emits the same `HostEvent::Cloud` as `breath::spawn` (see the notes).
use crate::area;
use crate::cow::{after, OwnTime, Pos, Vel};
use crate::dragon::breath::{self, Cloud, CloudSpec, Step, WAIT};
use crate::goals::Events;
use crate::rules::breath_cloud as rule;
use crate::targeting::{self, Who};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// Marks a cloud past its wait: `tick_live` steps it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Live;

/// Spawns a cloud of `spec` at its floor `at`, asleep until its wait is over.
pub fn spawn(w: &mut World, spec: CloudSpec, at: [f64; 3]) -> Entity {
    spawn_led(w, spec, at, 0)
}

/// `spawn` for a cloud made before the tick's cloud step (the dragon's flame: Horizon ages it on the tick it is made): its
/// timers come `lead` ticks sooner. Saved as an `area_effect_cloud` (`nbt_fields::breath_cloud`).
pub fn spawn_led(w: &mut World, spec: CloudSpec, at: [f64; 3], lead: i32) -> Entity {
    let c = spec.cloud(at);
    let (radius, duration) = (c.radius, c.duration);
    let e = w.spawn((Pos(Vec3::new(at[0], at[1], at[2])), c, OwnTime)).id();
    crate::nbt::mark_saved(w, e, crate::species::lists::AREA_EFFECT_CLOUD);
    w.entity_mut(e).insert(crate::cow::TypeName(crate::species::lists::AREA_EFFECT_CLOUD));
    w.resource_mut::<Events>().host.cloud(at, radius, duration);
    after(w, e, WAIT - lead, wake);
    after(w, e, WAIT + duration - lead, gone);
    e
}

/// A cloud the save kept (an `area_effect_cloud` with no potion): its keys then set its shape and age. None for a potion cloud.
pub fn build(w: &mut World, at: Vec3, c: &horizon_core::nbt::Compound) -> Option<Entity> {
    if c.get(&horizon_core::nbt::JavaString::from("potion_contents")).is_some() {
        return None;
    }
    // `Entity.deltaMovement` starts at zero and `saveWithoutId` writes it as `Motion`.
    let e = w.spawn((Pos(at), Vel(Vec3::ZERO), breath::FLAME.cloud([at.x, at.y, at.z]), OwnTime)).id();
    crate::nbt::mark_saved(w, e, crate::species::lists::AREA_EFFECT_CLOUD);
    w.entity_mut(e).insert(crate::cow::TypeName(crate::species::lists::AREA_EFFECT_CLOUD));
    // Summoned with no `Age` key: age 0 (the keys read after this one, `Duration` among them, set what the timers then check).
    resume(w, e, 0);
    Some(e)
}

/// A loaded cloud of total age `age`: it waits what its wait leaves of that age, then lasts what its duration leaves.
pub fn resume(w: &mut World, e: Entity, age: i32) {
    let Some(duration) = w.get::<Cloud>(e).map(|c| c.duration) else { return };
    if age < WAIT {
        after(w, e, WAIT - age, wake);
    } else if let Ok(mut m) = w.get_entity_mut(e) {
        m.insert(Live);
    }
    after(w, e, (WAIT + duration - age).max(1), gone);
}

/// The wait is over, on the tick `WAIT` after the spawn: the age is `WAIT - 1`, so the step that follows is the tick `WAIT`, as
/// `AreaEffectCloud.serverTick` counts it (`tick_live` runs after the timers).
fn wake(w: &mut World, e: Entity) {
    if w.get::<Live>(e).is_some() {
        return;
    }
    let Some(mut c) = w.get_mut::<Cloud>(e) else { return };
    c.age = WAIT - 1;
    drop(c);
    w.entity_mut(e).insert(Live);
}

/// The cloud's end: it is removed.
pub fn gone(w: &mut World, e: Entity) {
    // A `Duration` read after the timer was armed moves the end: the timer is re-armed for what the cloud's own duration leaves.
    if let Some(cl) = w.get::<Cloud>(e) {
        let age = if w.get::<Live>(e).is_some() { cl.age } else { cl.age + crate::cow::now_of(w, e) as i32 };
        let left = WAIT + cl.duration - age;
        if left > 1 {
            after(w, e, left, gone);
            return;
        }
    }
    w.despawn(e);
}

/// How many clouds `tick_live` has stepped in all: zero while no cloud is past its wait.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudVisits(pub u64);

/// The live clouds, one step each per tick, against the players and creatures their box meets. Players they harm take the
/// victims go out as `HostEvent::CloudHit`: the host applies the damage (`indirect_magic`) and the creatures' effect.
pub fn tick_live(w: &mut World) {
    if !crate::goals::sparse_any::<Live>(w) {
        return;
    }
    let ids: Vec<Entity> = crate::queries::entities::<With<Live>>(w);
    if ids.is_empty() {
        return;
    }
    w.get_resource_or_insert_with(CloudVisits::default).0 += ids.len() as u64;
    for e in ids {
        let Some(mut c) = w.get::<Cloud>(e).cloned() else { continue };
        let area = rule::scan_box(c.at, rule::cloud_reach(c.radius, c.radius_per_tick));
        let mut cands = Vec::new();
        for id in area::players_in(w, area) {
            let who = Who::Player(id);
            if let Some(i) = targeting::info(w, who) {
                cands.push((who, i.bounds, i.pos));
            }
        }
        for m in area::creatures_in(w, area, e) {
            let who = Who::Entity(m);
            if let Some(i) = targeting::info(w, who) {
                cands.push((who, i.bounds, i.pos));
            }
        }
        let (amount, at, amplifier) = (breath::harm(c.amplifier), c.at, c.amplifier);
        match breath::step(&mut c, &cands) {
            Step::Gone => {
                w.despawn(e);
            }
            Step::Live(hit) => {
                *w.get_mut::<Cloud>(e).expect("cloud") = c;
                let (mut players, mut mobs) = (Vec::new(), Vec::new());
                for who in hit {
                    match who {
                        Who::Player(id) => players.push(id),
                        Who::Entity(m) => mobs.push(m),
                    }
                }
                if !players.is_empty() || !mobs.is_empty() {
                    w.resource_mut::<Events>().host.cloud_hit(at, amount, amplifier, players, mobs);
                }
            }
        }
    }
}
