//! The area cloud as a sleeping entity (`AreaEffectCloud`, Horizon `dragon.rs` `Breath`): its shape and its arithmetic are
//! `dragon::breath` (`Cloud`, `CloudSpec`, `step`), which this module only schedules. A cloud is inert for `WAIT` ticks: it has
//! no `Live` mark and no system visits it. One wake-up at `WAIT` makes it `Live` (its age set so that this tick's step is `WAIT`,
//! where the radius starts to grow); one wake-up at `WAIT + duration` removes it. Between them, `tick_live` visits it each tick.
//! It emits the same `HostEvent::Cloud` as `breath::spawn` (see the notes).
use crate::area;
use crate::cow::{after, OwnTime, Pos};
use crate::dragon::breath::{self, Cloud, CloudSpec, Step, WAIT};
use crate::goals::Events;
use crate::targeting::{self, Who};
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

/// Marks a cloud past its wait: `tick_live` steps it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Live;

/// Spawns a cloud of `spec` at its floor `at`, asleep until its wait is over.
pub fn spawn(w: &mut World, spec: CloudSpec, at: [f64; 3]) -> Entity {
    spawn_led(w, spec, at, 0)
}

/// `spawn` for a cloud made before the tick's cloud step (the dragon's flame: Horizon ages it on the tick it is made): its
/// timers come `lead` ticks sooner.
pub fn spawn_led(w: &mut World, spec: CloudSpec, at: [f64; 3], lead: i32) -> Entity {
    let c = spec.cloud(at);
    let (radius, duration) = (c.radius, c.duration);
    let e = w.spawn((Pos(Vec3::new(at[0], at[1], at[2])), c, OwnTime)).id();
    w.resource_mut::<Events>().host.cloud(at, radius, duration);
    after(w, e, WAIT - lead, wake);
    after(w, e, WAIT + duration - lead, gone);
    e
}

/// The wait is over, on the tick `WAIT` after the spawn: the age is `WAIT - 1`, so the step that follows is the tick `WAIT`, as
/// `AreaEffectCloud.serverTick` counts it (`tick_live` runs after the timers).
fn wake(w: &mut World, e: Entity) {
    let Some(mut c) = w.get_mut::<Cloud>(e) else { return };
    c.age = WAIT - 1;
    drop(c);
    w.entity_mut(e).insert(Live);
}

fn gone(w: &mut World, e: Entity) {
    w.despawn(e);
}

/// How many clouds `tick_live` has stepped in all: zero while no cloud is past its wait.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudVisits(pub u64);

/// The live clouds, one step each per tick, against the players and creatures their box meets. Players they harm take the
/// instant damage. Not done: the creatures' effect (`breath_affect_mob`) and the damage source `indirect_magic`.
pub fn tick_live(w: &mut World) {
    let ids: Vec<Entity> = w.query_filtered::<Entity, With<Live>>().iter(w).collect();
    if ids.is_empty() {
        return;
    }
    w.get_resource_or_insert_with(CloudVisits::default).0 += ids.len() as u64;
    for e in ids {
        let Some(mut c) = w.get::<Cloud>(e).cloned() else { continue };
        let reach = c.radius.max(c.radius + c.radius_per_tick);
        let [x, y, z] = c.at;
        let area = Aabb::new(Vec3::new(x - f64::from(reach), y, z - f64::from(reach)), Vec3::new(x + f64::from(reach), y + 0.5, z + f64::from(reach)));
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
        let amount = breath::harm(c.amplifier);
        match breath::step(&mut c, &cands) {
            Step::Gone => {
                w.despawn(e);
            }
            Step::Live(hit) => {
                *w.get_mut::<Cloud>(e).expect("cloud") = c;
                for who in hit {
                    if let Who::Player(id) = who {
                        area::hurt_player(w, id, amount, e);
                    }
                }
            }
        }
    }
}
