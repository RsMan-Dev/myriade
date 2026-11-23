//! `LivingEntity.onClimbable` and `handleOnClimbable`: a body on a `#minecraft:climbable` block (vines, ladders, scaffolding) or an open
//! trapdoor over a ladder is held to 0.15 a tick and rises by 0.2 against a wall. The equines and the flying monsters never are: their rule
//! poses `NoClimb`.
use crate::blocks::world_of;
use crate::cow::{Drive, Pos};
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

/// `AbstractHorse.onClimbable`, `Ghast.onClimbable`, `Phantom.onClimbable`, `HappyGhast.onClimbable`: a plain false.
#[derive(Component, Clone, Copy, Debug)]
pub struct NoClimb;

/// The rule that excludes the species from climbing (`NoClimb`; no params).
pub static NO_CLIMB: crate::rules::Rule = crate::rules::Rule {
    name: "no_climb",
    equip: |w, e, _| {
        w.entity_mut(e).insert(NoClimb);
        if let Some(mut d) = w.get_mut::<crate::cow::Drive>(e) {
            d.no_climb = true;
        }
    },
    attrs: crate::rules::no_attrs,
    interact: crate::rules::no_interact,
};

/// `handleOnClimbable`: the move across at most 0.15 each way, the fall at most 0.15 (a NaN fall is kept, as `Mth.clamp` and `Math.max` keep it).
pub fn velocity(v: Vec3) -> Vec3 {
    let limit = f64::from(0.15_f32);
    let y = if v.y.is_nan() { v.y } else { v.y.max(-limit) };
    Vec3::new(v.x.clamp(-limit, limit), y, v.z.clamp(-limit, limit))
}

/// The climb of `handleRelativeFrictionAndCalculateMovement`: on a climbable and pressed against a wall (or jumping), the body rises at 0.2.
pub fn climb_lift(climbable: bool, horizontal: bool, jumping: bool) -> bool {
    climbable && (horizontal || jumping)
}

/// `IBlockExtension.isLadder`, vanilla's part: the block at the position is `#minecraft:climbable`, or it is an open trapdoor over a ladder
/// facing the same way (`makesOpenTrapdoorAboveClimbable`). The mods' parts are not read here.
pub fn block_climbable(mw: &MobWorld, p: Vec3) -> bool {
    let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let facts = mw.facts.get(mw.world.block(x, y, z));
    if mw.has_tag("minecraft:climbable", &facts.name) {
        return true;
    }
    facts.has_class("TrapDoorBlock")
        && mw.prop(x, y, z, "open") == Some("true")
        && mw.block_name(x, y - 1, z) == "minecraft:ladder"
        && mw.prop(x, y - 1, z, "facing") == mw.prop(x, y, z, "facing")
}

/// `LivingEntity.onClimbable`'s block test: the block at the body is `#minecraft:can_glide_through` (read only by a gliding body).
pub fn glides_through(mw: &MobWorld, p: Vec3) -> bool {
    let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let facts = mw.facts.get(mw.world.block(x, y, z));
    mw.has_tag("minecraft:can_glide_through", &facts.name)
}

/// `LivingEntity.onClimbable`'s decision: a gliding body in a `can_glide_through` block is not on a climbable (the test runs first in vanilla).
pub fn climbs(climbable: bool, excluded: bool, gliding: bool, glides_through: bool) -> bool {
    climbable && !excluded && !(gliding && glides_through)
}

/// `LivingEntity.onClimbable`: a wall climber answers by its flag (`Spider.onClimbable`); an excluded body never; the others by the block.
pub fn on_climbable(mw: &MobWorld, p: Vec3, climber: bool, climbing: bool, excluded: bool, gliding: bool) -> bool {
    if climber {
        climbing
    } else {
        climbs(block_climbable(mw, p), excluded, gliding, gliding && glides_through(mw, p))
    }
}

/// `LivingEntity.isPushable`'s veto: a living body on a climbable is not pushed. Read only when a push asks; a body with no `Drive` (an object) is not.
pub fn on_climbable_entity(w: &World, e: Entity) -> bool {
    match (w.get::<Pos>(e), w.get::<Drive>(e)) {
        (Some(p), Some(d)) => on_climbable(world_of(w), p.0, d.climber, d.climbing, d.no_climb, d.fall_flying),
        _ => false,
    }
}
