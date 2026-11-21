//! Level reads for the block-breaking and trampling modules. Blocks come from the Mob's `MobWorld` (`blocks::world_of`), the
//! flags from `Level` and `goals::mob_griefing`. A write is an event (`blocks::set_state`), applied at the next step.
use crate::IFloor as _;
use crate::blocks::{set_state, world_of};
use crate::cow::{Bounds, Contact, Level, Vel};
use crate::goals::mob_griefing;
use crate::rules::level::{full_face_of, jump_speed};
use bevy_ecs::prelude::*;
use horizon_client_render::collisions::projectile_can_stand_on;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::position::BlockPos;

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
/// `IBlockExtension.canEntityDestroy`: `vanilla`, the creature's own rule, unless the host's block parts answer otherwise (`MobWorld::mob_hooks`,
/// absent while no mod listens).
pub fn can_entity_destroy(w: &World, e: Entity, p: [i32; 3], vanilla: bool) -> bool {
    match crate::bridge::ids::id_of(w, e) {
        Some(id) => world_of(w).can_entity_destroy(id, p, vanilla),
        None => vanilla,
    }
}
/// The cells of the entity's box grown by `inflate` on every side (`floor` of the low and high corners, both included).
pub fn cells_in(w: &World, e: Entity, inflate: f64) -> Vec<[i32; 3]> {
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0.inflate(Vec3::new(inflate, inflate, inflate))) else {
        return Vec::new();
    };
    let (lo, hi) = (b.min_position(), b.max_position());
    let mut cells = Vec::new();
    for x in lo.x.ifloor()..=hi.x.ifloor() {
        for y in lo.y.ifloor()..=hi.y.ifloor() {
            for z in lo.z.ifloor()..=hi.z.ifloor() {
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
        v.0.y = jump_speed(v.0.y);
    }
}
/// The cells a creature asked the host to break, whose answer comes with the host's writes: `kept_check` jumps if none became air.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct AskedBreaks(pub Vec<[i32; 3]>);
/// The jump after a break (`IfBroke`): the host answers each asked block by writing the air or not, so the jump is decided two
/// ticks later (`kept_check`) from what the cells still hold, once per bump and not by tick.
pub fn jump_if_kept(w: &mut World, e: Entity, cells: Vec<[i32; 3]>) {
    if cells.is_empty() {
        return;
    }
    w.entity_mut(e).insert(AskedBreaks(cells));
    crate::cow::after(w, e, 2, kept_check);
}
fn kept_check(w: &mut World, e: Entity) {
    let Some(AskedBreaks(cells)) = w.entity_mut(e).take::<AskedBreaks>() else { return };
    if cells.iter().all(|p| !block_empty(w, *p)) && is_on_ground(w, e) {
        jump(w, e);
    }
}
/// Asks for the block at `p` to become air. The drop and the names of the breaker are not sent: no output for them yet. While a mod hears
/// `LivingDestroyBlockEvent`, a creature's break goes to the host (`MiscEvent::DestroyAsk`), which writes the air unless a mod cancels.
pub fn break_block(w: &mut World, p: [i32; 3], _drop: bool, _by: Option<Entity>, breaker: Option<Entity>) {
    if let Some(breaker) = breaker
        && asks_living_destroy_block(w)
    {
        w.get_resource_or_insert_with(crate::colony_register::MiscOut::default)
            .0
            .push(crate::host_out_misc::MiscEvent::DestroyAsk { breaker, pos: p });
        return;
    }
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
    full_face_of(b, face)
}
