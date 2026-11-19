//! `TemptGoal`: a player within range holds a tempting item; the animal looks at the player and walks to it, stopping
//! 2.5 blocks away. An animal can have several (the pig: the carrot on a stick, then its foods), one entry each.
use super::*;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// The items that tempt, by name (resolved to ids once, at creation).
    pub items: &'static [&'static str],
    /// The distance at which the animal stops, and the range within which a player tempts.
    pub stop: f64,
    pub range: f64,
    /// The head's turn speed toward the player.
    pub look_y: f32,
    /// `calmDown`: ticks without temptation after the goal ended.
    pub calm: u64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, items: &[], stop: 2.5, range: 10.0, look_y: 95.0, calm: 100 };
}
/// One `TemptGoal`: its parameters and its state.
#[derive(Clone, Copy, Debug)]
pub struct Tempt {
    /// The items that tempt, a bit per item id.
    pub items: crate::items::ItemSet,
    pub speed: f64,
    pub stop: f64,
    pub range: f64,
    pub look_y: f32,
    pub calm: u64,
    pub calm_until: u64,
    /// The tick since which the calm-down stands still because the goal cannot be asked (0: it counts).
    pub paused_at: u64,
    pub player: u32,
    pub aim: (i32, i32),
    /// The rank of the entry in the selector.
    pub slot: u8,
}
/// The tempt goals of the animal, by `Inst::nth` (one instance each: its own items, speed, calm-down and player).
#[derive(Component, Clone, Debug)]
pub struct Tempts(pub Vec<Tempt>);
/// A tempt goal runs: which instance (`nth`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Tempting(pub u8);

pub static MODULE: Module = Module {
    name: "tempt",
    flags: MOVE | LOOK,
    after_change: Some(after_change),
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Locomotion, Cap::Head],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    let items = crate::items::ItemSet::of(p.items);
    let t = Tempt { items, speed: p.speed, stop: p.stop, range: p.range, look_y: p.look_y, calm: p.calm, calm_until: 0, paused_at: 0, player: 0, aim: (i32::MIN, 0), slot: inst.slot };
    match w.get_mut::<Tempts>(e) {
        Some(mut ts) => {
            debug_assert_eq!(ts.0.len(), inst.nth as usize);
            ts.0.push(t);
        }
        None => {
            w.entity_mut(e).insert(Tempts(vec![t]));
        }
    }
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((wake_on_player, tempt_run).chain().in_set(slot));
}
/// A player took an item, put it away, came or went: the animals it newly draws are woken (those that were already in
/// range, and those whose goal runs, notice by themselves).
fn wake_on_player(clock: Res<Clock>, on: Res<Online>, idx: Res<Index>, cows: Query<(&Pos, &Tempts)>, mut reqs: ResMut<Requests>) {
    let d2 = |p: &Player, c: Vec3| (p.x - c.x).powi(2) + (p.y - c.y).powi(2) + (p.z - c.z).powi(2);
    for &(old, new) in &on.seen {
        let Some(n) = new else { continue };
        idx.for_radius(n.x, n.z, SCAN, |c| {
            let Ok((pos, ts)) = cows.get(c) else { return };
            for t in &ts.0 {
                let reach = t.range.max(2.0).powi(2);
                let was = old.is_some_and(|o| o.holds_any(t.items) && d2(&o, pos.0) <= reach);
                if clock.of(c) >= t.calm_until && n.holds_any(t.items) && d2(&n, pos.0) <= reach && !was {
                    reqs.0.push(Req::Start(c, t.slot));
                }
            }
        });
    }
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let now = crate::cow::now_of(w, e);
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let Some(t) = w.get::<Tempts>(e).and_then(|ts| ts.0.get(inst.nth as usize).copied()) else { return false };
    if now < t.calm_until {
        return false;
    }
    let mut best: Option<(u32, f64)> = None;
    let reach = t.range.max(2.0);
    w.resource::<Online>().for_near(pos.x, pos.z, reach, |p| {
        let d = d2(pos, Vec3::new(p.x, p.y, p.z));
        if p.holds_any(t.items) && d <= reach * reach && best.is_none_or(|b| d < b.1) {
            best = Some((p.id, d));
        }
    });
    let Some((id, _)) = best else { return false };
    w.get_mut::<Tempts>(e).unwrap().0[inst.nth as usize].player = id;
    true
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    w.get_mut::<Tempts>(e).unwrap().0[inst.nth as usize].aim = (i32::MIN, 0);
    w.entity_mut(e).insert(Tempting(inst.nth));
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    let now = crate::cow::now_of(w, e);
    w.entity_mut(e).remove::<Tempting>();
    stop_nav(w, e);
    // `calmDown = reducedTickDelay(100)`: no new temptation by this goal for 100 ticks, then it is looked at
    // again; the other tempt goal of the animal has its own.
    let mut ts = w.get_mut::<Tempts>(e).unwrap();
    let calm = ts.0[inst.nth as usize].calm;
    ts.0[inst.nth as usize].calm_until = now + calm;
    arm(w, e, inst.slot, calm as i32);
}
/// `TemptGoal.canUse` counts its `calmDown` down each time the selector asks, and the selector asks only a goal that
/// could take its flags: while another goal of an equal or higher priority holds them, the calm-down stands still.
fn after_change(w: &mut World, e: Entity, c: Change) {
    let now = crate::cow::now_of(w, e);
    let Some(n) = w.get::<Tempts>(e).map(|t| t.0.len()) else { return };
    for i in 0..n {
        let (slot, calm_until, paused_at) = {
            let t = &w.get::<Tempts>(e).unwrap().0[i];
            (t.slot, t.calm_until, t.paused_at)
        };
        let g = w.get::<GoalSet>(e).unwrap();
        match c {
            Change::Started => {
                let blocked = !g.slot_running(slot) && !g.replaceable(slot);
                if blocked && calm_until > now && paused_at == 0 {
                    w.get_mut::<Tempts>(e).unwrap().0[i].paused_at = now;
                }
            }
            Change::Ended => {
                let free = g.replaceable(slot);
                if free && paused_at != 0 {
                    let t = &mut w.get_mut::<Tempts>(e).unwrap().0[i];
                    t.calm_until += now - t.paused_at;
                    t.paused_at = 0;
                    let left = t.calm_until.saturating_sub(now);
                    arm(w, e, slot, left as i32);
                }
            }
        }
    }
}
/// `TemptGoal.canContinue` and `tick`: look at the player, stop near it, walk to it when its block changed.
fn tempt_run(
    clock: Res<Clock>,
    on: Res<Online>,
    mut q: Query<(Aim, &mut Tempts, &Tempting, Option<&Phase>)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, following, asked), mut ts, inst, phase) in &mut q {
        // `TemptGoal` continues and ticks on the selector's ticks only (tick + id even): one look and at most one path
        // search every other tick, as the `Mob`.
        let phase = phase.map_or(0, |p| p.0);
        let now = clock.of(e);
        if (now + phase) % 2 == 1 && now > 1 {
            continue;
        }
        let Some(t) = ts.0.get_mut(inst.0 as usize) else { continue };
        let max_d = t.range.max(2.0).powi(2);
        let p = on.get(t.player).filter(|p| p.holds_any(t.items) && d2(Vec3::new(p.x, p.y, p.z), pos.0) <= max_d);
        let Some(p) = p else {
            reqs.0.push(Req::End(e, t.slot));
            continue;
        };
        aim(&mut cmd, e, &mut look, looking, [p.x, p.eye(), p.z], Some(t.look_y));
        if d2(Vec3::new(p.x, p.y, p.z), pos.0) < t.stop * t.stop {
            if following || asked || t.aim.0 != i32::MIN {
                t.aim = (i32::MIN, 0);
                cmd.queue(move |w: &mut World| stop_nav(w, e));
            }
        } else {
            let a = (p.x.floor() as i32, p.z.floor() as i32);
            if a != t.aim || (!following && !asked) {
                t.aim = a;
                go_to(&mut cmd, e, p.x, p.z, t.speed, 2);
            }
        }
    }
}
