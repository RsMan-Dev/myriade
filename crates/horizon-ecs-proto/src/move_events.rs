//! The game events of a movement (`Entity.move`: `checkFallDamage`, `applyMovementEmissionAndPlaySound`, `updateFluidInteraction`): `step`,
//! `swim`, `flap`, `hit_ground`, `splash`, each with its entity as the source, for the vibration listeners (sculk sensors, the warden), and
//! the sounds of the same movement (step, swim, splash). The game events are off unless the host asks (`MoveEventsOn`), the sounds never;
//! only an entity that moved this tick (or the tick before, to see it stop) is visited, a resting one costs nothing.
use crate::behaviours::move_emission::{Cfg, Flap, Params};
use crate::behaviours::swim_sound::{splash_volume, swim_sound, SwimSounds};
use crate::cow::{Body, Clock, Contact, FluidState, Level, Moving, Pos, Rng, Vel};
use crate::mount::Mount;
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
    Option<&'a mut Rng>,
    Option<&'a Vel>,
    Has<crate::behaviours::elder_curse::Silent>,
    Option<&'a mut SwimSounds>,
    Option<&'a Mount>,
    Has<crate::cow::GrowAt>,
);

/// The pitch of a splash or a swim, drawn from the entity's stream (`1` for one without a stream).
fn splash_pitch(rng: Option<&mut Rng>) -> f32 {
    rng.map_or(1.0, |r| {
        let a = (r.next_u32() >> 8) as f32 / 16_777_216.0;
        let b = (r.next_u32() >> 8) as f32 / 16_777_216.0;
        (a - b) * 0.4 + 1.0
    })
}

/// `Entity.moveDist += 0.6 * horizontal distance`, then the emission when it passes `nextStep`. The distance is the one between this tick's
/// position and the last seen, which is the applied movement (a `do_move` is the only thing that moves a visited entity).
pub fn move_events(
    clock: Res<Clock>,
    level: Res<Level>,
    heard: Option<Res<MoveEventsOn>>,
    mut ev: ResMut<Events>,
    mut cmd: Commands,
    mut q: Query<Visit, Or<(With<Moving>, With<StepState>)>>,
) {
    // The sounds of a movement play for the players whatever listens; only its game events wait for a vibration listener.
    let heard = on(heard);
    for (e, p, c, fs, b, st, pose, cfg, moving, mut rng, vel, silent, mut sw, mount, baby) in &mut q {
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
                if heard && flaps(&cfg, e, clock.of(e), c.on_ground) {
                    ev.host.game_event_by(e, "minecraft:flap", at, false, sneaking, cfg.dampens);
                }
            } else if s.dist > s.next {
                // `vibrationAndSoundEffectsFromBlock`: a step on the ground (not swimming); else, in water, a swim.
                let stepped = c.on_ground && pose != POSE_SWIMMING;
                if stepped || fs.in_water {
                    s.next = if cfg.step_gap > 0.0 { s.dist + cfg.step_gap } else { s.dist.floor() + 1.0 };
                }
                if stepped {
                    // `playStepSound`: the step sound of the block under the feet (`getOnPos`, 0.2 below), at 0.15 of its volume.
                    if cfg.sounds {
                        let (x, y, z) = (p.0.x.floor() as i32, (p.0.y - 0.2).floor() as i32, p.0.z.floor() as i32);
                        let facts = level.mw.0.facts.get(level.mw.0.world.block(x, y, z));
                        if !(&*facts.name == "minecraft:air" || facts.name.ends_with("_air")) {
                            ev.host.sound(facts.sounds.step.to_string(), at, facts.sounds.volume * 0.15, facts.sounds.pitch);
                        }
                    }
                    if heard {
                        ev.host.game_event_by(e, "minecraft:step", at, true, sneaking, cfg.dampens);
                    }
                } else if fs.in_water {
                    // `waterSwimSound` (before the event): the species' own sound, `entity.generic.swim` without a `SwimSounds`; its volume is
                    // the speed (`min(1, 0.35 * |v|)`, the horizontal part at 0.2), `playSwimSound`'s for a species that has its own.
                    if cfg.sounds && !silent {
                        let [x, y, z] = vel.map_or([0.0; 3], |v| [v.0.x, v.0.y, v.0.z]);
                        let f = ((x * x * 0.2 + y * y + z * z * 0.2).sqrt() as f32 * 0.35).min(1.0);
                        let (name, volume) = match sw.as_deref_mut() {
                            Some(sw) => {
                                let ridden = mount.is_some_and(|m| m.rider.is_some());
                                let name = match sw.baby {
                                    Some(b) if baby => b,
                                    _ => swim_sound(&sw.sounds, c.on_ground, ridden, &mut sw.steps),
                                };
                                (name, splash_volume(c.on_ground, f))
                            }
                            None => ("entity.generic.swim", f),
                        };
                        ev.host.sound(name.to_string(), at, volume, splash_pitch(rng.as_deref_mut()));
                    }
                    if heard {
                        ev.host.game_event_by(e, "minecraft:swim", at, false, sneaking, cfg.dampens);
                    }
                }
            }
        }
        // `updateFluidInteraction`: entering water splashes. `checkFallDamage`: a landing after a fall hits the ground.
        if !s.was_water && fs.in_water {
            if heard {
                ev.host.game_event_by(e, "minecraft:splash", at, false, sneaking, cfg.dampens);
            }
            // `Entity.doWaterSplashSound`: the generic splash, its volume the speed (`min(1, 0.2 * |v|)`) and a pitch from the entity's stream.
            if !silent {
                let [x, y, z] = vel.map_or([0.0; 3], |v| [v.0.x, v.0.y, v.0.z]);
                let speed = ((x * x * 0.2 + y * y + z * z * 0.2).sqrt() as f32 * 0.2).min(1.0);
                let pitch = splash_pitch(rng.as_deref_mut());
                ev.host.sound("entity.generic.splash", at, speed, pitch);
            }
        }
        if heard && !s.was_ground && c.on_ground && s.fall > 0.0 {
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
    s.add_systems(move_events.after(crate::cow::travel).after(crate::locomotion::fly::travel));
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
