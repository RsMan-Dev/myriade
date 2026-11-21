//! The goals of the two fliers that move by a wanted point (`fly::Control::Direct` and `Bank`): the vex (`VexChargeAttackGoal`,
//! `VexRandomMoveGoal`, `VexCopyOwnerTargetGoal`) and the phantom (`PhantomAttackPlayerTargetGoal`, `PhantomAttackStrategyGoal`,
//! `PhantomSweepAttackGoal`, `PhantomCircleAroundAnchorGoal`). No scan: the chances are deadlines (a geometric wait, as `stroll`), what
//! changes the state wakes a goal by event (a target gained or lost, the dive set or cleared). The goals that tick (the charge, the dive,
//! the circle, the strategy) have a marker that their system visits: a goal that does not run costs nothing.
//! The `Mob` ticks a goal that is not `every_tick` every other tick: the counts here are in ticks (twice the `reduced` ones).
//! Not ported: the sounds, the owner link made at the summon. The cats that scare a diving phantom are `hiss::scare`.
use crate::IFloor as _;
use super::*;
use crate::targeting::{self, Target, Who};
use crate::{cow::*, goals::*};
use crate::rules::flier_goals::*;
use crate::rules::follow_mob::recalc_after;
use horizon_core::geometry::Vec3;

/// The phantom dives (`Mob.extra.flag`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Diving;

fn target_of(w: &World, e: Entity) -> Option<(Who, targeting::Info)> {
    let who = w.get::<Target>(e)?.who;
    targeting::info(w, who).filter(|i| !i.exempt).map(|i| (who, i))
}
/// One strike at the target (`Mob.doHurtTarget`): the attack damage, from the animal.
fn hit(w: &mut World, e: Entity, who: Who) {
    let amount = w.get::<AttackDamage>(e).map_or(1.0, |d| strike_amount(d.0));
    crate::hostile::strike(w, e, who, amount);
}
fn wanted_active(w: &World, e: Entity) -> bool {
    w.get::<MoveTo>(e).is_some_and(|m| m.active)
}

// ---- the vex ------------------------------------------------------------------------------------------------------
#[derive(Clone, Copy, Debug)]
pub struct Params {}
impl Params {
    pub const DEFAULT: Self = Self {};
}
/// The charge runs (its system visits it).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Charging;

/// `VexChargeAttackGoal`: a target alive, nothing wanted, one chance in 7 per ask, farther than 2 blocks: fly straight at its eyes; the
/// charge ends at the first contact (a strike) or when the point is reached.
pub static VEX_CHARGE: Module = Module { name: "vex_charge", tick: Some(charge_tick), flags: MOVE, arm: arm_vex_charge, fired: charge_fired, can_use: charge_can_use, start: charge_start, stop: charge_stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn arm_vex_charge(w: &mut World, e: Entity, _: Inst) {
    let wait = with_rng(w, e, |r| idle_wait(r, recalc_after(7) * 2));
    arm_module(w, e, &VEX_CHARGE, wait.max(2));
}
fn charge_ok(w: &World, e: Entity) -> bool {
    let (Some(p), Some((_, t))) = (w.get::<Pos>(e), target_of(w, e)) else { return false };
    charge_ready(wanted_active(w, e), t.pos.x - p.0.x, t.pos.y - p.0.y, t.pos.z - p.0.z)
}
fn charge_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if charge_ok(w, e) {
        true
    } else {
        arm_vex_charge(w, e, Inst { slot: 0, nth: 0 });
        false
    }
}
fn charge_can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    charge_ok(w, e)
}
fn charge_start(w: &mut World, e: Entity, _: Inst) {
    if let Some((_, t)) = target_of(w, e) {
        crate::locomotion::fly::steer_to(w, e, [t.pos.x, t.eye, t.pos.z], 1.0);
    }
    w.entity_mut(e).insert(Charging);
}
fn charge_stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Charging>();
    arm_vex_charge(w, e, Inst { slot: 0, nth: 0 });
}
fn charge_tick(w: &mut World) {
    let es: Vec<Entity> = w.query_filtered::<Entity, With<Charging>>().iter(w).collect();
    for e in es {
        let Some((who, t)) = target_of(w, e) else { end(w, e, &VEX_CHARGE); continue };
        let (Some(b), Some(p)) = (w.get::<Bounds>(e).map(|b| b.0), w.get::<Pos>(e).map(|p| p.0)) else { continue };
        if b.intersects(t.bounds) {
            hit(w, e, who);
            end(w, e, &VEX_CHARGE);
        } else if !wanted_active(w, e) {
            end(w, e, &VEX_CHARGE);
        } else if charge_steers(t.pos.x - p.x, t.pos.y - p.y, t.pos.z - p.z) {
            crate::locomotion::fly::steer_to(w, e, [t.pos.x, t.eye, t.pos.z], 1.0);
        }
    }
}

/// `VexRandomMoveGoal`: nothing wanted, one chance in 7 per ask: three tries at an air block within (7, 5, 7) of the origin (the anchor, or
/// where the vex is); the first one is the point, at speed 0.25. One shot: the goal ends at once.
pub static VEX_RANDOM_MOVE: Module = Module { name: "vex_random_move", flags: MOVE, arm: arm_vex_random, fired: random_fired, start: random_start, stop: random_stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn arm_vex_random(w: &mut World, e: Entity, _: Inst) {
    let wait = with_rng(w, e, |r| idle_wait(r, recalc_after(7) * 2));
    arm_module(w, e, &VEX_RANDOM_MOVE, wait.max(2));
}
fn random_fired(w: &mut World, e: Entity, i: Inst) -> bool {
    if wanted_active(w, e) {
        arm_vex_random(w, e, i);
        false
    } else {
        true
    }
}
fn random_start(w: &mut World, e: Entity, _: Inst) {
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let origin = w.get::<Anchor>(e).map_or([p.x.ifloor(), p.y.ifloor(), p.z.ifloor()], |a| a.0);
    let mw = w.resource::<Level>().mw.0;
    for _ in 0..3 {
        let c = with_rng(w, e, |r| [origin[0] + r.next_int(15) - 7, origin[1] + r.next_int(11) - 5, origin[2] + r.next_int(15) - 7]);
        if mw.facts.is_air(mw.world.block(c[0], c[1], c[2])) {
            let at = [f64::from(c[0]) + 0.5, f64::from(c[1]) + 0.5, f64::from(c[2]) + 0.5];
            crate::locomotion::fly::steer_to(w, e, at, 0.25);
            break;
        }
    }
    end(w, e, &VEX_RANDOM_MOVE);
}
fn random_stop(w: &mut World, e: Entity, i: Inst) {
    arm_vex_random(w, e, i);
}

/// `VexCopyOwnerTargetGoal` (flag TARGET): when the owner (a creature, `tame::Owner`) gets a target that is not the vex and is alive, the vex
/// takes it. By EVENT: the target the owner just got (`Events.target_acquired`).
pub static VEX_COPY_OWNER_TARGET: Module = Module { name: "vex_copy_owner_target", tick: Some(copy_owner_tick), flags: TARGET, requires: &[Cap::Targets], ..Module::DEFAULT };
fn copy_owner_target(w: &mut World) {
    let acquired: Vec<Entity> = w.resource::<Events>().target_acquired.iter().collect();
    for o in acquired {
        let Some(t) = w.get::<Target>(o).copied() else { continue };
        let vexes: Vec<(Entity, u8)> = w
            .query::<(Entity, &crate::tame::Owner, &GoalSet, Has<Target>)>()
            .iter(w)
            .filter(|(e, owner, _, has)| owner.0 == Some(Who::Entity(o)) && !has && t.who != Who::Entity(*e))
            .filter_map(|(e, _, g, _)| g.slot_of(&VEX_COPY_OWNER_TARGET).map(|s| (e, s)))
            .collect();
        for (e, slot) in vexes {
            if targeting::info(w, t.who).is_some_and(|i| !i.exempt) {
                targeting::set_target(w, e, Target::new(t.who, 0, slot));
            }
        }
    }
}

// ---- the phantom --------------------------------------------------------------------------------------------------
/// The strategy runs: the countdown to the dive.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Strategy {
    pub next_sweep: i32,
}
/// The dive runs.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Sweeping;
/// The circle around the anchor: its state (`PhantomCircleAroundAnchorGoal` fields).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Circle {
    pub angle: f32,
    pub distance: f32,
    pub height: f32,
    pub clockwise: f32,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Circling;

/// `PhantomAttackPlayerTargetGoal` (TARGET): one scan per `recalc_after(60)` asks (a deadline of 2x that): a player within (16 + width, 64, 16 +
/// width) becomes the target (it must be seen: the target's own `must_see`, with no memory, ends it the tick it is not).
pub static PHANTOM_ATTACK_PLAYER_TARGET: Module = Module { name: "phantom_attack_player_target", flags: TARGET, equip: scan_equip, arm: scan_arm, fired: scan_fired, can_use: scan_can_use, start: scan_start, stop: scan_stop, requires: &[Cap::Targets], ..Module::DEFAULT };
fn scan_equip(_: &mut World, _: Entity, _: Inst, _: &'static (dyn std::any::Any + Sync)) {}
/// The first scan comes after `recalc_after(20)` asks (`Phantom.register`), the next ones every `recalc_after(60)`.
fn scan_arm(w: &mut World, e: Entity, _: Inst) {
    arm_module(w, e, &PHANTOM_ATTACK_PLAYER_TARGET, 2 * recalc_after(20));
}
fn scan_rearm(w: &mut World, e: Entity) {
    arm_module(w, e, &PHANTOM_ATTACK_PLAYER_TARGET, 2 * recalc_after(60));
}
fn scan_pick(w: &World, e: Entity) -> Option<u32> {
    let (p, b) = (w.get::<Pos>(e)?.0, w.get::<Body>(e)?);
    let r = scan_range(f64::from(b.width));
    let mut found = None;
    w.resource::<Online>().for_near(p.x, p.z, r, |pl| {
        if found.is_none() && in_scan_box(pl.x - p.x, pl.y - p.y, pl.z - p.z, r) {
            found = Some(pl.id);
        }
    });
    found.filter(|&id| targeting::info(w, Who::Player(id)).is_some_and(|i| !i.exempt))
}
fn scan_fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if w.get::<Target>(e).is_some() {
        return false;
    }
    if scan_pick(w, e).is_some() {
        true
    } else {
        scan_rearm(w, e);
        false
    }
}
fn scan_can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    scan_pick(w, e).is_some()
}
fn scan_start(w: &mut World, e: Entity, i: Inst) {
    if let Some(id) = scan_pick(w, e) {
        let mut t = Target::new(Who::Player(id), 0, i.slot);
        t.must_see = true;
        // `PhantomAttackPlayerTargetGoal.canContinueToUse` has no follow range (the generic target goal's would end it above 16 blocks).
        t.follow_scale = 1.0e6;
        targeting::set_target(w, e, t);
    }
}
fn scan_stop(w: &mut World, e: Entity, _: Inst) {
    scan_rearm(w, e);
}

fn anchor_above_target(w: &mut World, e: Entity) {
    let Some((_, t)) = target_of(w, e) else { return };
    let sea = w.resource::<Level>().mw.0.sea_level;
    let y = anchor_y(t.pos.y.ifloor(), with_rng(w, e, |r| r.next_int(20)), sea);
    w.entity_mut(e).insert(Anchor([t.pos.x.ifloor(), y, t.pos.z.ifloor()]));
}
/// Sets or clears the dive and wakes the goals it concerns (the sweep starts, the circle starts again when the dive ends).
fn set_diving(w: &mut World, e: Entity, on: bool) {
    let Some(g) = w.get::<GoalSet>(e) else { return };
    let (sweep, circle) = (g.slot_of(&PHANTOM_SWEEP_ATTACK), g.slot_of(&PHANTOM_CIRCLE_AROUND_ANCHOR));
    if on {
        w.entity_mut(e).insert(Diving);
        if let Some(s) = sweep {
            w.resource_mut::<Requests>().0.push(Req::Start(e, s));
        }
    } else {
        w.entity_mut(e).remove::<Diving>();
        if let Some(c) = circle {
            w.resource_mut::<Requests>().0.push(Req::Start(e, c));
        }
    }
}

/// `PhantomAttackStrategyGoal` (no flag): with a target, the circle above it, then every 8 to 11 seconds a dive. Started and ended by the
/// target (events).
pub static PHANTOM_ATTACK_STRATEGY: Module = Module { name: "phantom_attack_strategy", tick: Some(strategy_tick), flags: 0, can_use: strategy_can_use, start: strategy_start, stop: strategy_stop, requires: &[Cap::Targets, Cap::Locomotion], ..Module::DEFAULT };
fn strategy_can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    target_of(w, e).is_some()
}
fn strategy_start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Strategy { next_sweep: 2 * recalc_after(10) }).remove::<Diving>();
    anchor_above_target(w, e);
}
fn strategy_stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Strategy>();
    if let Some(a) = w.get::<Anchor>(e).map(|a| a.0) {
        let top = w.resource::<Level>().mw.0.motion_blocking_height(a[0], a[2]);
        let y = drop_anchor_y(top, with_rng(w, e, |r| r.next_int(20)));
        w.entity_mut(e).insert(Anchor([a[0], y, a[2]]));
    }
}
/// A target gained starts the strategy, a target lost ends it (the events, once); the countdown to the dive.
fn strategy_tick(w: &mut World) {
    let (got, lost): (Vec<Entity>, Vec<Entity>) = {
        let ev = w.resource::<Events>();
        (ev.target_acquired.iter().collect(), ev.target_lost.iter().collect())
    };
    for e in got {
        if w.get::<Strategy>(e).is_none() && let Some(s) = w.get::<GoalSet>(e).and_then(|g| g.slot_of(&PHANTOM_ATTACK_STRATEGY)) {
            w.resource_mut::<Requests>().0.push(Req::Start(e, s));
        }
    }
    for e in lost {
        if w.get::<Strategy>(e).is_some() && let Some(s) = w.get::<GoalSet>(e).and_then(|g| g.slot_of(&PHANTOM_ATTACK_STRATEGY)) {
            w.resource_mut::<Requests>().0.push(Req::End(e, s));
        }
    }
    let es: Vec<Entity> = w.query_filtered::<Entity, (With<Strategy>, Without<Diving>)>().iter(w).collect();
    for e in es {
        let due = {
            let mut s = w.get_mut::<Strategy>(e).unwrap();
            s.next_sweep -= 1;
            s.next_sweep <= 0
        };
        if due {
            anchor_above_target(w, e);
            let next = 2 * recalc_after((8 + with_rng(w, e, |r| r.next_int(4))) * 20);
            w.get_mut::<Strategy>(e).unwrap().next_sweep = next;
            set_diving(w, e, true);
        }
    }
}

/// `PhantomSweepAttackGoal` (MOVE): the dive: the wanted point is the target's middle; a contact ends the dive with a strike; a wall or a
/// hurt ends it; the goal ends the target.
pub static PHANTOM_SWEEP_ATTACK: Module = Module { name: "phantom_sweep_attack", tick: Some(sweep_tick), flags: MOVE, can_use: sweep_can_use, start: sweep_start, stop: sweep_stop, requires: &[Cap::Targets, Cap::Locomotion], ..Module::DEFAULT };
fn sweep_can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    sweep_may_start(w.get::<Diving>(e).is_some(), w.get::<Target>(e).is_some())
}
fn sweep_start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Sweeping);
}
fn sweep_stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Sweeping>();
    targeting::clear_target(w, e);
    // The target goal that held it ends with it (the `Mob`'s `setTarget(null)`), and scans again later.
    end(w, e, &PHANTOM_ATTACK_PLAYER_TARGET);
    if w.get::<Diving>(e).is_some() {
        set_diving(w, e, false);
    }
}
fn sweep_tick(w: &mut World) {
    let es: Vec<Entity> = w.query_filtered::<Entity, With<Sweeping>>().iter(w).collect();
    for e in es {
        let Some((who, t)) = target_of(w, e) else { end(w, e, &PHANTOM_SWEEP_ATTACK); continue };
        if w.get::<Diving>(e).is_none() {
            end(w, e, &PHANTOM_SWEEP_ATTACK);
            continue;
        }
        // `canContinueToUse`: the cats near hiss every 20 ticks and a scared phantom ends the dive.
        let now = w.resource::<Clock>().of(e);
        if crate::hiss::scare(w, e, now) == Some(true) {
            end(w, e, &PHANTOM_SWEEP_ATTACK);
            continue;
        }
        let mid = sweep_mid(t.bounds.min_position().y, t.bounds.max_position().y);
        if let Some(mut m) = w.get_mut::<MoveTo>(e) {
            *m = MoveTo { active: true, x: t.pos.x, y: mid, z: t.pos.z, speed_modifier: 1.0 };
        }
        let reach = w.get::<Bounds>(e).map(|b| b.0.inflate(Vec3::new(0.2, 0.2, 0.2)));
        if reach.is_some_and(|r| r.intersects(t.bounds)) {
            hit(w, e, who);
            set_diving(w, e, false);
        } else if w.get::<Contact>(e).is_some_and(|c| c.horizontal) {
            set_diving(w, e, false);
        }
    }
}

/// `PhantomCircleAroundAnchorGoal` (MOVE): without a target or out of the dive, fly round the anchor at a radius of 5 to 15 blocks, 15
/// degrees a point; the height, the radius and the angle are drawn again by chance (1 in 350, 250, 450 ticks of the goal).
pub static PHANTOM_CIRCLE_AROUND_ANCHOR: Module = Module { name: "phantom_circle_around_anchor", tick: Some(circle_tick), flags: MOVE, equip: circle_equip, arm: circle_arm, can_use: circle_can_use, start: circle_start, stop: circle_stop, requires: &[Cap::Locomotion], ..Module::DEFAULT };
fn circle_equip(w: &mut World, e: Entity, _: Inst, _: &'static (dyn std::any::Any + Sync)) {
    // `Phantom.register`: the anchor is 5 blocks above where it appeared.
    if let Some(p) = w.get::<Pos>(e).map(|p| p.0) {
        w.entity_mut(e).insert(Anchor([p.x.ifloor(), p.y.ifloor() + 5, p.z.ifloor()]));
    }
}
fn circle_arm(w: &mut World, e: Entity, i: Inst) {
    arm(w, e, i.slot, 1);
}
fn circle_can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    circle_may_start(w.get::<Target>(e).is_some(), w.get::<Diving>(e).is_some())
}
fn select_next(w: &mut World, e: Entity) {
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let a = w.get::<Anchor>(e).map_or([p.x.ifloor(), p.y.ifloor(), p.z.ifloor()], |a| a.0);
    w.entity_mut(e).insert(Anchor(a));
    let Some(mut c) = w.get_mut::<Circle>(e) else { return };
    c.angle += c.clockwise * 15.0_f32.to_radians();
    let at = circle_point(a, c.distance, c.height, c.angle);
    if let Some(mut m) = w.get_mut::<MoveTo>(e) {
        *m = MoveTo { active: true, x: at[0], y: at[1], z: at[2], speed_modifier: 1.0 };
    }
}
fn circle_start(w: &mut World, e: Entity, _: Inst) {
    let (distance, height, side) = with_rng(w, e, |r| (5.0 + r.next_float() * 10.0, -4.0 + r.next_float() * 9.0, r.next_int(2)));
    w.entity_mut(e).insert((Circle { angle: 0.0, distance, height, clockwise: if side == 0 { 1.0 } else { -1.0 } }, Circling, crate::cow::Moving));
    select_next(w, e);
}
fn circle_stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Circling>();
}
/// Every other tick (the `Mob` ticks a goal that is not `every_tick` on the ticks where tick + id is even).
fn circle_tick(w: &mut World) {
    let es: Vec<Entity> = w.query_filtered::<Entity, With<Circling>>().iter(w).collect();
    for e in es {
        let now = crate::cow::now_of(w, e);
        let one = |w: &mut World| {
            if (now + phase_of(w, e)) % 2 != 0 {
                return;
            }
            let (Some(p), Some(mut c)) = (w.get::<Pos>(e).map(|p| p.0), w.get::<Circle>(e).copied()) else { return };
            let mut redraw = false;
            let (h, d, a) = with_rng(w, e, |r| {
                let h = (r.next_int(recalc_after(350)) == 0).then(|| -4.0 + r.next_float() * 9.0);
                let d = r.next_int(recalc_after(250)) == 0;
                let a = (r.next_int(recalc_after(450)) == 0).then(|| r.next_float() * 2.0 * std::f32::consts::PI);
                (h, d, a)
            });
            if let Some(h) = h {
                c.height = h;
            }
            if d {
                c.distance += 1.0;
                if c.distance > 15.0 {
                    c.distance = 5.0;
                    c.clockwise = -c.clockwise;
                }
            }
            if let Some(a) = a {
                c.angle = a;
                redraw = true;
            }
            *w.get_mut::<Circle>(e).unwrap() = c;
            if redraw {
                select_next(w, e);
            }
            let t = w.get::<MoveTo>(e).copied().unwrap_or(MoveTo { active: false, x: 0.0, y: 0.0, z: 0.0, speed_modifier: 1.0 });
            if circle_reached(t.x - p.x, t.y - p.y, t.z - p.z) {
                select_next(w, e);
            }
            let mw = w.resource::<Level>().mw.0;
            let air = |x: i32, y: i32, z: i32| mw.facts.get(mw.world.block(x, y, z)).name.as_ref() == "minecraft:air";
            let (x, y, z) = (p.x.ifloor(), p.y.ifloor(), p.z.ifloor());
            let t = w.get::<MoveTo>(e).copied().unwrap_or(t);
            if t.y < p.y && !air(x, y - 1, z) {
                let mut c = *w.get::<Circle>(e).unwrap();
                c.height = raised_height(c.height);
                *w.get_mut::<Circle>(e).unwrap() = c;
                select_next(w, e);
            }
            let t = w.get::<MoveTo>(e).copied().unwrap_or(t);
            if t.y > p.y && !air(x, y + 1, z) {
                let mut c = *w.get::<Circle>(e).unwrap();
                c.height = lowered_height(c.height);
                *w.get_mut::<Circle>(e).unwrap() = c;
                select_next(w, e);
            }
        };
        one(w);
    }
}

/// The vex copies its owner's target only when a target was gained this tick.
fn copy_owner_tick(w: &mut World) {
    if !w.resource::<Events>().target_acquired.is_empty() {
        copy_owner_target(w);
    }
}
