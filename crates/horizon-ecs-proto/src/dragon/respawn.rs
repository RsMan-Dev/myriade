//! The dragon's respawn (`tick_respawn` D:4130-4250, `rebuild_spike` D:4402-4512, `finish_respawn` D:4581, `abort_respawn` D:4640).
//! The stages run on the own time of the first respawn crystal: one re-arm per tick while `respawn` is set, nothing otherwise.
//! `try_respawn` (the choice of the four portal crystals, D:4252) is not here: its caller passes them to `start`.
use crate::blocks::{set_state, set_state_id, state_id, resolve, world_of};
use crate::cow::*;
use crate::dragon::crystals::{Beam, EndCrystal, Shielded};
use crate::dragon::fight::{EndFight, RespawnStage};
use crate::explosion::explode;
use crate::goals::Events;
use crate::projectile::Blast;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_world::worldgen::end_spike::{spikes_for_seed, EndSpike};

const CENTER: [i32; 3] = [0, 128, 0];

/// Starts a respawn with the four portal crystals; the first of them carries the timer.
pub fn start(w: &mut World, portal: Vec<Entity>) {
    let Some(&timer) = portal.first() else { return };
    {
        let f = w.resource_mut::<EndFight>().into_inner();
        f.respawn = Some((RespawnStage::Start, 0));
        f.respawn_crystals = portal;
    }
    after(w, timer, 1, tick);
}

fn set_stage(w: &mut World, s: RespawnStage) {
    w.resource_mut::<EndFight>().respawn = Some((s, 0));
}

/// Ends the respawn before its end (a respawn crystal was destroyed, `crystal_destroyed`): the beams go, the stage is cleared.
pub fn abort(w: &mut World) {
    let list = {
        let f = w.resource_mut::<EndFight>().into_inner();
        f.respawn = None;
        std::mem::take(&mut f.respawn_crystals)
    };
    for c in list {
        if w.get_entity(c).is_ok() {
            w.entity_mut(c).remove::<Beam>();
        }
    }
}

fn set_beams(w: &mut World, target: Option<[i32; 3]>) {
    let list = w.resource::<EndFight>().respawn_crystals.clone();
    for c in list {
        if w.get_entity(c).is_err() {
            continue;
        }
        match target {
            Some(t) => {
                w.entity_mut(c).insert(Beam(t));
            }
            None => {
                w.entity_mut(c).remove::<Beam>();
            }
        }
    }
}

fn growl(w: &mut World) {
    w.resource_mut::<Events>().host.level_event(3001, [0.5, 128.5, 0.5], 0);
}

/// One tick of the respawn: the stage's act at the stage's tick, the next stage at its end.
fn tick(w: &mut World, c: Entity) {
    let Some((stage, time)) = w.resource::<EndFight>().respawn else { return };
    let alive = w.resource::<EndFight>().respawn_crystals.iter().all(|&p| w.get_entity(p).is_ok());
    if !alive {
        abort(w);
        return;
    }
    w.resource_mut::<EndFight>().respawn = Some((stage, time + 1));
    match stage {
        RespawnStage::Start => {
            set_beams(w, Some(CENTER));
            set_stage(w, RespawnStage::Preparing);
        }
        RespawnStage::Preparing => {
            if time < 100 {
                if matches!(time, 0 | 50..=52) || time >= 95 {
                    growl(w);
                }
            } else {
                set_stage(w, RespawnStage::Pillars);
            }
        }
        RespawnStage::Pillars => {
            let (start, end) = (time % 40 == 0, time % 40 == 39);
            if !(start || end) {
                return;
            }
            let seed = w.resource::<WorldSeed>().0;
            let spikes = spikes_for_seed(seed);
            match spikes.get((time / 40) as usize) {
                Some(spike) if start => set_beams(w, Some([spike.center_x, spike.height + 1, spike.center_z])),
                Some(spike) => rebuild(w, spike),
                None if start => set_stage(w, RespawnStage::Summoning),
                None => {}
            }
        }
        RespawnStage::Summoning => {
            if time >= 100 {
                finish(w);
                return;
            } else if time >= 80 || (1..5).contains(&time) {
                growl(w);
            } else if time == 0 {
                set_beams(w, Some(CENTER));
            }
        }
    }
    if w.resource::<EndFight>().respawn.is_some() && w.get_entity(c).is_ok() {
        after(w, c, 1, tick);
    }
}

/// The spike is cleared, blown up and grown again with an invulnerable beam crystal (`rebuild_spike`).
fn rebuild(w: &mut World, spike: &EndSpike) {
    let (cx, cz, h) = (spike.center_x, spike.center_z, spike.height);
    let Some(air) = resolve(world_of(w), "minecraft:air", &[]) else { return };
    for y in h - 10..=h + 10 {
        for z in cz - 10..=cz + 10 {
            for x in cx - 10..=cx + 10 {
                if state_id(world_of(w), [x, y, z]) != air && y > 65 {
                    set_state_id(w, [x, y, z], air);
                }
            }
        }
    }
    let at = Vec3::new(f64::from(cx) + 0.5, f64::from(h), f64::from(cz) + 0.5);
    explode(w, Blast { at, radius: 5.0, fire: false, owner: None, source: None });
    let r = f64::from(spike.radius);
    for y in 0..=h + 10 {
        for z in cz - spike.radius..=cz + spike.radius {
            for x in cx - spike.radius..=cx + spike.radius {
                let (dx, dz) = (f64::from(x - cx), f64::from(z - cz));
                if dx * dx + dz * dz <= r * r + 1.0 && y < h {
                    set_state(w, [x, y, z], "minecraft:obsidian", &[]);
                }
            }
        }
    }
    let crystal = Vec3::new(f64::from(cx) + 0.5, f64::from(h + 1), f64::from(cz) + 0.5);
    let e = crate::dragon::crystals::spawn(w, crystal, true);
    w.entity_mut(e).insert((Shielded, Beam(CENTER)));
    set_state(w, [cx, h, cz], "minecraft:bedrock", &[]);
    set_state(w, [cx, h + 1, cz], "minecraft:fire", &[]);
}

/// The respawn is over (`finish_respawn`): beams and rebuilt crystals go, the portal crystals blow up, and the new dragon is summoned.
fn finish(w: &mut World) {
    set_beams(w, None);
    let shielded: Vec<Entity> = {
        let mut q = w.query_filtered::<Entity, (With<Shielded>, With<EndCrystal>)>();
        q.iter(w).collect()
    };
    for e in shielded {
        w.despawn(e);
        let f = w.resource_mut::<EndFight>().into_inner();
        f.crystals_alive = f.crystals_alive.saturating_sub(1);
    }
    let list = {
        let f = w.resource_mut::<EndFight>().into_inner();
        f.respawn = None;
        f.killed = false;
        std::mem::take(&mut f.respawn_crystals)
    };
    for c in list {
        let Some(at) = w.get::<Pos>(c).map(|p| p.0) else { continue };
        w.despawn(c);
        explode(w, Blast { at, radius: 6.0, fire: false, owner: None, source: None });
    }
    crate::dragon::spawn_new_dragon(w);
}
