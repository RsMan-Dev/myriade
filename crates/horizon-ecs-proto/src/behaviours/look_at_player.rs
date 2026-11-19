//! `LookAtPlayerGoal`: now and then (one chance in `period` per ask, one ask every 2 ticks) look at the nearest player
//! within range for 40 to 80 ticks. The ask is a deadline of the wheel.
use super::*;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub range: f64,
    pub period: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { range: 6.0, period: 50 };
}
/// `LookAtPlayerGoal`'s range and ask period; `armed`: its deadline is in the wheel; `pick`: the player `can_use` chose.
#[derive(Component, Clone, Copy, Debug)]
pub struct Glance {
    pub range: f64,
    pub period: i32,
    pub armed: bool,
    pub slot: u8,
    pick: u32,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Glancing {
    pub player: u32,
    pub until: u64,
}

pub static MODULE: Module = Module {
    name: "look_at_player",
    flags: LOOK,
    blocked: Some(blocked),
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Glance { range: p.range, period: p.period, armed: false, slot: inst.slot, pick: 0 });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((lazy_wake, glance_run).chain().in_set(slot));
}
/// Arms the ask of an animal that has none and that a player can see.
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let g = *w.get::<Glance>(e).unwrap();
    let p = w.get::<Pos>(e).unwrap().0;
    if !g.armed && w.resource::<LazyLook>().watched(p.x, p.z) {
        let wait = with_rng(w, e, |r| 2 * geometric(r, g.period));
        crate::goals::arm(w, e, inst.slot, wait);
        w.get_mut::<Glance>(e).unwrap().armed = true;
    }
}
/// The ask was refused: the next one is armed.
fn rearm(w: &mut World, e: Entity, inst: Inst) {
    w.get_mut::<Glance>(e).unwrap().armed = false;
    arm(w, e, inst);
}
fn blocked(w: &mut World, e: Entity, inst: Inst) {
    rearm(w, e, inst);
}
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(mut g) = w.get_mut::<Glance>(e) else { return false };
    g.armed = false;
    let g = *g;
    // With the lazy look, a timer that comes up for an animal no player sees lapses (nothing re-arms it until a
    // player's range reaches it again).
    let p = w.get::<Pos>(e).unwrap().0;
    if !w.resource::<LazyLook>().watched(p.x, p.z) {
        return false;
    }
    // Nobody within the range of the look: the ask fails; the next one is armed without the selector.
    let mut any = false;
    w.resource::<Online>().for_near(p.x, p.z, g.range, |q| any |= (q.x - p.x).powi(2) + (q.z - p.z).powi(2) <= g.range * g.range);
    if !any {
        let wait = with_rng(w, e, |r| 2 * geometric(r, g.period));
        crate::goals::arm(w, e, inst.slot, wait);
        w.get_mut::<Glance>(e).unwrap().armed = true;
        return false;
    }
    true
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let Some(g) = w.get::<Glance>(e).copied() else { return false };
    let mut best: Option<(u32, f64)> = None;
    w.resource::<Online>().for_near(pos.x, pos.z, g.range, |p| {
        let d = d2(pos, Vec3::new(p.x, p.y, p.z));
        if d <= g.range * g.range && best.is_none_or(|b| d < b.1) {
            best = Some((p.id, d));
        }
    });
    match best {
        Some(b) => {
            w.get_mut::<Glance>(e).unwrap().pick = b.0;
            true
        }
        None => {
            rearm(w, e, inst);
            false
        }
    }
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let now = w.resource::<Clock>().0;
    let id = w.get::<Glance>(e).unwrap().pick;
    // `reducedTickDelay(40 + nextInt(40))` asks of 2 ticks.
    let n = with_rng(w, e, |r| r.next_int(40));
    w.entity_mut(e).insert(Glancing { player: id, until: now + 2 * ((40 + n as u64 + 1) / 2) });
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Glancing>();
    rearm(w, e, inst);
}
/// An animal that walks into a watched bin (the lazy look) takes its look timers again.
fn lazy_wake(moves: Res<CellMoves>, lazy: Res<LazyLook>, q: Query<(&Pos, &Glance, Option<&Scan>)>, mut reqs: ResMut<Requests>) {
    if !lazy.enabled {
        return;
    }
    for &m in &moves.0 {
        if let Ok((p, g, s)) = q.get(m)
            && (!g.armed || s.is_some_and(|s| !s.armed))
            && lazy.watched(p.0.x, p.0.z)
        {
            reqs.0.push(Req::Arm(m));
        }
    }
}
/// `LookAtPlayerGoal.canContinue` and `tick`.
fn glance_run(
    clock: Res<Clock>,
    on: Res<Online>,
    mut q: Query<(Aim, &Glancing, &Glance)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, _, _), g, data) in &mut q {
        let p = on.get(g.player).filter(|p| d2(Vec3::new(p.x, p.y, p.z), pos.0) <= data.range * data.range);
        match p {
            Some(p) if clock.0 < g.until => aim(&mut cmd, e, &mut look, looking, [p.x, p.eye(), p.z], None),
            _ => reqs.0.push(Req::End(e, data.slot)),
        }
    }
}
