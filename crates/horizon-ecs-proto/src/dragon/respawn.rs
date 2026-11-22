//! The dragon's respawn (`tick_respawn` D:4130-4250, `rebuild_spike` D:4402-4512, `finish_respawn` D:4581, `abort_respawn` D:4640).
//! The stages run on the own time of the first respawn crystal: one re-arm per tick while `respawn` is set, nothing otherwise.
//! `try_respawn` (the choice of the four portal crystals, D:4252) asks `start`.
use crate::blocks::{set_state, set_state_id, state_id, resolve, world_of};
use crate::cow::*;
use crate::dragon::crystals::{Beam, EndCrystal, Shielded};
use crate::dragon::fight::{EndFight, RespawnStage};
use crate::explosion::{explode, explode_with};
use crate::goals::Events;
use crate::projectile::Blast;
use crate::rules::dragon_respawn as rule;
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

/// `try_respawn` (D:4252): after a kill, with no respawn running, one crystal (or more) in the cube on each of the four sides of the
/// exit portal, three blocks out and one up, begins the respawn: the podium goes back to end stone and inactive. False when any
/// side has none. The caller asks it when a crystal is placed there, never per tick.
pub fn try_respawn(w: &mut World) -> bool {
    {
        let f = w.resource::<EndFight>();
        if !f.killed || f.respawn.is_some() {
            return false;
        }
    }
    if w.resource::<EndFight>().exit_portal.is_none() {
        crate::dragon::portal::activate_exit_portal(w);
    }
    let Some(origin) = w.resource::<EndFight>().exit_portal else { return false };
    let mut found = Vec::new();
    let mut q = w.query_filtered::<(Entity, &Pos), Or<(With<EndCrystal>, With<crate::end_crystal::EndCrystal>)>>();
    let all: Vec<(Entity, Vec3)> = q.iter(w).map(|(e, p)| (e, p.0)).collect();
    for d in [[0, -1], [0, 1], [-1, 0], [1, 0]] {
        let cell = rule::side_cell(origin, d);
        let hits: Vec<Entity> = all.iter().filter(|(_, p)| crate::dragon::crystals::bounds(*p).intersects(cell)).map(|(e, _)| *e).collect();
        if hits.is_empty() {
            return false;
        }
        found.extend(hits);
    }
    let mw = world_of(w);
    let ids = (resolve(mw, "minecraft:bedrock", &[]), resolve(mw, "minecraft:end_portal", &[]));
    for y in origin[1] - 1..=origin[1] + 3 {
        for z in origin[2] - 3..=origin[2] + 3 {
            for x in origin[0] - 3..=origin[0] + 3 {
                let id = state_id(mw, [x, y, z]);
                if Some(id) == ids.0 || Some(id) == ids.1 {
                    set_state(w, [x, y, z], "minecraft:end_stone", &[]);
                }
            }
        }
    }
    crate::dragon::portal::place_podium(w, origin, false);
    start(w, found);
    true
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
    w.resource_mut::<Events>().host.level_event(3001, [0, 128, 0], 0);
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
                if rule::preparing_growl(time) {
                    growl(w);
                }
            } else {
                set_stage(w, RespawnStage::Pillars);
            }
        }
        RespawnStage::Pillars => {
            let (start, end) = rule::pillar_edges(time);
            if start || end {
                let spikes = spikes_for_seed(crate::blocks::world_of(w).world_seed);
                match spikes.get(rule::spike_index(time)) {
                    Some(spike) if start => set_beams(w, Some([spike.center_x, spike.height + 1, spike.center_z])),
                    Some(spike) => rebuild(w, spike),
                    None if start => set_stage(w, RespawnStage::Summoning),
                    None => {}
                }
            }
        }
        RespawnStage::Summoning => {
            if time >= 100 {
                finish(w);
                return;
            } else if rule::summoning_growl(time) {
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
    for y in 0..=h + 10 {
        for z in cz - spike.radius..=cz + spike.radius {
            for x in cx - spike.radius..=cx + spike.radius {
                if rule::in_disc(x - cx, z - cz, spike.radius) && y < h {
                    set_state(w, [x, y, z], "minecraft:obsidian", &[]);
                }
            }
        }
    }
    if spike.guarded {
        pillar_bars(w, spike);
    }
    let crystal = Vec3::new(f64::from(cx) + 0.5, f64::from(h + 1), f64::from(cz) + 0.5);
    let e = crate::dragon::crystals::spawn(w, crystal, true);
    w.entity_mut(e).insert((Shielded, Beam(CENTER)));
    set_state(w, [cx, h, cz], "minecraft:bedrock", &[]);
    set_state(w, [cx, h + 1, cz], "minecraft:fire", &[]);
}

/// The cage of a guarded spike (`EndSpikeFeature`'s iron bars): a ring five wide and a roof, four high from the top, each bar joined
/// to its neighbours north, south, west and east as the edge of the cage lies.
fn pillar_bars(w: &mut World, spike: &EndSpike) {
    for dx in -2i32..=2 {
        for dz in -2i32..=2 {
            for dy in 0..=3 {
                let Some(sides) = rule::cage_sides(dx, dz, dy) else { continue };
                let props: Vec<(&str, &str)> = ["north", "south", "west", "east"].into_iter().zip(sides).map(|(k, on)| (k, if on { "true" } else { "false" })).collect();
                if let Some(id) = resolve(world_of(w), "minecraft:iron_bars", &props) {
                    set_state_id(w, [spike.center_x + dx, spike.height + dy, spike.center_z + dz], id);
                }
            }
        }
    }
}

/// The respawn is over (`finish_respawn`): beams go, the spike crystals are unshielded (`reset_spike_crystals`), the portal crystals blow up, and the new dragon is summoned.
fn finish(w: &mut World) {
    set_beams(w, None);
    let spikes = spikes_for_seed(world_of(w).world_seed);
    let on_spike: Vec<Entity> = {
        let mut q = w.query_filtered::<(Entity, &Pos), With<EndCrystal>>();
        let on = |p: Vec3| spikes.iter().any(|s| rule::is_on_spike([p.x, p.y, p.z], s.center_x, s.center_z, s.radius, s.height));
        q.iter(w).filter(|(_, p)| on(p.0)).map(|(e, _)| e).collect()
    };
    for e in on_spike {
        w.entity_mut(e).remove::<(Shielded, Beam)>();
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
        explode_with(w, Blast { at, radius: 6.0, fire: false, owner: None, source: None }, false);
    }
    spawn_new_dragon(w);
}

/// `createNewDragon`: over the podium, in the holding pattern, a random yaw; it becomes the fight's dragon. Its generators are seeded
/// from the world's seed and its own uid (Horizon uses a process-unique seed: the stream is the dragon's, not the level's).
pub fn spawn_new_dragon(w: &mut World) -> Entity {
    use crate::dragon::phases;
    use horizon_core::random::{LegacyRandom, RandomSource};
    let uid = crate::goals::next_uid();
    let seed = world_of(w).world_seed ^ uid as i64;
    let yaw = LegacyRandom::new(seed).next_float() * 360.0;
    let e = summon(w, Vec3::new(0.0, 128.0, 0.0), yaw, seed, phases::HOLDING);
    w.resource_mut::<EndFight>().dragon = Some(e);
    e
}

/// A dragon at `at` with `yaw`, its generators seeded from `seed`, born in `phase` (`/summon`, an egg, the fight's spawn). Not the fight's.
pub fn summon(w: &mut World, at: Vec3, yaw: f32, seed: i64, phase: u8) -> Entity {
    use crate::dragon::{flight::{DragonRng, FlightHistory}, parts::Parts, phases};
    use horizon_core::random::LegacyRandom;
    let sp = *w.resource::<SpeciesTable>().0.iter().find(|s| s.name == crate::species::ENDER_DRAGON).expect("the dragon's species");
    let e = spawn_species(w, &sp, at.x, at.z, seed as u64);
    w.entity_mut(e).insert((Pos(at), Yaw(yaw), FlightHistory::new(at.y, yaw), Parts::default(), DragonRng(LegacyRandom::new(seed.wrapping_add(1)), LegacyRandom::new(seed.wrapping_add(2)))));
    phases::set_phase(w, e, phase);
    e
}
