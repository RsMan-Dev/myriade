//! The breeze's four own behaviours (`BreezeAi`, `monster_brain.rs`: `Shoot`, `ShootWhenStuck`, `Slide`, `LongJump`) and its slide to the
//! walk target (the `move_to_target_sink` with a pose and a shot at its end). Each is a behaviour of the brain; the conditions of memory are
//! the entries' `needs` (`species/breeze.rs`). The pose (`Pose`) is a component the four read: a shot or a slide starts only standing.
//! Why the long jump is not `behaviours/long_jump` of the goals: that one is the frog's and the goat's (a goal, a prepared 40-tick look, a box of
//! cells to pick from, the angles 65 to 80); the breeze's picks a point behind its target, inhales 10 ticks, and tries the angles 40 to 80,
//! with no collision test of the arc. The arc arithmetic (`LongJumpUtil`) is the service `jump_arc`, shared with that goal.
//! Not ported: the sounds, the ground particles, the jump boost and levitation effects, the mount, the target's gaze-free yaw of a player
//! (taken from his view vector), the pose the slide sink gives while it runs (animation only).
//! The calculations are in `rules::breeze`.
use crate::behaviours::Cap;
use crate::brain::sensors::attackable::NEAREST_ATTACKABLE;
use crate::brain::*;
use crate::goals::with_rng;
use crate::projectile::{launch, Flight};
use crate::rules::breeze::{self as rules};
use crate::targeting::{self, FollowRange, Who};
use horizon_core::geometry::Vec3;

pub static SHOOT_MEM: MemKey = MemKey::new("minecraft:breeze_shoot", Shape::Unit);
pub static SHOOT_COOLDOWN: MemKey = MemKey::new("minecraft:breeze_shoot_cooldown", Shape::Unit);
pub static SHOOT_CHARGING: MemKey = MemKey::new("minecraft:breeze_shoot_charging", Shape::Unit);
pub static SHOOT_RECOVERING: MemKey = MemKey::new("minecraft:breeze_shoot_recovering", Shape::Unit);
pub static JUMP_COOLDOWN: MemKey = MemKey::new("minecraft:breeze_jump_cooldown", Shape::Unit);
pub static JUMP_INHALING: MemKey = MemKey::new("minecraft:breeze_jump_inhaling", Shape::Unit);
pub static JUMP_TARGET: MemKey = MemKey::new("minecraft:breeze_jump_target", Shape::Block);
pub static LEAVING_WATER: MemKey = MemKey::new("minecraft:breeze_leaving_water", Shape::Unit);

/// `Breeze`'s pose (`variant3`): what its animations and the start conditions of its behaviours read.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pose {
    Standing,
    Shooting,
    Inhaling,
    LongJumping,
    Sliding,
}
fn pose(w: &World, e: Entity) -> Pose {
    w.get::<Pose>(e).copied().unwrap_or(Pose::Standing)
}
fn set_pose(w: &mut World, e: Entity, p: Pose) {
    w.entity_mut(e).insert(p);
}
fn equip(w: &mut World, e: Entity, _: &Ctx) {
    if w.get::<Pose>(e).is_none() {
        w.entity_mut(e).insert(Pose::Standing);
    }
}
fn target(w: &World, e: Entity) -> Option<Who> {
    match get(w, e, &ATTACK_TARGET) {
        Some(MemValue::Who(who)) => Some(*who),
        _ => None,
    }
}
fn pos(w: &World, e: Entity) -> Vec3 {
    w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0)
}
fn body(w: &World, e: Entity) -> Body {
    *w.get::<Body>(e).expect("a body")
}
fn in_water(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| f.in_water)
}
fn on_ground(w: &World, e: Entity) -> bool {
    w.get::<Contact>(e).is_some_and(|c| c.on_ground)
}
/// `Breeze.getFiringYPosition`.
fn firing_y(w: &World, e: Entity) -> f64 {
    rules::firing_y(pos(w, e).y, body(w, e).height)
}
/// `BreezeUtil.hasLineOfSight`.
fn line_of_sight(w: &World, e: Entity, to: Vec3) -> bool {
    let range = rules::los_range(w.get::<FollowRange>(e).map_or(24.0, |f| f.0));
    let from = pos(w, e);
    to.distance(from) <= range && w.resource::<Level>().mw.0.clear_between(from, to, false)
}
/// The target's yaw for `randomPointBehindTarget`: a creature's, a player's from his view vector (south is 0).
fn yaw_of(w: &World, who: Who) -> f64 {
    match who {
        Who::Entity(t) => w.get::<Yaw>(t).map_or(0.0, |y| f64::from(y.0)),
        Who::Player(id) => {
            let g = w.resource::<crate::hostile::PlayerState>().gaze(id);
            (-g[0]).atan2(g[2]).to_degrees()
        }
    }
}
/// `BreezeUtil.randomPointBehindTarget`: the draws are taken in their order (two uniforms, then the float).
fn point_behind(w: &mut World, e: Entity, who: Who) -> Option<Vec3> {
    let i = targeting::info(w, who)?;
    let yaw = yaw_of(w, who);
    let (u, v, f) = with_rng(w, e, |r| (r.next_f64(), r.next_f64(), r.next_float()));
    Some(rules::behind_point(i.pos, yaw, u, v, f))
}

// ---- Shoot ------------------------------------------------------------------------------------------------------------
pub static SHOOT: Behaviour = Behaviour {
    name: "minecraft:breeze_shoot",
    mems: &[&ATTACK_TARGET, &SHOOT_MEM, &SHOOT_COOLDOWN, &SHOOT_CHARGING, &SHOOT_RECOVERING, &WALK_TARGET, &JUMP_TARGET],
    requires: &[Cap::Brain, Cap::Body, Cap::Targets],
    equip,
    can_start: shoot_can_start,
    start: shoot_start,
    tick: shoot_tick,
    stop: shoot_stop,
    ..Behaviour::DEFAULT
};
fn shoot_can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if pose(w, e) != Pose::Standing {
        return false;
    }
    let Some(t) = target(w, e) else { return false };
    let within = targeting::info(w, t).is_some_and(|i| pos(w, e).distance_squared(i.pos) < 256.0);
    if !within {
        erase(w, e, &SHOOT_MEM);
        return false;
    }
    true
}
fn shoot_start(w: &mut World, e: Entity, _: &Ctx) {
    set_pose(w, e, Pose::Shooting);
    set(w, e, &SHOOT_CHARGING, MemValue::Unit, Some(15));
}
fn shoot_tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    let Some(t) = target(w, e) else { return false };
    if !present(w, e, &SHOOT_MEM) {
        return false;
    }
    let Some(i) = targeting::info(w, t) else { return true };
    look_at(w, e, [i.pos.x, i.pos.y, i.pos.z]);
    if present(w, e, &SHOOT_CHARGING) || present(w, e, &SHOOT_RECOVERING) {
        return true;
    }
    set(w, e, &SHOOT_RECOVERING, MemValue::Unit, Some(4));
    let firing = firing_y(w, e);
    let p = pos(w, e);
    let height = i.bounds.max_position().y - i.bounds.min_position().y;
    let d = rules::aim(i.pos, height, firing, p);
    let draws: [f64; 6] = with_rng(w, e, |r| std::array::from_fn(|_| r.next_f64()));
    launch(w, Flight::BREEZE_WIND_CHARGE, Vec3::new(p.x, firing, p.z), rules::wind_shot(d, draws), Some(Who::Entity(e)));
    true
}
fn shoot_stop(w: &mut World, e: Entity, _: &Ctx) {
    if pose(w, e) == Pose::Shooting {
        set_pose(w, e, Pose::Standing);
    }
    set(w, e, &SHOOT_COOLDOWN, MemValue::Unit, Some(10));
    erase(w, e, &SHOOT_MEM);
}

// ---- ShootWhenStuck ---------------------------------------------------------------------------------------------------
pub static SHOOT_WHEN_STUCK: Behaviour = Behaviour {
    name: "minecraft:breeze_shoot_when_stuck",
    mems: &[&ATTACK_TARGET, &JUMP_INHALING, &JUMP_TARGET, &WALK_TARGET, &SHOOT_MEM],
    requires: &[Cap::Brain, Cap::Body],
    one_shot: true,
    equip,
    can_start: |w, e, _| in_water(w, e),
    start: |w, e, _| set(w, e, &SHOOT_MEM, MemValue::Unit, Some(60)),
    ..Behaviour::DEFAULT
};

// ---- Slide ------------------------------------------------------------------------------------------------------------
pub static SLIDE: Behaviour = Behaviour {
    name: "minecraft:breeze_slide",
    mems: &[&ATTACK_TARGET, &WALK_TARGET, &JUMP_COOLDOWN, &SHOOT_MEM],
    requires: &[Cap::Brain, Cap::Body, Cap::Goals, Cap::Locomotion],
    one_shot: true,
    equip,
    can_start: |w, e, _| on_ground(w, e) && !in_water(w, e) && pose(w, e) == Pose::Standing,
    start: slide_start,
    ..Behaviour::DEFAULT
};
/// `Slide.start`: away from the target when it is within 4 blocks, else behind it or on the circle of 8 to 4 blocks around it.
fn slide_start(w: &mut World, e: Entity, _: &Ctx) {
    let Some(t) = target(w, e) else { return };
    let Some(i) = targeting::info(w, t) else { return };
    let p = pos(w, e);
    let centre = Vec3::new(p.x.floor() + 0.5, p.y.floor() + 0.5, p.z.floor() + 0.5);
    let (dx, dy, dz) = (i.pos.x - centre.x, i.pos.y - centre.y, i.pos.z - centre.z);
    let mut chosen: Option<Vec3> = None;
    if rules::near_for_away(dx, dy, dz)
        && let Some(a) = pick(w, e, crate::spots::Pick::DefaultAway { h: 5, v: 5, from: [i.pos.x, i.pos.y, i.pos.z] })
    {
        let away = Vec3::new(a[0], a[1], a[2]);
        if line_of_sight(w, e, away) && rules::is_further(i.pos, away, p) {
            chosen = Some(away);
        }
    }
    let at = match chosen {
        Some(v) => v,
        None => {
            if with_rng(w, e, |r| r.next_u32() & 1 == 0) {
                let Some(v) = point_behind(w, e, t) else { return };
                v
            } else {
                let a = with_rng(w, e, |r| r.next_f64());
                rules::middle_circle(p, i.pos, a)
            }
        }
    };
    let b = rules::block_of(at);
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Block(b), speed: 0.6, close: 1 }), None);
}

/// `SlideToTargetSink`: the plain sink, and at its end the pose is standing again and, with a target, a shot is wanted for 60 ticks.
pub static SLIDE_SINK: Behaviour = Behaviour {
    name: "minecraft:breeze_slide_to_target_sink",
    mems: &[&WALK_TARGET, &PATH, &CANT_REACH_WALK_TARGET_SINCE, &ATTACK_TARGET, &SHOOT_MEM],
    requires: &[Cap::Brain, Cap::Body, Cap::Goals, Cap::Locomotion],
    passive: crate::brain::behaviours::move_to_target_sink::BEHAVIOUR.passive,
    equip: |w, e, c| {
        equip(w, e, c);
        (crate::brain::behaviours::move_to_target_sink::BEHAVIOUR.equip)(w, e, c);
    },
    can_start: |w, e, c| (crate::brain::behaviours::move_to_target_sink::BEHAVIOUR.can_start)(w, e, c),
    start: |w, e, c| {
        set_pose(w, e, Pose::Sliding);
        (crate::brain::behaviours::move_to_target_sink::BEHAVIOUR.start)(w, e, c);
    },
    tick: |w, e, c| (crate::brain::behaviours::move_to_target_sink::BEHAVIOUR.tick)(w, e, c),
    stop: |w, e, c| {
        (crate::brain::behaviours::move_to_target_sink::BEHAVIOUR.stop)(w, e, c);
        set_pose(w, e, Pose::Standing);
        if present(w, e, &ATTACK_TARGET) {
            set(w, e, &SHOOT_MEM, MemValue::Unit, Some(60));
        }
    },
    ..Behaviour::DEFAULT
};

// ---- LongJump ---------------------------------------------------------------------------------------------------------
pub static LONG_JUMP: Behaviour = Behaviour {
    name: "minecraft:breeze_long_jump",
    mems: &[&ATTACK_TARGET, &JUMP_COOLDOWN, &SHOOT_MEM, &WALK_TARGET, &JUMP_TARGET, &JUMP_INHALING, &LEAVING_WATER, &HURT_BY, &NEAREST_ATTACKABLE],
    requires: &[Cap::Brain, Cap::Body, Cap::Goals, Cap::Locomotion, Cap::Targets],
    equip,
    can_start: jump_can_run,
    start: jump_start,
    tick: jump_tick,
    stop: jump_stop,
    ..Behaviour::DEFAULT
};
/// `LongJump.canJumpFromCurrentPosition` at the entity's block.
fn can_jump_from_here(w: &World, e: Entity) -> bool {
    rules::can_jump_from(w.resource::<Level>().mw.0, rules::block_of(pos(w, e)))
}
fn floats(w: &World, e: Entity) -> bool {
    w.get::<FluidState>(e).is_some_and(|f| rules::floats(f.water, f.in_lava))
}
/// `LongJump.canRun` (the conditions of memory are the entry's `needs`).
fn jump_can_run(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if !on_ground(w, e) && !in_water(w, e) {
        return false;
    }
    if floats(w, e) {
        return false;
    }
    if present(w, e, &JUMP_TARGET) {
        return true;
    }
    let Some(t) = target(w, e) else { return false };
    let Some(i) = targeting::info(w, t) else { return false };
    let follow = w.get::<FollowRange>(e).map_or(24.0, |f| f.0);
    let p = pos(w, e);
    if rules::out_of_follow(i.pos, p, follow) {
        erase(w, e, &ATTACK_TARGET);
        targeting::clear_target(w, e);
        return false;
    }
    if rules::too_close(i.pos, p) {
        return false;
    }
    if !can_jump_from_here(w, e) {
        return false;
    }
    let Some(behind) = point_behind(w, e, t) else { return false };
    let mw = w.resource::<Level>().mw.0;
    let Some(spot) = rules::snap_to_surface(mw, behind) else { return false };
    let below = mw.block_name(spot[0], spot[1] - 1, spot[2]);
    if rules::dangerous(below) {
        return false;
    }
    let centre = Vec3::new(f64::from(spot[0]) + 0.5, f64::from(spot[1]) + 0.5, f64::from(spot[2]) + 0.5);
    if !line_of_sight(w, e, centre) && !line_of_sight(w, e, centre.add_vector(Vec3::new(0.0, 4.0, 0.0))) {
        return false;
    }
    set(w, e, &JUMP_TARGET, MemValue::Block(spot), None);
    true
}
fn jump_start(w: &mut World, e: Entity, _: &Ctx) {
    if !present(w, e, &JUMP_INHALING) {
        set(w, e, &JUMP_INHALING, MemValue::Unit, Some(10));
    }
    set_pose(w, e, Pose::Inhaling);
    if let Some(MemValue::Block(b)) = get(w, e, &JUMP_TARGET) {
        let b = *b;
        look_at(w, e, [f64::from(b[0]) + 0.5, f64::from(b[1]) + 0.5, f64::from(b[2]) + 0.5]);
    }
}
/// `LongJump.calculateOptimalJumpVector`: the angles shuffled, the first that reaches the target (the breeze's call passes no collision test).
fn optimal_jump_vector(w: &mut World, e: Entity, target: Vec3) -> Option<Vec3> {
    let angles = crate::jump_arc::shuffle(rules::ALLOWED_ANGLES, |i| with_rng(w, e, |r| r.next_int(i)));
    let max = rules::jump_max_velocity(w.get::<FollowRange>(e).map_or(24.0, |f| f.0));
    crate::jump_arc::first_reaching(&angles, pos(w, e), w.get::<Gravity>(e)?.0, target, max, None)
}
/// `LongJump.tick`, and `canStillUse` (not standing, and the jump's cooldown not started).
fn jump_tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if pose(w, e) == Pose::Standing || present(w, e, &JUMP_COOLDOWN) {
        return false;
    }
    let water = in_water(w, e);
    if !water && present(w, e, &LEAVING_WATER) {
        erase(w, e, &LEAVING_WATER);
    }
    if !present(w, e, &JUMP_INHALING) && pose(w, e) == Pose::Inhaling {
        let spot = match get(w, e, &JUMP_TARGET) {
            Some(MemValue::Block(b)) => Some(*b),
            _ => None,
        };
        let v = spot.and_then(|t| optimal_jump_vector(w, e, Vec3::new(f64::from(t[0]) + 0.5, f64::from(t[1]), f64::from(t[2]) + 0.5)));
        let Some(v) = v else {
            set_pose(w, e, Pose::Standing);
            return false;
        };
        if water {
            set(w, e, &LEAVING_WATER, MemValue::Unit, None);
        }
        set_pose(w, e, Pose::LongJumping);
        if let Some(mut d) = w.get_mut::<Drive>(e) {
            d.discard_friction = true;
        }
        crate::goals::set_velocity(w, e, v);
        return true;
    }
    let landed_in_water = water && !present(w, e, &LEAVING_WATER);
    if pose(w, e) == Pose::LongJumping && (on_ground(w, e) || landed_in_water) {
        set_pose(w, e, Pose::Standing);
        let hurt = present(w, e, &HURT_BY);
        set(w, e, &JUMP_COOLDOWN, MemValue::Unit, Some(rules::landing_cooldown(hurt)));
        set(w, e, &SHOOT_MEM, MemValue::Unit, Some(100));
    }
    true
}
fn jump_stop(w: &mut World, e: Entity, _: &Ctx) {
    if matches!(pose(w, e), Pose::LongJumping | Pose::Inhaling) {
        set_pose(w, e, Pose::Standing);
    }
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.discard_friction = false;
    }
    erase(w, e, &JUMP_TARGET);
    erase(w, e, &JUMP_INHALING);
    erase(w, e, &LEAVING_WATER);
}
