//! The game events of a movement (`Entity.move`: `checkFallDamage`, `applyMovementEmissionAndPlaySound`, `updateFluidInteraction`): `step`,
//! `swim`, `flap`, `hit_ground`, `splash`, each with its entity as the source, for the vibration listeners (sculk sensors, the warden). Off
//! unless the host asks (`MoveEventsOn`); only an entity that moved this tick (or the tick before, to see it stop) is visited, a resting one costs nothing.
use crate::behaviours::move_emission::{Cfg, Flap, Params};
use crate::cow::{Body, Clock, Contact, FluidState, Moving, Pos};
use crate::goals::Events;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// Set by the host while a vibration listener may hear (a sculk block is loaded, a mod hears game events).
#[derive(Resource, Default)]
pub struct MoveEventsOn(pub bool);

/// `Entity.moveDist` / `nextStep` (apart: the state of the walk, sparse, inserted at the first movement seen) and what the last tick left.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct StepState {
    last: Vec3,
    dist: f32,
    next: f32,
    was_ground: bool,
    was_water: bool,
    fall: f64,
}

/// `Pose.CROUCHING` and `Pose.SWIMMING`.
const POSE_CROUCHING: u8 = 5;
const POSE_SWIMMING: u8 = 3;

fn on(g: Option<Res<MoveEventsOn>>) -> bool {
    g.is_some_and(|g| g.0)
}

type Visit<'a> = (
    Entity,
    &'a Pos,
    &'a Contact,
    &'a FluidState,
    &'a Body,
    Option<&'a mut StepState>,
    Option<&'a crate::pose_state::PoseOverride>,
    Option<&'a Cfg>,
    Has<Moving>,
);

/// `Entity.moveDist += 0.6 * horizontal distance`, then the emission when it passes `nextStep`. The distance is the one between this tick's
/// position and the last seen, which is the applied movement (a `do_move` is the only thing that moves a visited entity).
pub fn move_events(clock: Res<Clock>, mut ev: ResMut<Events>, mut cmd: Commands, mut q: Query<Visit, Or<(With<Moving>, With<StepState>)>>) {
    for (e, p, c, fs, b, st, pose, cfg, moving) in &mut q {
        if b.object || b.orb || b.tnt {
            continue;
        }
        let cfg: Params = cfg.map_or(Params::DEFAULT, |c| *c.0);
        let fresh = StepState { last: p.0, dist: 0.0, next: 1.0, was_ground: c.on_ground, was_water: fs.in_water, fall: 0.0 };
        let Some(mut s) = st else {
            cmd.entity(e).insert(fresh);
            continue;
        };
        let at = [p.0.x, p.0.y, p.0.z];
        let pose = pose.map_or(0, |o| o.0);
        let sneaking = pose == POSE_CROUCHING;
        let (dx, dz) = (p.0.x - s.last.x, p.0.z - s.last.z);
        s.dist += (0.6_f64 * (dx * dx + dz * dz).sqrt()) as f32;
        // `getOnPos` air: nothing under the feet and not in a fluid.
        let air = !c.on_ground && !fs.in_water;
        if cfg.events {
            if air {
                if flaps(&cfg, e, clock.of(e), c.on_ground) {
                    ev.host.game_event_by(e, "minecraft:flap", at, false, sneaking, cfg.dampens);
                }
            } else if s.dist > s.next {
                // `vibrationAndSoundEffectsFromBlock`: a step on the ground (not swimming); else, in water, a swim.
                let stepped = c.on_ground && pose != POSE_SWIMMING;
                if stepped || fs.in_water {
                    s.next = if cfg.step_gap > 0.0 { s.dist + cfg.step_gap } else { s.dist.floor() + 1.0 };
                }
                if stepped {
                    ev.host.game_event_by(e, "minecraft:step", at, true, sneaking, cfg.dampens);
                } else if fs.in_water {
                    ev.host.game_event_by(e, "minecraft:swim", at, false, sneaking, cfg.dampens);
                }
            }
        }
        // `updateFluidInteraction`: entering water splashes. `checkFallDamage`: a landing after a fall hits the ground.
        if !s.was_water && fs.in_water {
            ev.host.game_event_by(e, "minecraft:splash", at, false, sneaking, cfg.dampens);
        }
        if !s.was_ground && c.on_ground && s.fall > 0.0 {
            ev.host.game_event_by(e, "minecraft:hit_ground", at, true, sneaking, cfg.dampens);
        }
        s.fall = if c.on_ground { 0.0 } else { c.fall_distance.max(s.fall) };
        (s.last, s.was_ground, s.was_water) = (p.0, c.on_ground, fs.in_water);
        // The tick that stopped it (`travel` took `Moving` off) is the last one seen: a landing is in it.
        if !moving {
            cmd.entity(e).remove::<StepState>();
        }
    }
}

/// `isFlapping()` of the entity's type, `age` being its own tick count.
fn flaps(cfg: &Params, e: Entity, age: u64, on_ground: bool) -> bool {
    match cfg.flap {
        Flap::Never => false,
        Flap::Airborne => !on_ground,
        Flap::Every { period, flying_only, offset } => {
            let shift = if offset { u64::from(e.index().index()) * 3 } else { 0 };
            (!flying_only || !on_ground) && (age + shift) % u64::from(period) == 0
        }
    }
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(move_events.after(crate::cow::travel).after(crate::locomotion::fly::travel).run_if(on));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flap_periods() {
        let e = Entity::PLACEHOLDER;
        let bee = Params { flap: Flap::Every { period: 2, flying_only: true, offset: false }, ..Params::DEFAULT };
        assert!(flaps(&bee, e, 4, false) && !flaps(&bee, e, 5, false) && !flaps(&bee, e, 4, true));
        let allay = Params { flap: Flap::Airborne, ..Params::DEFAULT };
        assert!(flaps(&allay, e, 7, false) && !flaps(&allay, e, 7, true));
        assert!(!flaps(&Params::DEFAULT, e, 0, false));
    }
}
