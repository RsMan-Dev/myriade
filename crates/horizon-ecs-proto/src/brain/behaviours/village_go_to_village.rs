//! `GoToClosestVillage` (`villager_brain.rs:2309`): 5 draws, the first nearer village wins. The villages are the sections of the
//! villager's own POI memories and of the villagers it sees (`village_sections`), not of the whole registry.
use crate::brain::*;
use crate::cow::{Pos, SpeciesId};
use crate::poi::PoiRegistry;
use crate::spots::Pick;
use crate::species::villager::{HOME, JOB_SITE, MEETING_POINT, POTENTIAL_JOB_SITE};
use crate::targeting::Who;

pub struct Params {
    pub speed: f32,
    pub h: i32,
    pub v: i32,
    pub close: i32,
}

pub static BEHAVIOUR: Behaviour = Behaviour { name: "minecraft:go_to_closest_village", mems: &[&WALK_TARGET], one_shot: true, can_start, ..Behaviour::DEFAULT };

const VILLAGE_CAP: i32 = 7;

fn section(p: [f64; 3]) -> [i32; 3] {
    [p[0].floor() as i32 >> 4, p[1].floor() as i32 >> 4, p[2].floor() as i32 >> 4]
}

fn poi_block(w: &World, e: Entity, k: &MemKey) -> Option<[i32; 3]> {
    match get(w, e, k) {
        Some(MemValue::Block(p)) => Some(*p),
        _ => None,
    }
}

/// The POI of a villager's memories that is still a POI, in the sections, and the same for the villagers it sees.
fn village_sections(w: &mut World, e: Entity) -> Vec<[i32; 3]> {
    let keys = [&JOB_SITE, &POTENTIAL_JOB_SITE, &HOME, &MEETING_POINT];
    let mut pois: Vec<[i32; 3]> = keys.into_iter().filter_map(|k| poi_block(w, e, k)).collect();
    let own = w.get::<SpeciesId>(e).map(|s| s.0);
    for who in find_all(w, e, |w, who| matches!(who, Who::Entity(t) if own.is_some() && w.get::<SpeciesId>(t).map(|s| s.0) == own)) {
        if let Who::Entity(t) = who {
            pois.extend(keys.into_iter().filter_map(|k| poi_block(w, t, k)));
        }
    }
    let reg = w.resource::<PoiRegistry>();
    let mut out: Vec<[i32; 3]> = pois.into_iter().filter(|&p| reg.get(p).is_some()).map(|p| [p[0] >> 4, p[1] >> 4, p[2] >> 4]).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// `sectionsToVillage`: the Chebyshev distance in sections to the nearest village section, at most 7.
fn to_village(villages: &[[i32; 3]], s: [i32; 3]) -> i32 {
    villages.iter().map(|v| (0..3).map(|i| (v[i] - s[i]).abs()).max().unwrap_or(0)).min().unwrap_or(VILLAGE_CAP).min(VILLAGE_CAP)
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if present(w, e, &WALK_TARGET) {
        return false;
    }
    let Some(at) = w.get::<Pos>(e).map(|p| [p.0.x, p.0.y, p.0.z]) else { return false };
    let villages = village_sections(w, e);
    let here = to_village(&villages, section(at));
    if here <= 1 {
        return false;
    }
    let p = c.get::<Params>();
    let mut target = None;
    for _ in 0..5 {
        let Some(q) = pick(w, e, Pick::Land { h: p.h, v: p.v }) else { continue };
        let d = to_village(&villages, section(q));
        if d < here {
            target = Some(q);
            break;
        }
        if d == here {
            target = Some(q);
        }
    }
    if let Some(q) = target {
        set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Point(q), speed: p.speed, close: p.close }), None);
    }
    true
}
