//! Seek a shelter (`Fox.SeekShelterGoal`): in a storm under the sky, or in daylight in a village section, walk to a spot the sky does
//! not see (`hide_pos`: ten tries). The storm is an event: `storm_wake` runs when `Storm` changes, and marks the animals under the
//! sky with `Stormy`; the weather value is not read by `can_use`. Outside a storm the village check runs every `interval` asks.
use super::fox_state as pose;
use super::*;
use crate::cow::Pos;
use crate::goals::MOVE;
use crate::goals::{go_to_at, with_rng, Req, Requests};
use crate::cow::Level;
use crate::hostile::Daylight;
use crate::poi::PoiRegistry;
use crate::spots::{WalkValue, walk_target_value};
use crate::targeting::Target;
use crate::rules::shelter::{hide_offset, next_interval};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed of the walk.
    pub speed: f64,
    /// Ticks between two village checks (`reduced` at the start, as the Mob's `reducedTickDelay`).
    pub interval: i32,
    /// Pose bits that stop the goal (asleep).
    pub blocked: u8,
}

/// The weather: `true` during a storm. Inserted at the world's creation; changes wake the goal (no per-tick read).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct Storm(pub bool);

/// An animal under the sky when a storm starts: the goal is asked for it at every ask until the storm ends.
#[derive(Component, Clone, Copy, Debug)]
pub struct Stormy;

#[derive(Component, Clone, Copy, Debug)]
pub struct SeekShelter {
    pub p: &'static Params,
    pub slot: u8,
    /// `interval`: the countdown of the village checks, `reduced` at creation.
    pub interval: i32,
    /// `wanted`: the spot the goal walks to, set by `hide_pos`.
    pub wanted: [f64; 3],
}

pub static MODULE: Module = Module {
    name: "seek_shelter",
    flags: MOVE,
    equip,
    tick: Some(tick),
    can_use,
    start,
    ends_with_path: true,
    requires: &[Cap::Locomotion],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(SeekShelter { p, slot: inst.slot, interval: (p.interval + 1) / 2, wanted: [0.0; 3] });
}

/// The ticks of the module: the storm's change is looked at through `is_changed` (no weather value is read per tick).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, storm_wake);
}

/// The storm began or ended: the animals under the sky are marked and asked (`Stormy`), those no longer under it are unmarked.
fn storm_wake(storm: Res<Storm>, mut cmd: Commands, q: Query<(Entity, &SeekShelter, Option<&Stormy>)>, mut reqs: ResMut<Requests>) {
    if !storm.is_changed() {
        return;
    }
    for (e, g, marked) in &q {
        if storm.0 {
            cmd.entity(e).insert(Stormy);
            reqs.0.push(Req::Start(e, g.slot));
        } else if marked.is_some() {
            cmd.entity(e).remove::<Stormy>();
        }
    }
}

/// `hide_pos` (`goals.rs:2740`): ten random spots within 10 blocks horizontally and 3 below or above. A spot the sky does not see
/// and whose walk value is below zero is taken.
fn hide_pos(w: &mut World, e: Entity) -> Option<[f64; 3]> {
    let p = w.get::<Pos>(e)?.0;
    let b = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
    let mw = w.resource::<Level>().mw.0;
    let wv = w.get::<WalkValue>(e).copied().unwrap_or(WalkValue::Zero);
    for _ in 0..10 {
        let d = with_rng(w, e, |r| hide_offset(&mut |n| r.next_int(n)));
        let q = [b[0] + d[0], b[1] + d[1], b[2] + d[2]];
        if !mw.can_see_sky(q[0], q[1], q[2]) && walk_target_value(mw, wv, q) < 0.0 {
            return Some([f64::from(q[0]) + 0.5, f64::from(q[1]), f64::from(q[2]) + 0.5]);
        }
    }
    None
}

/// The section of the animal is a village section (`isVillage` as the Mob compares it: `sections_to_village(...) > 1`).
/// DOUBT: the Mob's `village_sections` is not ported; the section is given to `PoiRegistry` directly.
fn in_village(w: &World, at: [i32; 3]) -> bool {
    let sec = [at[0] >> 4, at[1] >> 4, at[2] >> 4];
    w.get_resource::<PoiRegistry>().is_some_and(|r| r.sections_to_village(sec) > 1)
}

fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(g) = w.get::<SeekShelter>(e).copied() else { return false };
    if g.slot != inst.slot || pose::has(w, e, g.p.blocked) || w.get::<Target>(e).is_some() {
        return false;
    }
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let at = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
    let sees_sky = w.resource::<Level>().mw.0.can_see_sky(at[0], at[1], at[2]);
    let ask = if w.get::<Stormy>(e).is_some() && sees_sky {
        true
    } else {
        let left = g.interval - 1;
        if let Some(mut s) = w.get_mut::<SeekShelter>(e) {
            s.interval = next_interval(left, g.p.interval);
        }
        left <= 0 && w.get_resource::<Daylight>().is_some_and(|d| d.0) && sees_sky && in_village(w, at)
    };
    ask && match hide_pos(w, e) {
        Some(wanted) => {
            if let Some(mut s) = w.get_mut::<SeekShelter>(e) {
                s.wanted = wanted;
            }
            true
        }
        None => false,
    }
}

fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(g) = w.get::<SeekShelter>(e).copied() else { return };
    if g.slot == inst.slot {
        go_to_at(w, e, g.wanted, g.p.speed, 0);
    }
}
