//! Level reads for the block-breaking and trampling modules. Blocks come from the Mob's `MobWorld` (`blocks::world_of`), the
//! flags from `Level` and `goals::mob_griefing`. A write is an event (`blocks::set_state`), applied at the next step.
use crate::IFloor as _;
use crate::blocks::world_of;
use crate::cow::{Bounds, Contact, Level, Pos, Vel};
use crate::goals::mob_griefing;
use crate::rules::level::{full_face_of, JUMP_POWER};
use bevy_ecs::prelude::*;
use myriade_level::collisions::projectile_can_stand_on;
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
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let mw = w.resource::<Level>().mw.0;
    // `Entity.getBlockJumpFactor`: the block at the feet, else the one 0.500001 below when the feet's is 1.
    let feet = crate::block_factor::jump(mw, [p.x, p.y, p.z]);
    let factor = if feet == 1.0 { crate::block_factor::jump(mw, [p.x, p.y - 0.500001, p.z]) } else { feet };
    // `LivingEntity.getJumpPower`: 0.42 x the block factor, plus 0.1 x (level + 1) of jump boost (0 without the effect).
    let boost = w
        .get::<crate::effects::Effects>(e)
        .and_then(|f| f.list.iter().find(|x| x.kind == crate::effects::Kind::JumpBoost))
        .map_or(0.0, |x| f64::from(0.1_f32 * (x.amp + 1) as f32));
    let power = JUMP_POWER * f64::from(factor) + boost;
    // `jumpFromGround`: nothing when the power is not above 1e-5; else the vertical speed is the larger of the power and the current one.
    if power <= 1e-5 {
        return;
    }
    if let Some(mut v) = w.get_mut::<Vel>(e) {
        v.0.y = power.max(v.0.y);
    }
}
/// The bounce restitution the block at `p` gives (`Block.getBounceRestitution`, from the data): 0 outside the loaded blocks.
pub fn bounce_restitution(w: &World, p: BlockPos) -> f32 {
    let mw = w.resource::<Level>().mw.0;
    mw.movement.get(mw.world.block(p.x, p.y, p.z)).map_or(0.0, |m| m.coefficients.bounce_restitution)
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
/// `Level.destroyBlock(p, drop, breaker)`, done by the host (`HostEvent::BlockBreak`), which still reads the block broken. While a mod
/// hears `LivingDestroyBlockEvent`, a creature's break is asked first (`MiscEvent::DestroyAsk`): the host breaks unless a mod cancels.
pub fn break_block(w: &mut World, p: [i32; 3], drop: bool, _by: Option<Entity>, breaker: Option<Entity>) {
    if let Some(breaker) = breaker
        && asks_living_destroy_block(w)
    {
        w.get_resource_or_insert_with(crate::colony_register::MiscOut::default)
            .0
            .push(crate::host_out_misc::MiscEvent::DestroyAsk { breaker, pos: p, drop });
        return;
    }
    if let Some(mut ev) = w.get_resource_mut::<crate::goals::Events>() {
        ev.host.block_break(p, drop, breaker);
    }
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
/// Inside the vanilla world border (`MobWorld.border`).
pub fn within_border(w: &World, p: [i32; 3]) -> bool {
    // `WorldBorder.isWithinBounds(x, z)`: `min <= v < max` on each axis, no border meaning the whole world.
    w.resource::<Level>().mw.0.border.is_none_or(|b| {
        let [min_x, min_z, max_x, max_z] = b.bounds(0.0);
        let (x, z) = (f64::from(p[0]), f64::from(p[2]));
        x >= min_x && x < max_x && z >= min_z && z < max_z
    })
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
