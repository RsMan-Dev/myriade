//! The bee (`Bee`, 1534 lines of `Mob`): its state, the hive service (a bee in a hive is out of the world for its time inside), and the goals of
//! its cycle: pollinate a flower, go home with the nectar, enter the hive, come out, locate another, grow crops, validate what it remembers.
//! No scan and no system: the asks of the `Mob` (every other tick) are deadlines (a goal that cannot run re-arms itself at the next time its
//! condition can change), the goals that tick are visited by their module's `tick` (`arbitrate` calls it while a bee lives) through markers, the hive is a resource whose
//! next release time is the only thing looked at each tick.
//! What it asks of the frame (and has here): `Hives` (the occupants of each hive: entity, tick it went in, minimum stay), `InHive` (the bee is
//! out of every query: `zone::sleep`; the zone does not wake it), the release (position in front of the hive, nectar delivered, honey).
//! The sting and the anger (`BeeAttackGoal`, `BeeBecomeAngryTargetGoal`, `BeeHurtByOtherGoal`) are the frame's melee, nearest_target, hurt_by and
//! neutral_anger, configured in `species/bee.rs`; `sting` is the melee's `on_hit` hook. The age and the love move by the time spent in the hive (`age_in_hive`). The poison follows the difficulty (`effects::Difficulty`) and is given when the
//! blow lands (`StingPending`). Not ported (the frame has no block write nor sound output: the `Mob` hands them to the host): the honey level of the
//! hive block, the crops grown, the sounds and the particles.
use super::*;
use crate::{cow::*, goals::*};
use std::collections::HashMap;
use crate::rules::bee::{close_to_hive, exit_point, front_of, hop_plan, hover_offset, manhattan_box, may_stop_hover, unit_toward_xz, within_blocks};
use crate::rules::nearest_target::reduced;

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

fn bee(w: &World, e: Entity) -> Option<&Bee> {
    w.get::<Bee>(e)
}
fn block_of(w: &World, e: Entity) -> [i32; 3] {
    let p = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
use horizon_core::geometry::Vec3;
fn closer_than(w: &World, e: Entity, p: [i32; 3], dist: i32) -> bool {
    within_blocks(block_of(w, e), p, dist)
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
    let (y_adjust, h, v) = hop_plan(block_of(w, e), p);
    let max_rad = f64::from((std::f64::consts::PI / 10.0) as f32);
    if let Some(at) = crate::spots::pick_for(w, e, crate::spots::Pick::AirTowards { h, v, fly: y_adjust, to: target, max_rad }) {
        crate::goals::go_to_at(w, e, at, 1.0, 1);
    }
}
/// `BeeWanderGoal.start`'s direction: towards a valid hive farther than 24 blocks (the normalised vector), else the view.
pub fn toward_far_hive(w: &World, e: Entity) -> Option<(f64, f64)> {
    let h = bee(w, e)?.hive.filter(|_| hive_valid(w, e) && !closer_than(w, e, bee(w, e).and_then(|b| b.hive).unwrap_or([0; 3]), 24))?;
    Some(unit_toward_xz(h, w.get::<Pos>(e)?.0))
}
/// The bee has stung: it will die, one chance in `1200 - t` every 5 ticks (`Bee.customServerAiStep`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Stung(pub i32);
/// The seconds of poison of the sting by difficulty id (peaceful, easy, normal, hard): `Bee.doHurtTarget` (the `Mob`'s `mobs/bee.rs::sting`).
pub const STING_POISON_SECONDS: [i32; 4] = [0, 0, 10, 18];
/// A sting waiting to know whether the blow landed (`Bee.doHurtTarget` takes the stinger and the anger first, the game gives the poison only when
/// the victim took the hit; a refused hit puts the stinger and the anger back). For a creature the damage queue answers (`LastHurtBy` of the
/// victim); a player's hurt is the host's (`PlayerHit`): its sting counts as landed.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
struct StingPending {
    victim: Entity,
    /// The victim's own tick when the blow was struck, and the world's.
    seen: u64,
    since: u64,
    nectar: bool,
    target: crate::targeting::Target,
}
/// `Bee.doHurtTarget`: the sting lands: the bee has stung, forgets its anger, and its sting is spent; the poison follows the hit.
fn sting(w: &mut World, e: Entity) {
    use crate::targeting::Who;
    let target = w.get::<crate::targeting::Target>(e).copied();
    let nectar = bee(w, e).is_some_and(|b| b.nectar);
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
    if let Some(t) = target {
        match t.who {
            Who::Entity(victim) => {
                let (seen, since) = (now_of(w, victim), w.resource::<Clock>().0);
                w.entity_mut(e).insert(StingPending { victim, seen, since, nectar, target: t });
            }
            Who::Player(id) => poison(w, Who::Player(id)),
        }
    }
}
/// The poison of the sting on `who`, by the level's difficulty.
fn poison(w: &mut World, who: crate::targeting::Who) {
    use crate::targeting::Who;
    let ticks = STING_POISON_SECONDS[usize::from(crate::effects::difficulty(w)).min(3)] * 20;
    if ticks == 0 {
        return;
    }
    match who {
        Who::Entity(v) => drop(crate::effects::add(w, v, crate::effects::Kind::Poison, 0, ticks)),
        Who::Player(id) => crate::effects::add_player(w, id, crate::effects::Kind::Poison, 0, ticks),
    }
}
/// The answer of the damage queue to the stings: the victim's `LastHurtBy` names the bee (landed: the poison), or three ticks pass without it
/// (refused: the stinger, the nectar, the target and so the anger come back; a victim that is gone died of the blow).
fn resolve_stings(w: &mut World) {
    use crate::targeting::Who;
    let es: Vec<Entity> = crate::goals::entities::<With<StingPending>>(w);
    for e in es {
        let s = *w.get::<StingPending>(e).unwrap();
        let landed = if w.get_entity(s.victim).is_err() {
            Some(false)
        } else if w.get::<LastHurtBy>(s.victim).is_some_and(|l| matches!(l.who, Who::Entity(x) if x == e) && l.at >= s.seen) {
            Some(true)
        } else if w.resource::<Clock>().0 > s.since + 3 {
            None
        } else {
            continue;
        };
        w.entity_mut(e).remove::<StingPending>();
        match landed {
            Some(true) => poison(w, Who::Entity(s.victim)),
            Some(false) => {}
            None => {
                if let Some(mut b) = w.get_mut::<Bee>(e) {
                    b.stung = false;
                    b.nectar = s.nectar;
                }
                w.entity_mut(e).remove::<Stung>();
                if w.get::<crate::targeting::Target>(e).is_none() {
                    crate::targeting::set_target(w, e, s.target);
                }
            }
        }
    }
}
fn stung_tick(w: &mut World) {
    let es: Vec<Entity> = crate::goals::entities::<With<Stung>>(w);
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
                    let stay = w.resource::<Clock>().0 - s.at;
                    release(w, s.e, p, stay);
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
                release(w, s.e, h, now - s.at);
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
/// `BeehiveBlockEntity.setBeeReleaseData`: the time inside counts for the age and the love (its own time stood still): a baby grows by that much,
/// the breeding rest of an adult shortens by that much, the love too.
fn age_in_hive(w: &mut World, e: Entity, stay: u64) {
    let now = now_of(w, e);
    if let Some(g) = w.get::<GrowAt>(e).map(|g| g.0).filter(|&g| g != u64::MAX) {
        let at = g.saturating_sub(stay).max(now + 1);
        w.entity_mut(e).insert(GrowAt(at));
        crate::cow::arm_at(w, e, at, crate::cow::Due::Grow);
    }
    if let Some(r) = w.get::<crate::behaviours::breed::Resting>(e).map(|r| r.0).filter(|&r| r > now) {
        match r.saturating_sub(stay) {
            left if left > now => drop(w.entity_mut(e).insert(crate::behaviours::breed::Resting(left))),
            _ => drop(w.entity_mut(e).remove::<crate::behaviours::breed::Resting>()),
        }
    }
    if let Some(l) = w.get::<crate::behaviours::breed::Love>(e).map(|l| l.0).filter(|&l| l > now) {
        match l.saturating_sub(stay) {
            left if left > now => drop(w.entity_mut(e).insert(crate::behaviours::breed::Love(left))),
            _ => drop(w.entity_mut(e).remove::<crate::behaviours::breed::Love>()),
        }
    }
}
fn release(w: &mut World, e: Entity, h: [i32; 3], stay: u64) {
    let now = w.resource::<Clock>().0;
    w.entity_mut(e).remove::<InHive>();
    crate::zone::wake(w, e, now);
    age_in_hive(w, e, stay);
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
    let f = front_of(mw.prop(h[0], h[1], h[2], "facing"));
    let blocked = mw.solid(h[0] + f[0], h[1], h[2] + f[1]);
    let p = exit_point(h, f, blocked, wd, ht);
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
    schedule_hive(w, e);
    schedule_flower(w, e);
    arm_validate_hive(w, e);
    if delivered {
        ask_pollinate(w, e);
    }
}

// ---- the goals -----------------------------------------------------------------------------------------------------
fn equip(w: &mut World, e: Entity, _: Inst, _: &'static (dyn std::any::Any + Sync)) {
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
pub static POLLINATE: Module = Module { name: "bee_pollinate", tick: Some(pollinate_tick), flags: MOVE, equip, arm: pollinate_arm, fired: pollinate_fired, can_use: pollinate_can_use, start: pollinate_start, stop: pollinate_stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn pollinate_arm(w: &mut World, e: Entity, _: Inst) {
    ensure(w, e);
    let left = cooldown_left(w, e, bee(w, e).map_or(0, |b| b.flower_cd_until));
    arm_in(w, e, &POLLINATE, left);
}
/// The cooldown is a deadline of the wheel; with nectar or in the rain nothing is asked until an EVENT says otherwise (the nectar delivered, the
/// weather changed: `ask_pollinate`).
fn pollinate_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(b) = bee(w, e).cloned() else { return false };
    let left = cooldown_left(w, e, b.flower_cd_until);
    if left > 0 {
        arm_in(w, e, &POLLINATE, left);
        return false;
    }
    !(b.nectar || w.resource::<Level>().mw.0.raining || bees_stay_in_hive(w) && false)
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
fn pollinate_can_use(w: &mut World, e: Entity, i: Inst) -> bool {
    match find_flower(w, e) {
        Some(p) => {
            let now = now_of(w, e);
            if let Some(mut b) = w.get_mut::<Bee>(e) {
                b.flower = Some(p);
            }
            let _ = now;
            arm_validate_flower(w, e);
            schedule_flower(w, e);
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
    if long {
        // The nectar changes what the bee wants: the hive goals are asked at once.
        ask_hive(w, e);
    }
    schedule_flower(w, e);
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
    let es: Vec<Entity> = crate::goals::entities::<With<Pollinating>>(w);
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
                let (ox, oz) = with_rng(w, e, |r| (hover_offset(r.next_float()), hover_offset(r.next_float())));
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
        if may_stop_hover(p.successful) && with_rng(w, e, |r| r.next_float()) < 0.2 {
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
    schedule_hive(w, e);
}
/// The time a condition of the hive goals changes by itself has come (the stay-out countdown, the cooldown, the tiredness): the three goals are asked.
fn locate_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    ask_hive(w, e);
    schedule_hive(w, e);
    false
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
    arm_validate_hive(w, e);
    ask(w, e, &GO_TO_HIVE);
    ask(w, e, &ENTER_HIVE);
    schedule_hive(w, e);
}

/// `BeeGoToHiveGoal`: a hive it knows, it wants to go in, not there yet: fly to it; gives up after 2400 ticks or when stuck.
pub static GO_TO_HIVE: Module = Module { name: "bee_go_to_hive", tick: Some(go_tick), flags: MOVE, equip, can_use: go_can_use, start: go_start, stop: go_stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
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
    let es: Vec<Entity> = crate::goals::entities::<With<GoingHome>>(w);
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
pub static ENTER_HIVE: Module = Module { name: "bee_enter_hive", tick: Some(hive_tick), flags: 0, equip, can_use: enter_can_use, start: enter_start, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn enter_can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(h) = bee(w, e).and_then(|b| b.hive) else { return false };
    let p = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let close = close_to_hive(h, p, 4.0);
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
/// Armed only while a hive is remembered (and by the event of a hive gone): a bee that knows none costs nothing.
fn vh_arm(w: &mut World, e: Entity, _: Inst) {
    ensure(w, e);
    arm_validate_hive(w, e);
}
fn arm_validate_hive(w: &mut World, e: Entity) {
    if bee(w, e).is_some_and(|b| b.hive.is_some()) {
        let cd = with_rng(w, e, |r| 20 + r.next_int(21)) as u64;
        arm_in(w, e, &VALIDATE_HIVE, cd);
    }
}
fn vh_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if bee(w, e).and_then(|b| b.hive).is_some_and(|h| !is_hive(w, h)) {
        drop_hive(w, e);
    }
    arm_validate_hive(w, e);
    false
}
pub static VALIDATE_FLOWER: Module = Module { name: "bee_validate_flower", flags: 0, equip, arm: vf_arm, fired: vf_fired, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn vf_arm(w: &mut World, e: Entity, _: Inst) {
    ensure(w, e);
    arm_validate_flower(w, e);
}
fn arm_validate_flower(w: &mut World, e: Entity) {
    if bee(w, e).is_some_and(|b| b.flower.is_some()) {
        let cd = with_rng(w, e, |r| 20 + r.next_int(21)) as u64;
        arm_in(w, e, &VALIDATE_FLOWER, cd);
    }
}
fn vf_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if bee(w, e).and_then(|b| b.flower).is_some_and(|f| !attracts_bees(w, f)) {
        drop_flower(w, e);
    }
    arm_validate_flower(w, e);
    false
}

/// `BeeGoToKnownFlowerGoal`: after 600 ticks without nectar, back to the remembered flower; gives up after 2400 ticks.
pub static GO_TO_KNOWN_FLOWER: Module = Module { name: "bee_go_to_known_flower", tick: Some(kf_tick), flags: MOVE, equip, arm: kf_arm, fired: kf_fired, can_use: kf_can_use, start: kf_start, stop: kf_stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn kf_arm(w: &mut World, e: Entity, _: Inst) {
    ensure(w, e);
    schedule_flower(w, e);
}
fn kf_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    schedule_flower(w, e);
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
    let es: Vec<Entity> = crate::goals::entities::<With<GoingToFlower>>(w);
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


// ---- events instead of probes ---------------------------------------------------------------------------------------
fn ask(w: &mut World, e: Entity, m: &'static Module) {
    if let Some(slot) = w.get::<GoalSet>(e).and_then(|g| g.slot_of(m)) {
        w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
    }
}
/// The hive goals are asked (a hive was set or dropped, the nectar changed hands, the weather changed, a countdown ended).
pub fn ask_hive(w: &mut World, e: Entity) {
    ask(w, e, &LOCATE_HIVE);
    ask(w, e, &GO_TO_HIVE);
    ask(w, e, &ENTER_HIVE);
}
fn ask_pollinate(w: &mut World, e: Entity) {
    ask(w, e, &POLLINATE);
}
/// The next time a condition of the hive goals changes by itself: the end of the stay-out countdown, of the locating cooldown, or the 3600 ticks
/// without nectar: one deadline of the wheel (the events cover the rest).
fn schedule_hive(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    let Some(b) = bee(w, e) else { return };
    let next = [b.stay_out_until, b.hive_cd_until, b.no_nectar_since + 3601].into_iter().filter(|&t| t > now).min();
    if let Some(t) = next {
        arm_in(w, e, &LOCATE_HIVE, t - now);
    }
}
/// The remembered flower is asked for again 600 ticks after the last nectar.
fn schedule_flower(w: &mut World, e: Entity) {
    let now = now_of(w, e);
    let Some(b) = bee(w, e) else { return };
    if b.flower.is_some() && !b.nectar {
        let t = b.no_nectar_since + 601;
        arm_in(w, e, &GO_TO_KNOWN_FLOWER, t.saturating_sub(now));
    }
}
/// The tick of every bee that lives (the module that all carry): the hive's release, the hive that went (the event of the points of interest),
/// the weather that changed (`StayInHive`: the host writes it when the day turns to night or the rain starts or stops), the stings.
pub fn hive_tick(w: &mut World) {
    release_tick(w);
    stung_tick(w);
    resolve_stings(w);
    if w.is_resource_changed::<StayInHive>() {
        let es: Vec<Entity> = crate::goals::entities::<With<Bee>>(w);
        for e in es {
            ask_hive(w, e);
            ask_pollinate(w, e);
        }
    }
    if w.resource::<crate::poi::PoiRegistry>().removed().is_empty() {
        return;
    }
    let gone: Vec<[i32; 3]> = w.resource::<crate::poi::PoiRegistry>().removed().to_vec();
    let es: Vec<Entity> = crate::goals::entities::<With<Bee>>(w);
    for e in es {
        if bee(w, e).and_then(|b| b.hive).is_some_and(|h| gone.contains(&h)) {
            drop_hive(w, e);
            ask_hive(w, e);
        }
    }
}
