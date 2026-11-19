//! Level reads for the block-breaking and trampling modules. Blocks come from the Mob's `MobWorld` (`blocks::world_of`), the
//! flags from `Level` and `goals::mob_griefing`. A write is an event (`blocks::set_state`), applied at the next step.
use crate::blocks::{set_state, world_of};
use crate::cow::{Bounds, Contact, Level, Vel};
use crate::goals::mob_griefing;
use bevy_ecs::prelude::*;
use horizon_client_render::collisions::projectile_can_stand_on;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::position::BlockPos;

/// `LivingEntity.getJumpPower` with the default jump strength and no boost.
const JUMP_POWER: f64 = 0.42;

pub fn block_name(w: &World, p: [i32; 3]) -> &'static str {
    world_of(w).block_name(p[0], p[1], p[2])
}
pub fn has_tag(w: &World, tag: &str, name: &str) -> bool {
    world_of(w).has_tag(tag, name)
}
pub fn has_block_tag(w: &World, tag: &str, p: [i32; 3]) -> bool {
    has_tag(w, tag, block_name(w, p))
}
pub fn block_has_class(w: &World, p: [i32; 3], classes: &[&str]) -> bool {
    let mw = world_of(w);
    let facts = mw.facts.get(mw.world.block(p[0], p[1], p[2]));
    classes.iter().any(|c| facts.has_class(c))
}
pub fn is_liquid(w: &World, p: [i32; 3]) -> bool {
    block_has_class(w, p, &["LiquidBlock"])
}
pub fn can_grief(w: &World, _e: Entity) -> bool {
    mob_griefing(w)
}
/// `IBlockExtension.canEntityDestroy` without a per-entity rule: the vanilla answer.
pub fn can_entity_destroy(_w: &World, _e: Entity, _p: [i32; 3], vanilla: bool) -> bool {
    vanilla
}
/// The cells of the entity's box grown by `inflate` on every side (`floor` of the low and high corners, both included).
pub fn cells_in(w: &World, e: Entity, inflate: f64) -> Vec<[i32; 3]> {
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0.inflate(Vec3::new(inflate, inflate, inflate))) else {
        return Vec::new();
    };
    let (lo, hi) = (b.min_position(), b.max_position());
    let mut cells = Vec::new();
    for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
        for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
            for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                cells.push([x, y, z]);
            }
        }
    }
    cells
}
pub fn is_on_ground(w: &World, e: Entity) -> bool {
    w.get::<Contact>(e).is_some_and(|c| c.on_ground)
}
pub fn jump(w: &mut World, e: Entity) {
    if let Some(mut v) = w.get_mut::<Vel>(e) {
        v.0.y = JUMP_POWER.max(v.0.y);
    }
}
/// The jump after a break waits for the host's answer on each block, which `HostOut` does not give yet: no jump here.
pub fn jump_if_kept(_w: &mut World, _e: Entity) {}
/// Asks for the block at `p` to become air. The drop and the names of the breaker are not sent: no output for them yet.
pub fn break_block(w: &mut World, p: [i32; 3], _drop: bool, _by: Option<Entity>, _breaker: Option<Entity>) {
    set_state(w, p, "minecraft:air", &[]);
}
pub fn asks_living_destroy_block(w: &World) -> bool {
    world_of(w).asks_living_destroy_block
}
/// No collision shape at `p` (`projectile.block_empty`).
pub fn block_empty(w: &World, p: [i32; 3]) -> bool {
    world_of(w).collision_bounds(p).is_none()
}
/// A projectile at height `entity_y` stands on the block at `p` (`projectile.can_stand_on`).
pub fn can_stand_on(w: &World, p: [i32; 3], entity_y: f64) -> bool {
    let mw = world_of(w);
    projectile_can_stand_on(mw.world, mw.collision, BlockPos::new(p[0], p[1], p[2]), entity_y)
}
pub fn block_is_air(w: &World, p: [i32; 3]) -> bool {
    let name = block_name(w, p);
    name == "minecraft:air" || name.ends_with("_air")
}
pub fn block_is_moving_piston(w: &World, p: [i32; 3]) -> bool {
    block_name(w, p) == "minecraft:moving_piston"
}
/// Inside the walls, which sit at |x| or |z| = `Level.half`.
pub fn within_border(w: &World, p: [i32; 3]) -> bool {
    let half = w.resource::<Level>().half;
    p[0].abs() < half && p[2].abs() < half
}
/// No collision shape of the level in the box.
pub fn box_free_of_blocks(w: &World, _e: Entity, b: Aabb) -> bool {
    w.resource::<Level>().no_collision(b)
}
/// No other entity with bounds overlaps the box.
pub fn box_free_of_crowd(w: &World, e: Entity, b: Aabb) -> bool {
    !w.iter_entities().any(|r| r.id() != e && r.get::<Bounds>().is_some_and(|o| o.0.intersects(b)))
}
/// The block at `p` covers the face `face` (down, up, north, south, west, east: even faces are the low side). The
/// collision bounding box stands for the shape: an approximation of `isFaceSturdy`.
pub fn full_face(w: &World, p: [i32; 3], face: u8) -> bool {
    let Some(b) = world_of(w).collision_bounds(p) else { return false };
    let (lo, hi) = (b.min_position(), b.max_position());
    let (lo, hi) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
    let axis = match face {
        0 | 1 => 1,
        2 | 3 => 2,
        _ => 0,
    };
    let touches = if face & 1 == 1 { hi[axis] >= 1.0 } else { lo[axis] <= 0.0 };
    touches && (0..3).filter(|&a| a != axis).all(|a| lo[a] <= 0.0 && hi[a] >= 1.0)
}
