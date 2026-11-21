//! The side heads of a wither (`WitherBoss.customServerAiStep`, heads 1 and 2): one goal per head (`nth` 0 and 1), each one a wheel
//! deadline (`Due::Goal`), no tick. At its due, a head fires at its target while the target is seen within `range` (next due
//! `period + 0..jitter` ticks), else it picks a creature in the box `pick` that is not in `except`, through the spatial index, and
//! waits `10 + 0..10` ticks. Idle volleys (difficulty from `idle_difficulty`): each wakeup counts one idle tick, and past
//! `idle_after` the head fires a dangerous skull at a random point of the box `scatter`. An accepted hurt adds 3 to each head (`hurt`).
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::projectile::{launch, Flight};
use crate::targeting::{self, Who};
use horizon_core::geometry::Vec3;
use crate::rules::side_heads::{aim_point, in_box, in_range_sq, shot_velocity, side_point, spread};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub flight: Flight,
    pub idle_flight: Flight,
    pub period: u64,
    pub jitter: u64,
    pub range: f64,
    pub pick: [f64; 3],
    pub except: &'static [&'static str],
    pub idle_after: i32,
    pub idle_difficulty: u8,
    pub scatter: [f64; 3],
    pub side: f64,
    pub height: f64,
}
impl Params {
    /// The wither's heads: 40 ticks (+0..20), 30 blocks of sight, 20 by 8 by 20 to pick in, idle volleys past 16 idle ticks.
    pub const WITHER: Self = Self {
        flight: Flight::wither_skull(false),
        idle_flight: Flight::wither_skull(true),
        period: 40,
        jitter: 20,
        range: 30.0,
        pick: [20.0, 8.0, 20.0],
        except: crate::species::wither::UNDEAD,
        idle_after: 16,
        idle_difficulty: 2,
        scatter: [10.0, 5.0, 10.0],
        side: 1.3,
        height: 2.2,
    };
}
#[derive(Clone, Copy, Debug)]
pub struct Head {
    p: Params,
    slot: u8,
    mask: crate::species::SpeciesSet,
    due: u64,
    target: Option<Who>,
    idle: i32,
}
/// The side heads of an animal, by `nth`.
#[derive(Component, Clone, Debug)]
pub struct Heads(pub Vec<Head>);
pub static MODULE: Module = Module {
    name: "side_heads",
    flags: 0,
    equip,
    fired,
    can_use,
    start,
    requires: &[Cap::Body, Cap::Head, Cap::Targets],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    let mask = species_mask(&p);
    let due = now_of(w, e) + 1;
    let head = Head { p, slot: inst.slot, mask, due, target: None, idle: 0 };
    match w.get_mut::<Heads>(e) {
        Some(mut hs) => hs.0.push(head),
        None => {
            w.entity_mut(e).insert(Heads(vec![head]));
        }
    }
    arm_at(w, e, due, Due::Goal(inst.slot));
}
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    head(w, e, inst.nth).is_some_and(|h| now_of(w, e) >= h.due)
}
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let now = now_of(w, e);
    let Some(mut h) = head(w, e, inst.nth) else { return };
    if now < h.due {
        return;
    }
    // The heads do nothing during the charge (the Mob's `customServerAiStep` returns before them): they wait for its end.
    if let Some(end) = super::invulnerable_charge::ends_at(w, e) {
        h.due = end;
        w.get_mut::<Heads>(e).unwrap().0[inst.nth as usize] = h;
        arm_at(w, e, end, Due::Goal(inst.slot));
        w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
        return;
    }
    let idle_on = difficulty(w) >= h.p.idle_difficulty;
    h.due = now + 10 + roll(w, e, 10);
    if idle_on {
        h.idle += 1;
    }
    if idle_on && h.idle > h.p.idle_after {
        let o = pos(w, e);
        let s = h.p.scatter;
        let at = [o.x + between(w, e, s[0]), o.y + between(w, e, s[1]), o.z + between(w, e, s[2])];
        shoot(w, e, inst.nth, at, h.p.idle_flight, h.p);
        h.idle = 0;
    }
    let aim = h.target.and_then(|who| sight(w, e, who, h.p.range));
    match aim {
        Some(at) => {
            shoot(w, e, inst.nth, at, h.p.flight, h.p);
            h.due = now + h.p.period + roll(w, e, h.p.jitter as i32);
            h.idle = 0;
        }
        None => h.target = pick(w, e, &h),
    }
    if let Some(mut hs) = w.get_mut::<Heads>(e) {
        hs.0[inst.nth as usize] = h;
    }
    arm_at(w, e, h.due, Due::Goal(inst.slot));
    w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
}
/// An accepted hurt (`WitherBoss.accepts`): each head's idle counter goes up by 3.
pub fn hurt(w: &mut World, e: Entity) {
    if let Some(mut hs) = w.get_mut::<Heads>(e) {
        hs.0.iter_mut().for_each(|h| h.idle += 3);
    }
}
fn head(w: &World, e: Entity, nth: u8) -> Option<Head> {
    w.get::<Heads>(e)?.0.get(nth as usize).copied()
}
/// Every creature kind but the excepted ones (the undead, for the wither).
fn species_mask(p: &Params) -> crate::species::SpeciesSet {
    let ids = crate::species::registry().species.iter().enumerate().filter(|(_, sp)| !p.except.contains(&sp.name)).map(|(i, _)| i);
    crate::species::SpeciesSet::of(ids)
}
/// A random creature of the box, from the spatial index (asked at the head's wakeup only).
fn pick(w: &mut World, e: Entity, h: &Head) -> Option<Who> {
    let o = pos(w, e);
    let b = h.p.pick;
    let mut found: Vec<Entity> = Vec::new();
    w.resource::<Index>().for_radius(o.x, o.z, b[0].max(b[2]), |c| {
        if c == e {
            return;
        }
        let (Some(cp), Some(cs)) = (w.get::<Pos>(c), w.get::<SpeciesId>(c)) else { return };
        if h.mask.contains(cs.0) && in_box(cp.0.x - o.x, cp.0.y - o.y, cp.0.z - o.z, b) {
            found.push(c);
        }
    });
    let mut who: Vec<Who> = found.into_iter().map(Who::Entity).collect();
    // The player too (not creative, not spectator), in the box and in sight, after the creatures.
    let mut players = Vec::new();
    w.resource::<crate::goals::Online>().for_near(o.x, o.z, b[0].max(b[2]), |p| players.push(p));
    for p in players {
        if in_box(p.x - o.x, p.y - o.y, p.z - o.z, b) && targeting::can_target(w, e, Who::Player(p.id)) && targeting::sees(w, e, Who::Player(p.id)) {
            who.push(Who::Player(p.id));
        }
    }
    if who.is_empty() {
        return None;
    }
    let k = roll(w, e, who.len() as i32) as usize;
    Some(who[k])
}
/// The aim point of a target still in sight within `range`: halfway up its body.
fn sight(w: &World, e: Entity, who: Who, range: f64) -> Option<[f64; 3]> {
    let i = targeting::info(w, who)?;
    let o = pos(w, e);
    let d = (i.pos.x - o.x).powi(2) + (i.pos.y - o.y).powi(2) + (i.pos.z - o.z).powi(2);
    (in_range_sq(d, range) && targeting::sees(w, e, who)).then(|| aim_point(i.pos, i.eye))
}
fn side_pos(w: &World, e: Entity, nth: u8, p: &Params) -> [f64; 3] {
    let o = pos(w, e);
    let yaw = w.get::<Yaw>(e).map_or(0.0, |y| y.0);
    side_point(o, yaw, nth, p.side, p.height)
}
fn shoot(w: &mut World, e: Entity, nth: u8, at: [f64; 3], flight: Flight, p: Params) {
    let h = side_pos(w, e, nth, &p);
    let pitch = 1.0 + (between(w, e, 0.5) - between(w, e, 0.5)) * 0.2;
    let o = pos(w, e);
    w.resource_mut::<Events>().host.sound("entity.wither.shoot", [o.x, o.y, o.z], 2.0, pitch as f32);
    let v = shot_velocity(h, at);
    launch(w, flight, Vec3::new(h[0], h[1], h[2]), v, Some(Who::Entity(e)));
}
fn pos(w: &World, e: Entity) -> Vec3 {
    w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0)
}
fn difficulty(w: &World) -> u8 {
    w.get_resource::<crate::effects::Difficulty>().map_or(2, |d| d.0)
}
fn roll(w: &mut World, e: Entity, n: i32) -> u64 {
    if n <= 0 {
        return 0;
    }
    with_rng(w, e, |r| r.next_int(n)) as u64
}
fn between(w: &mut World, e: Entity, half: f64) -> f64 {
    spread(with_rng(w, e, |r| r.next_f64()), half)
}
