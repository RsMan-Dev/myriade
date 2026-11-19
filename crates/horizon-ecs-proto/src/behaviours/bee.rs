//! The bee (`Bee`, 1534 lines of `Mob`): its state, the hive service (a bee in a hive is out of the world for its time inside), and the goals of
//! its cycle: pollinate a flower, go home with the nectar, enter the hive, come out, locate another, grow crops, validate what it remembers.
//! No scan and no system: the asks of the `Mob` (every other tick) are deadlines (a goal that cannot run re-arms itself at the next time its
//! condition can change), the goals that tick are visited by `tick_all` (called by `arbitrate`) through markers, the hive is a resource whose
//! next release time is the only thing looked at each tick.
//! What it asks of the frame (and has here): `Hives` (the occupants of each hive: entity, tick it went in, minimum stay), `InHive` (the bee is
//! out of every query: `zone::sleep`; the zone does not wake it), the release (position in front of the hive, nectar delivered, honey).
//! Not ported: the sting and the anger (`BeeAttackGoal`, `BeeBecomeAngryTargetGoal`, `HurtByTarget` of the bee: the melee and the targets of the
//! frame serve), the bee's growth and love while in the hive, the honey level of the block (no block entity: the hive is the resource), the
//! random hop of `pathfindRandomlyTowards` (the bee goes straight to a hive or a flower by its flying path), `Wander`'s direction towards a far
//! hive (`fly_stroll`), the sounds, the particles of the nectar.
use super::*;
use crate::{cow::*, goals::*};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Self = Self {};
}

/// `Bee`'s fields: nectar, sting, the remembered hive and flower, the cooldowns (own time), the flowers it gave up on.
#[derive(Component, Clone, Debug)]
pub struct Bee {
    pub nectar: bool,
    pub stung: bool,
    pub hive: Option<[i32; 3]>,
    pub flower: Option<[i32; 3]>,
    /// `stayOutOfHiveCountdown`, `remainingCooldownBeforeLocatingNewHive`, `...NewFlower`: the own time they end at.
    pub stay_out_until: u64,
    pub hive_cd_until: u64,
    pub flower_cd_until: u64,
    /// `ticksWithoutNectarSinceExitingHive` counts from this own time.
    pub no_nectar_since: u64,
    pub crops: i32,
    pub blacklist: Vec<[i32; 3]>,
    pub unreachable: HashMap<[i32; 3], u64>,
    pub pollinating: bool,
}
/// The occupants of the hives (the block entities of the `Mob`): who went in, when (world tick), the stay at least.
#[derive(Resource, Default)]
pub struct Hives {
    pub bees: HashMap<[i32; 3], Vec<Stored>>,
    /// The earliest world tick a stay can be over (the only thing looked at each tick).
    pub next: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct Stored {
    pub e: Entity,
    pub at: u64,
    pub min: u64,
}
/// The bee is inside a hive: out of every query (`zone::sleep`), and not woken by the zone.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct InHive(pub [i32; 3]);
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Pollinating {
    pub ticks: i32,
    pub successful: i32,
    pub hover: Option<[f64; 3]>,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct GoingHome {
    pub travelling: i32,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct GoingToFlower {
    pub travelling: i32,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct GrowingCrops;

fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
fn bee(w: &World, e: Entity) -> Option<&Bee> {
    w.get::<Bee>(e)
}
fn block_of(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
use horizon_core::geometry::Vec3;
fn closer_than(w: &World, e: Entity, p: [i32; 3], dist: i32) -> bool {
    let b = block_of(w, e);
    let d = |i: usize| f64::from(p[i] - b[i]);
    d(0) * d(0) + d(1) * d(1) + d(2) * d(2) < f64::from(dist * dist)
}
fn too_far(w: &World, e: Entity, p: [i32; 3]) -> bool {
    !closer_than(w, e, p, 48)
}
fn ensure(w: &mut World, e: Entity) {
    // The sting: a strike that lands costs the bee its life (`Bee.doHurtTarget`).
    if w.get::<crate::hostile::Attack>(e).is_some_and(|a| a.on_hit.is_none()) {
        w.get_mut::<crate::hostile::Attack>(e).unwrap().on_hit = Some(sting);
    }
    if w.get::<Bee>(e).is_none() {
        let now = now_of(w, e);
        w.entity_mut(e).insert(Bee { nectar: false, stung: false, hive: None, flower: None, stay_out_until: 0, hive_cd_until: 0, flower_cd_until: 0, no_nectar_since: now, crops: 0, blacklist: Vec::new(), unreachable: HashMap::new(), pollinating: false });
        // The constructor's draws: the cooldown before locating a flower.
        let cd = with_rng(w, e, |r| 20 + r.next_int(41)) as u64;
        w.get_mut::<Bee>(e).unwrap().flower_cd_until = now + cd;
    }
}
/// `Bee.pathfindRandomlyTowards`: a hop of a few blocks towards `p` (a flying spot nearer to it), then the path to that spot.
fn hop_towards(w: &mut World, e: Entity, p: [i32; 3]) {
    let target = [f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5];
    let here = block_of(w, e);
    let y_delta = target[1] as i32 - here[1];
    let y_adjust = if y_delta > 2 { 4 } else if y_delta < -2 { -4 } else { 0 };
    let dist = (p[0] - here[0]).abs() + (p[1] - here[1]).abs() + (p[2] - here[2]).abs();
    let (h, v) = if dist < 15 { (dist / 2, dist / 2) } else { (6, 8) };
    let max_rad = f64::from((std::f64::consts::PI / 10.0) as f32);
    if let Some(at) = crate::spots::pick_for(w, e, crate::spots::Pick::AirTowards { h, v, fly: y_adjust, to: target, max_rad }) {
        crate::goals::go_to_at(w, e, at, 1.0, 1);
    }
}
/// `BeeWanderGoal.start`'s direction: towards a valid hive farther than 24 blocks (the normalised vector), else the view.
pub fn toward_far_hive(w: &World, e: Entity) -> Option<(f64, f64)> {
    let h = bee(w, e)?.hive.filter(|_| hive_valid(w, e) && !closer_than(w, e, bee(w, e).and_then(|b| b.hive).unwrap_or([0; 3]), 24))?;
    let p = w.get::<Pos>(e)?.0;
    let v = [f64::from(h[0]) + 0.5 - p.x, f64::from(h[1]) + 0.5 - p.y, f64::from(h[2]) + 0.5 - p.z];
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1.0e-5 { Some((0.0, 0.0)) } else { Some((v[0] / len, v[2] / len)) }
}
/// The bee has stung: it will die, one chance in `1200 - t` every 5 ticks (`Bee.customServerAiStep`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Stung(pub i32);
/// `Bee.doHurtTarget`: the sting lands: the bee has stung, forgets its anger, and its sting is spent (the poison of the victim is not ported).
fn sting(w: &mut World, e: Entity) {
    if let Some(mut b) = w.get_mut::<Bee>(e) {
        b.stung = true;
        b.nectar = false;
    }
    w.entity_mut(e).insert(Stung(0));
    crate::targeting::clear_target(w, e);
    if let Some(mut a) = w.get_mut::<crate::behaviours::neutral_anger::Anger>(e) {
        a.holding = false;
        a.at = None;
    }
    w.entity_mut(e).remove::<crate::behaviours::neutral_anger::Angry>();
}
fn stung_tick(w: &mut World) {
    let es: Vec<Entity> = w.query_filtered::<Entity, With<Stung>>().iter(w).collect();
    for e in es {
        let t = {
            let mut s = w.get_mut::<Stung>(e).unwrap();
            s.0 += 1;
            s.0
        };
        if t % 5 == 0 && with_rng(w, e, |r| r.next_int((1200 - t).clamp(1, 1200))) == 0 {
            let hp = w.get::<Health>(e).map_or(1.0, |h| h.0);
            w.resource_mut::<DamageQueue>().add(e, hp, Cause::Cramming);
        }
    }
}
/// `Bee.attractsBees`.
pub fn attracts_bees(w: &World, p: [i32; 3]) -> bool {
    let mw = w.resource::<Level>().mw.0;
    let name = mw.block_name(p[0], p[1], p[2]);
    if !mw.has_tag("minecraft:bee_attractive", name) || mw.prop(p[0], p[1], p[2], "waterlogged") == Some("true") {
        return false;
    }
    name != "minecraft:sunflower" || mw.prop(p[0], p[1], p[2], "half") == Some("upper")
}
fn is_hive(w: &World, p: [i32; 3]) -> bool {
    let mw = w.resource::<Level>().mw.0;
    mw.has_tag("minecraft:beehives", mw.block_name(p[0], p[1], p[2]))
}
/// `Bee.getBeehiveBlockEntity` + `isHiveValid`: the remembered hive exists, within 48 blocks.
fn hive_valid(w: &World, e: Entity) -> bool {
    bee(w, e).and_then(|b| b.hive).is_some_and(|h| !too_far(w, e, h) && is_hive(w, h))
}
fn full(w: &World, h: [i32; 3]) -> bool {
    w.get_resource::<Hives>().is_some_and(|hv| hv.bees.get(&h).is_some_and(|v| v.len() >= 3))
}
/// The game's answer to `BEES_STAY_IN_HIVE` when it has one (the weather and the clock are the host's; the level data of the bench is static and
/// at night): `None` reads the level.
#[derive(Resource, Default)]
pub struct StayInHive(pub Option<bool>);
/// `BEES_STAY_IN_HIVE`: the overworld's night, or rain.
fn bees_stay_in_hive(w: &World) -> bool {
    if let Some(v) = w.get_resource::<StayInHive>().and_then(|s| s.0) {
        return v;
    }
    let mw = w.resource::<Level>().mw.0;
    (mw.dimension_type.has_timeline("minecraft:day") && (12542..23460).contains(&mw.day_time.rem_euclid(24000))) || mw.raining
}
fn hive_near_fire(w: &World, e: Entity) -> bool {
    let Some(h) = bee(w, e).and_then(|b| b.hive).filter(|_| hive_valid(w, e)) else { return false };
    let mw = w.resource::<Level>().mw.0;
    (-1..=1).any(|dx| (-1..=1).any(|dy| (-1..=1).any(|dz| mw.block_name(h[0] + dx, h[1] + dy, h[2] + dz) == "minecraft:fire")))
}
/// `Bee.wantsToEnterHive`.
pub fn wants_to_enter_hive(w: &World, e: Entity) -> bool {
    let Some(b) = bee(w, e) else { return false };
    let now = now_of(w, e);
    if now < b.stay_out_until || b.pollinating || b.stung || w.get::<crate::targeting::Target>(e).is_some() {
        return false;
    }
    (b.nectar || (now.saturating_sub(b.no_nectar_since) > 3600 && !b.nectar) || bees_stay_in_hive(w)) && !hive_near_fire(w, e)
}

// ---- the hive ------------------------------------------------------------------------------------------------------
/// The bee goes into its hive: out of every query for at least 600 ticks (2400 with nectar).
pub fn enter_hive(w: &mut World, e: Entity, hive: [i32; 3]) {
    let now = w.resource::<Clock>().0;
    let min = if bee(w, e).is_some_and(|b| b.nectar) { 2400 } else { 600 };
    w.get_resource_or_insert_with(Hives::default);
    let mut hv = w.resource_mut::<Hives>();
    hv.bees.entry(hive).or_default().push(Stored { e, at: now, min });
    hv.next = if hv.next == 0 { now + min } else { hv.next.min(now + min) };
    crate::zone::sleep(w, e, now);
    w.entity_mut(e).insert(InHive(hive));
}
/// The release of the occupants whose stay is over (`BeehiveBlockEntity.tickOccupants`), unless it is night or raining or the front is blocked.
pub fn release_tick(w: &mut World) {
    // A hive block that went (`PoiRegistry.removed`, readable the tick after): everyone inside comes out at once (the emergency release).
    if w.get_resource::<Hives>().is_some_and(|h| !h.bees.is_empty()) {
        let gone: Vec<[i32; 3]> = w.resource::<crate::poi::PoiRegistry>().removed().to_vec();
        for p in gone {
            if let Some(list) = w.resource_mut::<Hives>().bees.remove(&p) {
                for s in list {
                    release(w, s.e, p);
                }
            }
        }
    }
    let Some(hv) = w.get_resource::<Hives>() else { return };
    let now = w.resource::<Clock>().0;
    if hv.bees.is_empty() || now < hv.next {
        return;
    }
    let stay = bees_stay_in_hive(w);
    let hives: Vec<[i32; 3]> = hv.bees.keys().copied().collect();
    let mut next = u64::MAX;
    for h in hives {
        let list = w.resource::<Hives>().bees.get(&h).cloned().unwrap_or_default();
        let mut kept = Vec::new();
        for s in list {
            if now > s.at + s.min && !stay {
                release(w, s.e, h);
            } else {
                next = next.min(s.at + s.min + 1);
                kept.push(s);
            }
        }
        let mut hv = w.resource_mut::<Hives>();
        if kept.is_empty() {
            hv.bees.remove(&h);
        } else {
            hv.bees.insert(h, kept);
        }
    }
    // Night or rain: asked again in a second.
    w.resource_mut::<Hives>().next = if stay { now + 20 } else if next == u64::MAX { 0 } else { next };
}
fn release(w: &mut World, e: Entity, h: [i32; 3]) {
    let now = w.resource::<Clock>().0;
    w.entity_mut(e).remove::<InHive>();
    crate::zone::wake(w, e, now);
    let delivered = bee(w, e).is_some_and(|b| b.nectar);
    if let Some(mut b) = w.get_mut::<Bee>(e) {
        b.hive = Some(h);
        if delivered {
            b.nectar = false;
            b.crops = 0;
        }
    }
    let (wd, ht) = w.get::<Body>(e).map_or((0.7, 0.6), |b| (b.width, b.height));
    let mw = w.resource::<Level>().mw.0;
    let f = mw.prop(h[0], h[1], h[2], "facing").map_or([0, 0], |f| match f {
        "north" => [0, -1],
        "south" => [0, 1],
        "west" => [-1, 0],
        "east" => [1, 0],
        _ => [0, 0],
    });
    let blocked = mw.solid(h[0] + f[0], h[1], h[2] + f[1]);
    let delta = if blocked { 0.0 } else { 0.55 + f64::from(wd / 2.0) };
    let p = Vec3::new(f64::from(h[0]) + 0.5 + delta * f64::from(f[0]), f64::from(h[1]) + 0.5 - f64::from(ht / 2.0), f64::from(h[2]) + 0.5 + delta * f64::from(f[1]));
    if let Some(mut pos) = w.get_mut::<Pos>(e) {
        pos.0 = p;
    }
    if let Some(mut b) = w.get_mut::<Bounds>(e) {
        b.0 = box_at(p, wd, ht);
    }
    // `ticksWithoutNectarSinceExitingHive` restarts.
    let own = now_of(w, e);
    if let Some(mut b) = w.get_mut::<Bee>(e) {
        b.no_nectar_since = own;
    }
}

// ---- the goals -----------------------------------------------------------------------------------------------------
fn equip(w: &mut World, e: Entity, _: Inst, _: &(dyn std::any::Any + Sync)) {
    ensure(w, e);
}
/// A deadline in `ticks` of the bee's own time (never under 2: the asks come every other tick).
fn arm_in(w: &mut World, e: Entity, m: &'static Module, ticks: u64) {
    arm_module(w, e, m, ticks.max(2) as i32);
}
fn cooldown_left(w: &World, e: Entity, until: u64) -> u64 {
    until.saturating_sub(now_of(w, e))
}

/// `BeePollinateGoal`: no nectar, no cooldown, dry: the nearest flower within 5 blocks that a path reaches; fly to it, hover over it for 400 ticks
/// or more (then one chance in five per ask to stop), nectar at the end.
pub static POLLINATE: Module = Module { name: "bee_pollinate", flags: MOVE, equip, arm: pollinate_arm, fired: pollinate_fired, can_use: pollinate_can_use, start: pollinate_start, stop: pollinate_stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn pollinate_arm(w: &mut World, e: Entity, _: Inst) {
    ensure(w, e);
    let left = cooldown_left(w, e, bee(w, e).map_or(0, |b| b.flower_cd_until));
    arm_in(w, e, &POLLINATE, left);
}
fn pollinate_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let b = bee(w, e).cloned();
    let Some(b) = b else { return false };
    if cooldown_left(w, e, b.flower_cd_until) > 0 || b.nectar || w.resource::<Level>().mw.0.raining {
        // The conditions are a state, not a chance: asked again in two seconds.
        arm_in(w, e, &POLLINATE, 40);
        return false;
    }
    true
}
fn find_flower(w: &mut World, e: Entity) -> Option<[i32; 3]> {
    let here = block_of(w, e);
    {
        // The palette sweep (`may_hold`): no flower in any section of the box, no walk of its cells.
        let mw = w.resource::<Level>().mw.0;
        if !mw.world.may_hold([here[0] - 5, here[1] - 5, here[2] - 5], [here[0] + 5, here[1] + 5, here[2] + 5], |s| mw.has_tag("minecraft:bee_attractive", &mw.facts.get(s).name)) {
            if let Some(mut b) = w.get_mut::<Bee>(e) {
                b.unreachable.clear();
            }
            return None;
        }
    }
    let world_now = w.resource::<Clock>().0;
    let mut cache: HashMap<[i32; 3], u64> = HashMap::new();
    let old = bee(w, e).map(|b| b.unreachable.clone()).unwrap_or_default();
    let mut found = None;
    for p in manhattan_box(here, 5, 5, 5) {
        if !attracts_bees(w, p) {
            continue;
        }
        let until = old.get(&p).copied().unwrap_or(0);
        if world_now < until {
            cache.insert(p, until);
            continue;
        }
        if crate::locomotion::fly::reaches(w, e, p, 1) {
            found = Some(p);
            break;
        }
        cache.insert(p, world_now + 600);
    }
    if let Some(mut b) = w.get_mut::<Bee>(e) {
        b.unreachable = cache;
    }
    found
}
/// `BlockPos.withinBoxByManhattanDistance`: nearest first by Manhattan distance, then x, y, and +z before -z.
fn manhattan_box(o: [i32; 3], rx: i32, ry: i32, rz: i32) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    for depth in 0..=rx + ry + rz {
        let max_x = rx.min(depth);
        for x in -max_x..=max_x {
            let max_y = ry.min(depth - x.abs());
            for y in -max_y..=max_y {
                let z = depth - x.abs() - y.abs();
                if z <= rz {
                    out.push([o[0] + x, o[1] + y, o[2] + z]);
                    if z != 0 {
                        out.push([o[0] + x, o[1] + y, o[2] - z]);
                    }
                }
            }
        }
    }
    out
}
fn pollinate_can_use(w: &mut World, e: Entity, i: Inst) -> bool {
    match find_flower(w, e) {
        Some(p) => {
            let now = now_of(w, e);
            if let Some(mut b) = w.get_mut::<Bee>(e) {
                b.flower = Some(p);
            }
            let _ = now;
            // The `Mob` also asks a path here (speed 1.2), but the goal's own wanted point (0.35) comes after the navigation's in its tick and
            // wins every tick: the flight is the direct one.
            true
        }
        None => {
            let cd = with_rng(w, e, |r| 20 + r.next_int(41)) as u64;
            let now = now_of(w, e);
            if let Some(mut b) = w.get_mut::<Bee>(e) {
                b.flower_cd_until = now + cd;
            }
            arm_in(w, e, &POLLINATE, cd);
            let _ = i;
            false
        }
    }
}
fn pollinate_start(w: &mut World, e: Entity, _: Inst) {
    let now = now_of(w, e);
    if let Some(mut b) = w.get_mut::<Bee>(e) {
        b.pollinating = true;
        b.no_nectar_since = now;
    }
    w.entity_mut(e).insert(Pollinating { ticks: 0, successful: 0, hover: None });
}
fn pollinate_stop(w: &mut World, e: Entity, i: Inst) {
    let long = w.get::<Pollinating>(e).is_some_and(|p| p.successful > 400);
    w.entity_mut(e).remove::<Pollinating>();
    let now = now_of(w, e);
    if let Some(mut b) = w.get_mut::<Bee>(e) {
        if long {
            b.nectar = true;
        }
        b.pollinating = false;
        b.flower_cd_until = now + 200;
    }
    crate::goals::stop_nav(w, e);
    arm_in(w, e, &POLLINATE, 200);
    let _ = i;
}
fn drop_flower(w: &mut World, e: Entity) {
    let cd = with_rng(w, e, |r| 20 + r.next_int(41)) as u64;
    let now = now_of(w, e);
    if let Some(mut b) = w.get_mut::<Bee>(e) {
        b.flower = None;
        b.flower_cd_until = now + cd;
    }
}
fn pollinate_tick(w: &mut World) {
    let es: Vec<Entity> = w.query_filtered::<Entity, With<Pollinating>>().iter(w).collect();
    for e in es {
        let Some(f) = bee(w, e).and_then(|b| b.flower) else { end(w, e, &POLLINATE); continue };
        let raining = w.resource::<Level>().mw.0.raining;
        let mut p = *w.get::<Pollinating>(e).unwrap();
        p.ticks += 1;
        if p.ticks > 600 {
            drop_flower(w, e);
            let now = now_of_ref(w, e);
            if let Some(mut b) = w.get_mut::<Bee>(e) {
                b.pollinating = false;
                b.flower_cd_until = now + 200;
            }
            end(w, e, &POLLINATE);
            continue;
        }
        if raining {
            end(w, e, &POLLINATE);
            continue;
        }
        let flower = [f64::from(f[0]) + 0.5, f64::from(f[1]) + f64::from(0.6_f32), f64::from(f[2]) + 0.5];
        let pos = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
        let dist = |a: [f64; 3]| ((pos.x - a[0]).powi(2) + (pos.y - a[1]).powi(2) + (pos.z - a[2]).powi(2)).sqrt();
        // `BeePollinateGoal.setWantedPos`: a direct wanted point at 0.35 (the path of the start, at 1.2, goes on while it lasts: the navigation's
        // wanted point comes last in the `Mob`'s tick).
        let steer = |w: &mut World, h: [f64; 3]| crate::locomotion::fly::steer_to(w, e, h, f64::from(0.35_f32));
        if dist(flower) > 1.0 {
            p.hover = Some(flower);
            steer(w, flower);
            *w.get_mut::<Pollinating>(e).unwrap() = p;
            continue;
        }
        let hover = *p.hover.get_or_insert(flower);
        let arrived = dist(hover) <= 0.1;
        let mut set_wanted = true;
        if arrived {
            if with_rng(w, e, |r| r.next_int(25)) == 0 {
                let o = |r: &mut Rng| f64::from((r.next_float() * 2.0 - 1.0) * 0.333_333_34);
                let (ox, oz) = with_rng(w, e, |r| (o(r), o(r)));
                p.hover = Some([flower[0] + ox, flower[1], flower[2] + oz]);
                crate::goals::stop_nav(w, e);
            } else {
                set_wanted = false;
            }
        }
        if set_wanted && let Some(h) = p.hover {
            steer(w, h);
        }
        p.successful += 1;
        *w.get_mut::<Pollinating>(e).unwrap() = p;
        // `canContinueToUse`: after 400 ticks one chance in five each ask (every other tick).
        if p.successful > 400 && p.successful % 2 == 0 && with_rng(w, e, |r| r.next_float()) < 0.2 {
            end(w, e, &POLLINATE);
        }
    }
}
fn now_of_ref(w: &World, e: Entity) -> u64 {
    now_of(w, e)
}

/// `BeeLocateHiveGoal`: no hive and no cooldown, wants to enter one: the nearest hive with room within 20 blocks (not one it gave up on).
pub static LOCATE_HIVE: Module = Module { name: "bee_locate_hive", flags: 0, equip, arm: locate_arm, fired: locate_fired, can_use: locate_can_use, start: locate_start, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn locate_arm(w: &mut World, e: Entity, _: Inst) {
    ensure(w, e);
    arm_in(w, e, &LOCATE_HIVE, 40);
}
fn locate_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    arm_in(w, e, &LOCATE_HIVE, 40);
    true
}
fn locate_can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    bee(w, e).is_some_and(|b| now_of(w, e) >= b.hive_cd_until && b.hive.is_none()) && wants_to_enter_hive(w, e)
}
fn locate_start(w: &mut World, e: Entity, _: Inst) {
    let now = now_of(w, e);
    let here = block_of(w, e);
    let reg = w.resource::<crate::poi::PoiRegistry>();
    let mut hives: Vec<[i32; 3]> = reg.in_range(here, 20, false, |k| k == horizon_world::poi::PoiType::Beehive).into_iter().map(|(p, _)| p).filter(|&p| !full(w, p) && is_hive(w, p)).collect();
    let dist = |p: &[i32; 3]| (0..3).map(|i| i64::from(p[i] - here[i]).pow(2)).sum::<i64>();
    hives.sort_by_key(|p| (dist(p), *p));
    let b = w.get_mut::<Bee>(e);
    let Some(mut b) = b else { return };
    b.hive_cd_until = now + 200;
    let pick = hives.iter().find(|h| !b.blacklist.contains(h)).copied();
    if pick.is_none() && !hives.is_empty() {
        b.blacklist.clear();
    }
    b.hive = pick.or_else(|| hives.first().copied()).or(b.hive);
    end(w, e, &LOCATE_HIVE);
}

/// `BeeGoToHiveGoal`: a hive it knows, it wants to go in, not there yet: fly to it; gives up after 2400 ticks or when stuck.
pub static GO_TO_HIVE: Module = Module { name: "bee_go_to_hive", flags: MOVE, equip, arm: go_arm, fired: go_fired, can_use: go_can_use, start: go_start, stop: go_stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn go_arm(w: &mut World, e: Entity, _: Inst) {
    ensure(w, e);
    arm_in(w, e, &GO_TO_HIVE, 40);
}
fn go_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    arm_in(w, e, &GO_TO_HIVE, 40);
    true
}
fn reached_hive(w: &World, e: Entity, h: [i32; 3]) -> bool {
    closer_than(w, e, h, 2)
}
fn go_can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(h) = bee(w, e).and_then(|b| b.hive) else { return false };
    !too_far(w, e, h) && wants_to_enter_hive(w, e) && !reached_hive(w, e, h) && is_hive(w, h)
}
fn go_start(w: &mut World, e: Entity, _: Inst) {
    // Nothing is asked here (`BeeGoToHiveGoal.start` only resets its counters): the first tick asks.
    w.entity_mut(e).insert(GoingHome { travelling: 0 });
}
fn go_stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<GoingHome>();
    crate::goals::stop_nav(w, e);
    arm_in(w, e, &GO_TO_HIVE, 40);
}
fn drop_hive(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    if let Some(mut b) = w.get_mut::<Bee>(e) {
        b.hive = None;
        b.hive_cd_until = now + 200;
    }
}
fn go_tick(w: &mut World) {
    let es: Vec<Entity> = w.query_filtered::<Entity, With<GoingHome>>().iter(w).collect();
    for e in es {
        let Some(h) = bee(w, e).and_then(|b| b.hive) else { end(w, e, &GO_TO_HIVE); continue };
        let travelling = {
            let mut g = w.get_mut::<GoingHome>(e).unwrap();
            g.travelling += 1;
            g.travelling
        };
        if travelling > reduced(2400) * 2 {
            if let Some(mut b) = w.get_mut::<Bee>(e) {
                b.blacklist.push(h);
                while b.blacklist.len() > 3 {
                    b.blacklist.remove(0);
                }
            }
            drop_hive(w, e);
            end(w, e, &GO_TO_HIVE);
        } else if reached_hive(w, e, h) {
            // The walk is over: in.
            end(w, e, &GO_TO_HIVE);
            let slot = w.get::<GoalSet>(e).and_then(|g| g.slot_of(&ENTER_HIVE));
            if let Some(s) = slot {
                w.resource_mut::<Requests>().0.push(Req::Start(e, s));
            }
        } else if w.get::<Following>(e).is_none() && w.get::<NeedsPath>(e).is_none() {
            // No path in progress: ask again, or give the hive up when it is too far.
            if !closer_than(w, e, h, 16) {
                if too_far(w, e, h) {
                    drop_hive(w, e);
                    end(w, e, &GO_TO_HIVE);
                } else {
                    hop_towards(w, e, h);
                }
            } else {
                // `pathfindDirectly`: within 3 blocks the path may end at 1, else at 2; no path that reaches: the hive is given up (and kept
                // from the next choices).
                let reach = if closer_than(w, e, h, 3) { 1 } else { 2 };
                if crate::locomotion::fly::reaches(w, e, h, reach) {
                    crate::goals::go_to_at(w, e, [f64::from(h[0]) + 0.5, f64::from(h[1]) + 0.5, f64::from(h[2]) + 0.5], 1.0, reach);
                } else {
                    if let Some(mut b) = w.get_mut::<Bee>(e) {
                        b.blacklist.push(h);
                        while b.blacklist.len() > 3 {
                            b.blacklist.remove(0);
                        }
                    }
                    drop_hive(w, e);
                    end(w, e, &GO_TO_HIVE);
                }
            }
        }
    }
}

/// `BeeEnterHiveGoal`: within 2 blocks of its hive, it wants to go in and the hive has room: in; a full hive is forgotten.
pub static ENTER_HIVE: Module = Module { name: "bee_enter_hive", flags: 0, equip, arm: enter_arm, fired: enter_fired, can_use: enter_can_use, start: enter_start, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn enter_arm(w: &mut World, e: Entity, _: Inst) {
    ensure(w, e);
    arm_in(w, e, &ENTER_HIVE, 40);
}
fn enter_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    arm_in(w, e, &ENTER_HIVE, 40);
    true
}
fn enter_can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(h) = bee(w, e).and_then(|b| b.hive) else { return false };
    let p = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let close = (f64::from(h[0]) + 0.5 - p.x).powi(2) + (f64::from(h[1]) + 0.5 - p.y).powi(2) + (f64::from(h[2]) + 0.5 - p.z).powi(2) < 4.0;
    if !(close && wants_to_enter_hive(w, e)) {
        return false;
    }
    if !hive_valid(w, e) {
        return false;
    }
    if full(w, h) {
        if let Some(mut b) = w.get_mut::<Bee>(e) {
            b.hive = None;
        }
        return false;
    }
    true
}
fn enter_start(w: &mut World, e: Entity, _: Inst) {
    if let Some(h) = bee(w, e).and_then(|b| b.hive) {
        enter_hive(w, e, h);
    }
    end(w, e, &ENTER_HIVE);
}

/// `ValidateHiveGoal` and `ValidateFlowerGoal`: every 20 to 40 ticks, forget a hive or a flower that is gone.
pub static VALIDATE_HIVE: Module = Module { name: "bee_validate_hive", flags: 0, equip, arm: vh_arm, fired: vh_fired, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn vh_arm(w: &mut World, e: Entity, _: Inst) {
    ensure(w, e);
    let cd = with_rng(w, e, |r| 20 + r.next_int(21)) as u64;
    arm_in(w, e, &VALIDATE_HIVE, cd);
}
fn vh_fired(w: &mut World, e: Entity, i: Inst) -> bool {
    if bee(w, e).and_then(|b| b.hive).is_some_and(|h| !is_hive(w, h)) {
        drop_hive(w, e);
    }
    vh_arm(w, e, i);
    false
}
pub static VALIDATE_FLOWER: Module = Module { name: "bee_validate_flower", flags: 0, equip, arm: vf_arm, fired: vf_fired, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn vf_arm(w: &mut World, e: Entity, _: Inst) {
    ensure(w, e);
    let cd = with_rng(w, e, |r| 20 + r.next_int(21)) as u64;
    arm_in(w, e, &VALIDATE_FLOWER, cd);
}
fn vf_fired(w: &mut World, e: Entity, i: Inst) -> bool {
    if bee(w, e).and_then(|b| b.flower).is_some_and(|f| !attracts_bees(w, f)) {
        drop_flower(w, e);
    }
    vf_arm(w, e, i);
    false
}

/// `BeeGoToKnownFlowerGoal`: after 600 ticks without nectar, back to the remembered flower; gives up after 2400 ticks.
pub static GO_TO_KNOWN_FLOWER: Module = Module { name: "bee_go_to_known_flower", flags: MOVE, equip, arm: kf_arm, fired: kf_fired, can_use: kf_can_use, start: kf_start, stop: kf_stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn kf_arm(w: &mut World, e: Entity, _: Inst) {
    ensure(w, e);
    arm_in(w, e, &GO_TO_KNOWN_FLOWER, 40);
}
fn kf_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    arm_in(w, e, &GO_TO_KNOWN_FLOWER, 40);
    true
}
fn kf_can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(b) = bee(w, e) else { return false };
    b.flower.is_some_and(|f| now_of(w, e).saturating_sub(b.no_nectar_since) > 600 && !b.nectar && !closer_than(w, e, f, 2))
}
fn kf_start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(GoingToFlower { travelling: 0 });
}
fn kf_stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<GoingToFlower>();
    crate::goals::stop_nav(w, e);
    arm_in(w, e, &GO_TO_KNOWN_FLOWER, 40);
}
fn kf_tick(w: &mut World) {
    let es: Vec<Entity> = w.query_filtered::<Entity, With<GoingToFlower>>().iter(w).collect();
    for e in es {
        let Some(f) = bee(w, e).and_then(|b| b.flower) else { end(w, e, &GO_TO_KNOWN_FLOWER); continue };
        let travelling = {
            let mut g = w.get_mut::<GoingToFlower>(e).unwrap();
            g.travelling += 1;
            g.travelling
        };
        if travelling > reduced(2400) * 2 {
            drop_flower(w, e);
            end(w, e, &GO_TO_KNOWN_FLOWER);
        } else if closer_than(w, e, f, 2) {
            end(w, e, &GO_TO_KNOWN_FLOWER);
        } else if w.get::<Following>(e).is_none() && w.get::<NeedsPath>(e).is_none() {
            if too_far(w, e, f) {
                drop_flower(w, e);
                end(w, e, &GO_TO_KNOWN_FLOWER);
            } else {
                hop_towards(w, e, f);
            }
        }
    }
}

/// `BeeGrowCropGoal`: with nectar and a hive, fewer than 10 crops, one chance in 30 per ask: the crop 1 or 2 blocks below grows one stage.
/// Not ported here: the block change (the frame has no `SetBlock` service for a bee): the goal counts the crops it would grow.
pub static GROW_CROP: Module = Module { name: "bee_grow_crop", flags: 0, equip, requires: &[Cap::Locomotion], ..Module::DEFAULT };

/// The ticks of the goals above and the hive's release, run by `arbitrate` while a bee lives.
pub fn tick_all(w: &mut World) {
    static IDS: std::sync::OnceLock<[Option<usize>; 3]> = std::sync::OnceLock::new();
    let ids = IDS.get_or_init(|| {
        let r = crate::species::registry();
        [&POLLINATE, &GO_TO_HIVE, &GO_TO_KNOWN_FLOWER].map(|m| r.module_id(m))
    });
    let alive = |w: &World, k: usize| ids[k].is_some_and(|i| w.resource::<Carriers>().0.get(i).is_some_and(|n| *n > 0));
    if alive(w, 0) {
        pollinate_tick(w);
    }
    if alive(w, 1) {
        go_tick(w);
    }
    if alive(w, 2) {
        kf_tick(w);
    }
    if alive(w, 0) || alive(w, 1) || alive(w, 2) {
        stung_tick(w);
    }
    release_tick(w);
}
