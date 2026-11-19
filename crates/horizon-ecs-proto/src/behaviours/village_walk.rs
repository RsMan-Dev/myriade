//! The walks of the creatures that keep to a village: `GolemRandomStrollInVillageGoal` (now and then anywhere, else toward a point a villager
//! holds in a village section within two sections of it) and `MoveBackToVillageGoal` (only outside the village: toward the section nearest a
//! village). The village is the registry's (`poi.rs`). Each entry has a timer of its own (the wait is `idle_wait` of its interval) and a
//! speed; the path ends the goal. Missing: the villager who wants a golem (the villager's brain).
use super::*;
use crate::poi::PoiRegistry;
use crate::spots::{pick_for, Pick};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Anywhere three times in ten, else toward a held point of a village section nearby.
    InVillage,
    /// Only outside the village (more than one section from it): toward the nearest village section.
    Back,
}
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub kind: Kind,
    pub speed: f64,
    /// One chance in `reduced(interval)` per ask, one ask every two ticks.
    pub interval: i32,
}
impl Params {
    pub const IN_VILLAGE: Self = Self { kind: Kind::InVillage, speed: 0.6, interval: 240 };
    pub const BACK: Self = Self { kind: Kind::Back, speed: 0.6, interval: 10 };
}
#[derive(Clone, Copy, Debug)]
pub struct Walk {
    p: Params,
    slot: u8,
    pick: Option<[f64; 3]>,
    armed: bool,
}
/// The walks of the entity, by `Inst::nth`.
#[derive(Component, Clone, Debug)]
pub struct VillageWalks(pub Vec<Walk>);

pub static MODULE: Module = Module {
    name: "village_walk",
    flags: MOVE,
    ends_with_path: true,
    blocked: Some(blocked),
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    requires: &[Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let walk = Walk { p: *params::<Params>(p), slot: inst.slot, pick: None, armed: false };
    match w.get_mut::<VillageWalks>(e) {
        Some(mut v) => v.0.push(walk),
        None => {
            w.entity_mut(e).insert(VillageWalks(vec![walk]));
        }
    }
}
fn state(w: &World, e: Entity, inst: Inst) -> Option<Walk> {
    w.get::<VillageWalks>(e)?.0.iter().find(|x| x.slot == inst.slot).copied()
}
fn edit(w: &mut World, e: Entity, inst: Inst, f: impl FnOnce(&mut Walk)) {
    if let Some(mut v) = w.get_mut::<VillageWalks>(e)
        && let Some(x) = v.0.iter_mut().find(|x| x.slot == inst.slot)
    {
        f(x);
    }
}
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let Some(x) = state(w, e, inst) else { return };
    if x.armed {
        return;
    }
    let wait = with_rng(w, e, |r| idle_wait(r, (x.p.interval + 1) / 2));
    crate::goals::arm(w, e, inst.slot, wait);
    edit(w, e, inst, |x| x.armed = true);
}
fn rearm(w: &mut World, e: Entity, inst: Inst) {
    edit(w, e, inst, |x| x.armed = false);
    arm(w, e, inst);
}
fn blocked(w: &mut World, e: Entity, inst: Inst) {
    rearm(w, e, inst);
}
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    edit(w, e, inst, |x| x.armed = false);
    true
}
fn section_of(p: horizon_core::geometry::Vec3) -> [i32; 3] {
    [(p.x.floor() as i32) >> 4, (p.y.floor() as i32) >> 4, (p.z.floor() as i32) >> 4]
}
/// `SectionPos.center` as `Vec3.atBottomCenterOf` reads it.
fn section_center(s: [i32; 3]) -> [f64; 3] {
    [f64::from(s[0] * 16 + 8) + 0.5, f64::from(s[1] * 16 + 8), f64::from(s[2] * 16 + 8) + 0.5]
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(x) = state(w, e, inst) else { return false };
    let pos = w.get::<Pos>(e).unwrap().0;
    let here = section_of(pos);
    let found = match x.p.kind {
        Kind::Back => {
            let reg = w.resource::<PoiRegistry>();
            if reg.sections_to_village(here) <= 1 {
                None
            } else {
                let best = reg.closest_section(here, 2);
                (best != here).then(|| best)
            }
            .and_then(|best| pick_for(w, e, Pick::DefaultTowards { h: 10, v: 7, to: section_center(best), max_rad: std::f64::consts::FRAC_PI_2 }))
        }
        Kind::InVillage => {
            if with_rng(w, e, |r| r.next_float()) < 0.3 {
                pick_for(w, e, Pick::Land { h: 10, v: 7 })
            } else {
                towards_poi(w, e, here).or_else(|| pick_for(w, e, Pick::Land { h: 10, v: 7 }))
            }
        }
    };
    edit(w, e, inst, |x| x.pick = found);
    if found.is_none() {
        rearm(w, e, inst);
    }
    found.is_some()
}
/// `getPositionTowardsPoi`: a held point within 8 blocks of the centre of a random village section within two sections.
fn towards_poi(w: &mut World, e: Entity, here: [i32; 3]) -> Option<[f64; 3]> {
    let sections: Vec<[i32; 3]> = {
        let reg = w.resource::<PoiRegistry>();
        let mut v = Vec::new();
        for z in here[2] - 2..=here[2] + 2 {
            for y in here[1] - 2..=here[1] + 2 {
                for x in here[0] - 2..=here[0] + 2 {
                    if reg.sections_to_village([x, y, z]) == 0 {
                        v.push([x, y, z]);
                    }
                }
            }
        }
        v
    };
    if sections.is_empty() {
        return None;
    }
    let s = sections[with_rng(w, e, |r| r.next_int(sections.len() as i32)) as usize];
    let pois = w.resource::<PoiRegistry>().occupied_in_range([s[0] * 16 + 8, s[1] * 16 + 8, s[2] * 16 + 8], 8);
    if pois.is_empty() {
        return None;
    }
    let p = pois[with_rng(w, e, |r| r.next_int(pois.len() as i32)) as usize];
    pick_for(w, e, Pick::LandTowards { h: 10, v: 7, to: [f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5] })
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(x) = state(w, e, inst) else { return };
    if let Some(at) = x.pick {
        go_to_at(w, e, at, x.p.speed, 1);
    }
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    stop_nav(w, e);
    rearm(w, e, inst);
}
