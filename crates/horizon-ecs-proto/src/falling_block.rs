//! The falling block (`FallingBlockEntity`): a block that lost its support becomes a body that falls as `Entity.move` makes it (gravity 0.04,
//! the drag 0.98 on every axis: the body of `tnt.rs`), and lands on the tick its move ends on the ground: it becomes a block again where it
//! stands when that cell can be replaced and has no free cell under it, else it breaks and drops its item. It is visited only while it falls: once
//! landed it is gone, and the block it placed rests as any block does.
//! Ported: the 600-tick limit (a deadline), the concrete powder that lands in water. Not ported: the can-survive test of the block (every block
//! survives), the powder that is hit in water by a fast fall, the damage a block that falls far does to creatures, the cobweb and bubble-column
//! pushes, the block entity's data, the build-height limit (the generic fall under the world removes an object that left it), the `cancelDrop` and `dropItem` flags (a drop is always made).
use crate::cow::*;
use crate::item::drop_item;
use crate::tnt::SIZE;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_world::registry::BlockStateId;

/// A falling block: the state it will be placed with and the item it drops when it cannot be.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct FallingBlock {
    pub state: BlockStateId,
    pub drop: Option<ItemStack>,
    /// `Entity.fallDistance` as the last tick in the air left it (the move that lands resets the body's own before `lands` reads it).
    pub fell: f64,
}

/// `FallingBlockEntity.fall`: the block at `cell` is replaced by air and a falling one starts at its centre, 0.01 above its floor (the body is
/// 0.98 high), at rest.
pub fn fall(w: &mut World, cell: [i32; 3], state: BlockStateId, drop: Option<ItemStack>) -> Entity {
    let mw = crate::blocks::world_of(w);
    if let Some(air) = crate::blocks::resolve(mw, "minecraft:air", &[]) {
        crate::blocks::set_state_id(w, cell, air);
    }
    let at = Vec3::new(f64::from(cell[0]) + 0.5, f64::from(cell[1]) + f64::from((1.0 - SIZE) / 2.0), f64::from(cell[2]) + 0.5);
    let body = Body { width: SIZE, height: SIZE, step_height: 0.0, gravity: 0.04, speed: 0.0, fall_factor: 1.0, lava_walker: false, object: true, orb: false, tnt: true };
    let e = w.spawn((Pos(at), Vel(Vec3::ZERO), OwnTime, body, Bounds(box_at(at, SIZE, SIZE)), Contact { on_ground: false, horizontal: false, fall_distance: 0.0 }, FluidState::default(), FallingBlock { state, drop, fell: 0.0 }, Moving))
.id();
    crate::nbt::mark_saved(w, e, crate::species::lists::FALLING_BLOCK);
    w.entity_mut(e).insert(crate::cow::TypeName(crate::species::lists::FALLING_BLOCK));
    after(w, e, LIMIT + 1, expire);
    e
}

/// `FallingBlockEntity.tick`: a block still in the air after 600 ticks is removed, and drops its item.
pub const LIMIT: i32 = 600;
fn expire(w: &mut World, e: Entity) {
    if w.get::<Contact>(e).is_none_or(|c| c.on_ground) {
        return;
    }
    let (Some(b), Some(at)) = (w.get::<FallingBlock>(e).cloned(), w.get::<Pos>(e).map(|p| p.0)) else { return };
    w.entity_mut(e).despawn();
    if let Some(item) = b.drop {
        drop_item(w, item, at, Vec3::ZERO, 10);
    }
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(lands.after(travel).before(reindex));
}

/// The tick's end for the blocks in the air: the ones whose move ended on the ground land.
fn lands(w: &mut World) {
    // Nothing in the air: two lookups, no query to build.
    if !crate::goals::sparse_any::<FallingBlock>(w) {
        return;
    }
    for (c, mut b) in w.query::<(&Contact, &mut FallingBlock)>().iter_mut(w) {
        if !c.on_ground {
            b.fell = c.fall_distance;
        }
    }
    let landed: Vec<(Entity, Vec3)> = w.query_filtered::<(Entity, &Pos, &Contact), With<FallingBlock>>().iter(w).filter(|x| x.2.on_ground).map(|x| (x.0, x.1 .0)).collect();
    for (e, at) in landed {
        let Some(b) = w.get::<FallingBlock>(e).cloned() else { continue };
        let cell = [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32];
        let mw = crate::blocks::world_of(w);
        let (here, below) = (mw.world.block(cell[0], cell[1], cell[2]), mw.world.block(cell[0], cell[1] - 1, cell[2]));
        let free = |s| mw.facts.is_air(s) || mw.facts.get(s).replaceable;
        let (state, drop) = anvil_hurts(w, e, &b, b.state);
        let b = FallingBlock { state, drop, ..b };
        w.entity_mut(e).despawn();
        // `ConcretePowderBlock`: powder that lands in water sets as its concrete.
        let state = match &*mw.facts.get(b.state).name {
            n if n.ends_with("_concrete_powder") && mw.facts.get(here).fluid == horizon_client_lib::blocks::Fluid::Water => {
                crate::blocks::resolve(mw, &n.replace("_powder", ""), &[]).unwrap_or(b.state)
            }
            _ => b.state,
        };
        if free(here) && !free(below) {
            crate::blocks::set_state_id(w, cell, state);
        } else if let Some(item) = b.drop {
            drop_item(w, item, at, Vec3::ZERO, 10);
        }
    }
}

/// `FallingBlockEntity.causeFallDamage` of an anvil (`AnvilBlock.falling`: 2 a block, at most 40): the creatures in its box take
/// `min(floor(ceil(fell - 1) * 2), 40)`, and the anvil is worn one step with chance `0.05 + 0.05 * ceil(fell - 1)` (an anvil, a chipped one,
/// a damaged one, then it is destroyed: nothing is placed and nothing drops). Returns the state to place and the item to drop.
fn anvil_hurts(w: &mut World, e: Entity, b: &FallingBlock, state: BlockStateId) -> (BlockStateId, Option<ItemStack>) {
    let mw = crate::blocks::world_of(w);
    let name = mw.facts.get(state).name.to_string();
    let Some(kind) = name.strip_prefix("minecraft:").and_then(|n| n.strip_suffix("anvil")) else { return (state, b.drop.clone()) };
    let steps = (b.fell - 1.0).ceil() as i32;
    if steps < 0 {
        return (state, b.drop.clone());
    }
    let amount = ((steps as f32 * 2.0).floor() as i32).min(40) as f32;
    if amount > 0.0 {
        let (area, at) = (w.get::<Bounds>(e).map(|x| x.0), w.get::<Pos>(e).map(|p| p.0));
        if let (Some(area), Some(at)) = (area, at) {
            let hit: Vec<Entity> = w.query_filtered::<(Entity, &Bounds), With<Health>>().iter(w).filter(|x| x.1 .0.intersects(area)).map(|x| x.0).collect();
            for c in hit {
                w.resource_mut::<DamageQueue>().add_from(c, amount, Cause::FallingBlock, (at.x, at.z));
            }
        }
        crate::projectile_more::ensure_rng(w);
        let worn = w.resource_mut::<crate::projectile_more::ThrownRng>().0.next_float() < 0.05 + steps as f32 * 0.05;
        if worn {
            let next = match kind {
                "" => Some("minecraft:chipped_anvil"),
                "chipped_" => Some("minecraft:damaged_anvil"),
                _ => None,
            };
            return match next.and_then(|n| crate::blocks::resolve(mw, n, &[])) {
                Some(s) => (s, b.drop.clone()),
                None => (crate::blocks::resolve(mw, "minecraft:air", &[]).unwrap_or(state), None),
            };
        }
    }
    (state, b.drop.clone())
}
