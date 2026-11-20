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
    let e = w.spawn((Pos(at), Vel(Vec3::ZERO), OwnTime, body, Bounds(box_at(at, SIZE, SIZE)), Contact { on_ground: false, horizontal: false, fall_distance: 0.0 }, FluidState::default(), FallingBlock { state, drop }, Moving)).id();
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
    let landed: Vec<(Entity, Vec3)> = w.query_filtered::<(Entity, &Pos, &Contact), With<FallingBlock>>().iter(w).filter(|x| x.2.on_ground).map(|x| (x.0, x.1 .0)).collect();
    for (e, at) in landed {
        let Some(b) = w.get::<FallingBlock>(e).cloned() else { continue };
        let cell = [at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32];
        let mw = crate::blocks::world_of(w);
        let (here, below) = (mw.world.block(cell[0], cell[1], cell[2]), mw.world.block(cell[0], cell[1] - 1, cell[2]));
        let free = |s| mw.facts.is_air(s) || mw.facts.get(s).replaceable;
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
