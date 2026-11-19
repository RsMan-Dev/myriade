//! `FollowPlayerRiddenEntityGoal`: when the vehicle a player steers (a boat or a nautilus: `vehicle::Vehicle`) moves within 5 blocks of the creature, it swims
//! behind the player (the block behind in the rider's direction, one lower) at speed 1,0; within 4 blocks it goes AHEAD (10 blocks in the rider's
//! direction), and back behind when it is over 12 blocks off. It also pushes itself, 0,015 behind and 0,01 ahead, along its own input. It leaves when
//! the vehicle stops or the player steps off. What wakes it: `Events.vehicle_moved` (the host's write of the vehicle), nothing else; while it runs, a
//! system visits the creature each tick (`tick`, `canContinueToUse`). Only the nearest rider of the right kind is considered.
//! Écart: the `Mob` moves the creature twice in the tick of the goal (`move_relative` then `do_move`); here the push is added to the velocity and the
//! move is the frame's one.
use super::*;
use crate::vehicle::Vehicles;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// A nautilus (else a boat).
    pub nautilus: bool,
    pub speed: f64,
    /// The box of the creature is grown by this much to meet the vehicle's.
    pub near: f64,
    pub push_follow: f32,
    pub push_lead: f32,
    /// `reduced(10)`.
    pub recalc: i32,
    pub lead_distance: i32,
    pub close_sqr: f64,
    pub far_sqr: f64,
}
impl Params {
    pub const BOAT: Self = Self { nautilus: false, speed: 1.0, near: 5.0, push_follow: 0.015, push_lead: 0.01, recalc: 5, lead_distance: 10, close_sqr: 16.0, far_sqr: 144.0 };
    pub const NAUTILUS: Self = Self { nautilus: true, ..Self::BOAT };
}
#[derive(Clone, Copy, Debug)]
pub struct Escort {
    pub p: Params,
    slot: u8,
    pub following: bool,
    pub leading: bool,
    recalc: i32,
}
/// The instances of the creature (a boat's, a nautilus's), by rank in the selector.
#[derive(Component, Clone, Debug)]
pub struct Escorts(pub Vec<Escort>);
/// The goal of this rank runs.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Escorting(pub u8);

pub static MODULE: Module = Module { name: "follow_player_vehicle", flags: 0, equip, can_use, start, stop, tick: Some(tick), requires: &[Cap::Body, Cap::Goals, Cap::Locomotion], ..Module::DEFAULT };
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let s = Escort { p: *params::<Params>(p), slot: inst.slot, following: false, leading: false, recalc: 0 };
    match w.get_mut::<Escorts>(e) {
        Some(mut v) => v.0.push(s),
        None => {
            w.entity_mut(e).insert(Escorts(vec![s]));
        }
    }
}
fn tick(w: &mut World) {
    tick_state(w, escort_events);
}
/// The vehicle of a player moved (or stopped): the creatures around it look again; each escort runs its step (one registered system).
#[allow(clippy::type_complexity)]
fn escort_events(ev: Res<Events>, idx: Res<Index>, vehicles: Option<Res<Vehicles>>, q: Query<&Escorts>, running: Query<Entity, With<Escorting>>, mut reqs: ResMut<Requests>) {
    if let Some(vehicles) = vehicles {
        for id in ev.vehicle_moved.iter() {
            let Some(v) = vehicles.get(id) else { continue };
            let c = (v.bounds.min_position(), v.bounds.max_position());
            idx.for_radius((c.0.x + c.1.x) / 2.0, (c.0.z + c.1.z) / 2.0, 9.0, |o| {
                if let Ok(es) = q.get(o) {
                    for s in &es.0 {
                        if s.p.nautilus == v.nautilus {
                            reqs.0.push(Req::Start(o, s.slot));
                        }
                    }
                }
            });
        }
    }
    for e in &running {
        reqs.0.push(Req::Call(e, escort_step));
    }
}
fn state(w: &World, e: Entity, slot: u8) -> Option<Escort> {
    w.get::<Escorts>(e)?.0.iter().find(|s| s.slot == slot).copied()
}
fn with_state(w: &mut World, e: Entity, slot: u8, f: impl FnOnce(&mut Escort)) {
    if let Some(mut v) = w.get_mut::<Escorts>(e)
        && let Some(s) = v.0.iter_mut().find(|s| s.slot == slot)
    {
        f(s);
    }
}
/// The nearest player whose vehicle is of the kind.
fn rider(w: &World, e: Entity, nautilus: bool) -> Option<(crate::goals::Player, crate::vehicle::Vehicle)> {
    let pos = w.get::<Pos>(e)?.0;
    let (on, all) = (w.resource::<Online>(), w.get_resource::<Vehicles>()?);
    let mut best: Option<(f64, crate::goals::Player, crate::vehicle::Vehicle)> = None;
    for (id, v) in all.riders() {
        let Some(p) = on.get(id) else { continue };
        let d = d2(pos, Vec3::new(p.x, p.y, p.z));
        if v.nautilus == nautilus && best.is_none_or(|b| d < b.0) {
            best = Some((d, p, v));
        }
    }
    best.map(|b| (b.1, b.2))
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(s) = state(w, e, inst.slot) else { return false };
    let Some((_, v)) = rider(w, e, s.p.nautilus) else { return false };
    if s.following && v.moved() {
        return true;
    }
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return false };
    v.bounds.intersects(b.inflate(Vec3::new(s.p.near, s.p.near, s.p.near))) && v.moved()
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let Some(s) = state(w, e, inst.slot) else { return };
    let has = rider(w, e, s.p.nautilus).is_some();
    with_state(w, e, inst.slot, |s| (s.following, s.recalc, s.leading) = (has, 0, false));
    w.entity_mut(e).insert(Escorting(inst.slot));
    escort_step(w, e);
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    with_state(w, e, inst.slot, |s| s.following = false);
    w.entity_mut(e).remove::<Escorting>();
}
/// `Entity.getMotionDirection` / `Direction.fromYRot`: the horizontal step of the yaw (south, west, north, east).
fn motion_step(yaw: f32) -> [i32; 2] {
    match (yaw / 90.0 + 0.5).floor() as i32 & 3 {
        0 => [0, 1],
        1 => [-1, 0],
        2 => [0, -1],
        _ => [1, 0],
    }
}
/// `canContinueToUse`, then `tick`.
fn escort_step(w: &mut World, e: Entity) {
    let Some(Escorting(slot)) = w.get::<Escorting>(e).copied() else { return };
    let Some(s) = state(w, e, slot) else { return };
    let Some((p, v)) = rider(w, e, s.p.nautilus) else { return w.resource_mut::<Requests>().0.push(Req::End(e, slot)) };
    if !(s.following && v.passenger && v.moved()) {
        return w.resource_mut::<Requests>().0.push(Req::End(e, slot));
    }
    let speed = if s.leading { s.p.push_lead } else { s.p.push_follow };
    if let (Some(d), Some(yaw)) = (w.get::<Drive>(e).copied(), w.get::<Yaw>(e).map(|y| y.0)) {
        let mut push = Vel(Vec3::ZERO);
        move_relative(&mut push, yaw, speed, d.xxa, d.zza);
        push.0.y += f64::from(d.yya * speed);
        impulse(w, e, push.0);
    }
    let mut left = s.recalc - 1;
    if left > 0 {
        return with_state(w, e, slot, |s| s.recalc = left);
    }
    left = s.p.recalc;
    let [dx, dz] = motion_step(v.yaw);
    let block = [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32];
    let Some(pos) = w.get::<Pos>(e).map(|q| q.0) else { return };
    let dist = d2(pos, Vec3::new(p.x, p.y, p.z));
    let mut leading = s.leading;
    if !s.leading {
        let at = [f64::from(block[0] - dx), f64::from(block[1] - 1), f64::from(block[2] - dz)];
        go_to_at(w, e, at, s.p.speed, 1);
        if dist < s.p.close_sqr {
            (left, leading) = (0, true);
        }
    } else {
        let at = [f64::from(block[0] + dx * s.p.lead_distance), f64::from(block[1] - 1), f64::from(block[2] + dz * s.p.lead_distance)];
        go_to_at(w, e, at, s.p.speed, 1);
        if dist > s.p.far_sqr {
            (left, leading) = (0, false);
        }
    }
    with_state(w, e, slot, |s| (s.recalc, s.leading) = (left, leading));
}
