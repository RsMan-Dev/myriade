//! `LlamaFollowCaravanGoal`: a free creature of the caravan kind (it carries this module) within 9 x 4 x 9 blocks of a creature that LEADS a
//! line (held on a lead, or whose line goes back to one in at most 8 links) takes the place behind the last of the line when it is 2 blocks or
//! more from it, at 2,1 of its speed, and walks to the point 2 blocks behind its predecessor each tick. It leaves when the head dies, when
//! the chain no longer ends at a held one, when another took its place behind the same head, or when it is more than 26 blocks away for long
//! enough (its speed rises by 1,2 up to 3,0 first). What wakes it: a lead put on a creature near it (`Events.leashed`), a move of its own column
//! while some lead is on (the line may be near now), the end of another follower's goal (a place is free). The state of a line is two
//! components: `Lined { head }` on the follower and `Tail(follower)` on the head (so "has a tail" is one read, not a scan of the herd).
use super::*;
use crate::leash::{is_leashed, holder_of, Holder, Leashes};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// The zone around the creature where it looks for a line (half sizes).
    pub area: [f64; 3],
    /// A head nearer than this (squared) is not joined (the creature is already beside it).
    pub min_distance_sqr: f64,
    /// `CARAVAN_LIMIT`: the links a chain may have to reach a held creature.
    pub limit: i32,
    /// More than this (squared) from the head, the follower speeds up, then gives up.
    pub far_sqr: f64,
    pub max_speed: f64,
    pub speed_step: f64,
    /// `reduced(40)` selector ticks of patience once far.
    pub far_wait: i32,
    /// The point it walks to is this far behind its head.
    pub trail: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 2.1_f32 as f64, area: [9.0, 4.0, 9.0], min_distance_sqr: 4.0, limit: 8, far_sqr: 676.0, max_speed: 3.0, speed_step: 1.2, far_wait: 20, trail: 2.0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Caravan {
    pub p: Params,
    slot: u8,
    pick: Option<Entity>,
}
/// The creature walks in a line behind `head`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Lined {
    pub head: Entity,
    pub speed: f64,
    dist_check: i32,
    goal: [i32; 3],
}
/// The creature that follows this one.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Tail(pub Entity);

pub static MODULE: Module = Module {
    name: "follow_caravan",
    flags: MOVE,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Caravan { p: *params::<Params>(p), slot: inst.slot, pick: None });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(caravan_run.in_set(slot));
}
/// One system for the three things that wake or run the module (one registered system, not three): a lead put on a creature of the kind wakes the
/// free ones around it; a free creature that changes column while some lead is on may have come near a line (the lines are few: no lead, no
/// wake); each follower runs its step.
#[allow(clippy::type_complexity)]
fn caravan_run(
    ev: Res<Events>,
    idx: Res<Index>,
    cells: Res<CellMoves>,
    leashes: Option<Res<Leashes>>,
    pos: Query<&Pos>,
    own: Query<&Caravan>,
    free: Query<&Caravan, Without<Lined>>,
    loose: Query<&Caravan, (Without<Lined>, Without<crate::leash::Leash>)>,
    lined: Query<Entity, With<Lined>>,
    mut reqs: ResMut<Requests>,
) {
    for e in ev.leashed.iter() {
        let (Ok(p), Ok(c)) = (pos.get(e), own.get(e)) else { continue };
        idx.for_radius(p.0.x, p.0.z, c.p.area[0] + 1.0, |o| {
            if o != e && let Ok(f) = free.get(o) {
                reqs.0.push(Req::Start(o, f.slot));
            }
        });
    }
    if leashes.is_some_and(|l| !l.is_empty()) {
        for &e in &cells.0 {
            if let Ok(f) = loose.get(e) {
                reqs.0.push(Req::Start(e, f.slot));
            }
        }
    }
    for e in &lined {
        reqs.0.push(Req::Call(e, follow_step));
    }
}
fn alive(w: &World, e: Entity) -> bool {
    w.get::<Health>(e).is_some_and(|h| h.0 > 0.0)
}
/// `first_is_leashed`: the chain of heads from `head` reaches a held creature in at most `limit` links.
fn chain_leashed(w: &World, head: Option<Entity>, counter: i32, limit: i32) -> bool {
    let Some(h) = head else { return false };
    if counter > limit {
        return false;
    }
    is_leashed(w, h) || chain_leashed(w, w.get::<Lined>(h).map(|l| l.head), counter + 1, limit)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(c) = w.get::<Caravan>(e).copied() else { return false };
    if is_leashed(w, e) || w.get::<Lined>(e).is_some() {
        return false;
    }
    let (Some(pos), Some(b)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Bounds>(e).map(|b| b.0)) else { return false };
    let zone = b.inflate(horizon_core::geometry::Vec3::new(c.p.area[0], c.p.area[1], c.p.area[2]));
    let mut near = Vec::new();
    w.resource::<Index>().for_radius(pos.x, pos.z, c.p.area[0] + 2.0, |o| near.push(o));
    near.sort_by_key(|o| w.get::<Uid>(*o).map_or(0, |u| u.0));
    // The nearest line end (a follower with no one behind it), else the nearest held creature with no one behind it.
    let (mut lined, mut held): (Option<(Entity, f64)>, Option<(Entity, f64)>) = (None, None);
    for o in near {
        if o == e || w.get::<Caravan>(o).is_none() || !alive(w, o) || w.get::<Tail>(o).is_some() {
            continue;
        }
        let Some(ob) = w.get::<Bounds>(o).map(|b| b.0) else { continue };
        if !ob.intersects(zone) {
            continue;
        }
        let d = d2(pos, w.get::<Pos>(o).map_or(pos, |p| p.0));
        if w.get::<Lined>(o).is_some() {
            if lined.is_none_or(|l| d <= l.1) {
                lined = Some((o, d));
            }
        } else if is_leashed(w, o) && held.is_none_or(|l| d <= l.1) {
            held = Some((o, d));
        }
    }
    let Some((head, best)) = lined.or(held) else { return false };
    if best < c.p.min_distance_sqr {
        // Beside it already: look again a little later (the selector asks each tick).
        arm(w, e, c.slot, 10);
        return false;
    }
    if !is_leashed(w, head) && !chain_leashed(w, w.get::<Lined>(head).map(|l| l.head), 1, c.p.limit) {
        return false;
    }
    w.get_mut::<Caravan>(e).unwrap().pick = Some(head);
    true
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let Some(mut c) = w.get_mut::<Caravan>(e) else { return };
    let Some(head) = c.pick.take() else { return };
    let speed = c.p.speed;
    w.entity_mut(e).insert(Lined { head, speed, dist_check: 0, goal: [i32::MIN; 3] });
    w.entity_mut(head).insert(Tail(e));
    // `tick` runs the tick the goal starts.
    follow_step(w, e);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    let Some(l) = w.entity_mut(e).take::<Lined>() else { return };
    if w.get::<Tail>(l.head).is_some_and(|t| t.0 == e) {
        w.entity_mut(l.head).remove::<Tail>();
    }
    stop_nav(w, e);
    // A place behind the old head is free: the creatures around look again.
    let Some(c) = w.get::<Caravan>(e).copied() else { return };
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let mut near = Vec::new();
    w.resource::<Index>().for_radius(pos.x, pos.z, c.p.area[0] + 1.0, |o| near.push(o));
    for o in near {
        if o != e && let Some(f) = w.get::<Caravan>(o).filter(|_| w.get::<Lined>(o).is_none()) {
            let slot = f.slot;
            w.resource_mut::<Requests>().0.push(Req::Start(o, slot));
        }
    }
}
/// `canContinueToUse` (the selector's tick: one in two), then `tick`.
fn follow_step(w: &mut World, e: Entity) {
    let (Some(c), Some(l)) = (w.get::<Caravan>(e).copied(), w.get::<Lined>(e).copied()) else { return };
    let end = |w: &mut World| w.resource_mut::<Requests>().0.push(Req::End(e, c.slot));
    if !alive(w, l.head) || !chain_leashed(w, Some(l.head), 0, c.p.limit) || w.get::<Tail>(l.head).is_none_or(|t| t.0 != e) {
        return end(w);
    }
    let (Some(p), Some(hp)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Pos>(l.head).map(|p| p.0)) else { return };
    if (now_of(w, e) + phase_of(w, e)).is_multiple_of(2) {
        let (mut speed, mut check, mut raised) = (l.speed, l.dist_check, false);
        if d2(p, hp) > c.p.far_sqr {
            if speed <= c.p.max_speed {
                speed *= c.p.speed_step;
                check = c.p.far_wait;
                raised = true;
            } else if check == 0 {
                return end(w);
            }
        }
        if !raised && check > 0 {
            check -= 1;
        }
        let mut lined = w.get_mut::<Lined>(e).unwrap();
        (lined.speed, lined.dist_check) = (speed, check);
    }
    // A creature held at a fence knot does not walk to the one ahead.
    if matches!(holder_of(w, e), Some(Holder::Knot(_))) {
        return;
    }
    let l = *w.get::<Lined>(e).unwrap();
    let to = [hp.x - p.x, hp.y - p.y, hp.z - p.z];
    let dist = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
    let k = if dist > 0.0 { (dist - c.p.trail).max(0.0) / dist } else { 0.0 };
    let target = [p.x + to[0] * k, p.y + to[1] * k, p.z + to[2] * k];
    let block = [target[0].floor() as i32, target[1].floor() as i32, target[2].floor() as i32];
    let idle = w.get::<Following>(e).is_none() && w.get::<NeedsPath>(e).is_none();
    if l.goal != block || idle {
        w.get_mut::<Lined>(e).unwrap().goal = block;
        go_to_at(w, e, target, l.speed, 1);
    }
}
