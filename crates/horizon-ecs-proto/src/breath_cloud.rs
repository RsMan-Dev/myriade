//! The area cloud as a sleeping entity (`AreaEffectCloud`, Horizon `dragon.rs` `Breath`): its shape and its arithmetic are
//! `dragon::breath` (`Cloud`, `CloudSpec`, `step`), which this module only schedules. A cloud is inert for `WAIT` ticks: it has
//! no `Live` mark and no system visits it. One wake-up at `WAIT - 1` makes it `Live` (its age set so that the next tick is `WAIT`,
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
    let c = spec.cloud(at);
    let (radius, duration) = (c.radius, c.duration);
    let e = w.spawn((Pos(Vec3::new(at[0], at[1], at[2])), c, OwnTime)).id();
    w.resource_mut::<Events>().host.cloud(at, radius, duration);
    after(w, e, WAIT - 1, wake);
    after(w, e, WAIT + duration, gone);
    e
}

/// The wait is over: the age is `WAIT - 1`, so the next step is the tick `WAIT`, as `AreaEffectCloud.serverTick` counts it.
fn wake(w: &mut World, e: Entity) {
    let Some(mut c) = w.get_mut::<Cloud>(e) else { return };
    c.age = WAIT - 1;
    drop(c);
    w.entity_mut(e).insert(Live);
}

fn gone(w: &mut World, e: Entity) {
    w.despawn(e);
}

/// The live clouds, one step each per tick, against the players and creatures their box meets. Players they harm take the
/// instant damage. Not done: the creatures' effect (`breath_affect_mob`) and the damage source `indirect_magic`.
pub fn tick_live(w: &mut World) {
    let ids: Vec<Entity> = w.query_filtered::<Entity, With<Live>>().iter(w).collect();
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
