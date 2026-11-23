//! The falling block (`FallingBlockEntity`): a block that lost its support becomes a body that falls as `Entity.move` makes it (gravity 0.04,
//! the drag 0.98 on every axis: the body of `tnt.rs`), and lands on the tick its move ends on the ground. The landing (`falling_land`) hurts
//! the entities under it, and the host hears it: `Landed`, then the block it places (`SetBlock`) or the break (`Broke`). It is visited only
//! while it falls: once landed it is gone, and the block it placed rests as any block does.
//! Ported: the 600-tick limit (a deadline), the concrete powder that lands in water, the damage an anvil or a stalactite does under it, the
//! anvil's wear. Not ported: the can-survive test of the block (every block survives), the powder that is hit in water by a fast fall, the
//! cobweb and bubble-column pushes, the build-height limit (the generic fall under the world removes an object that left it).
use crate::cow::*;
use crate::falling_land::{self, Landing};
use crate::goals::Events;
use crate::host_out_falling::FallingEvent;
use crate::item::drop_item;
use crate::tnt::SIZE;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Compound;
use horizon_world::registry::BlockStateId;

/// A falling block: the state it will be placed with, the item it drops when it cannot be, the block entity data
/// (`TileEntityData`) its placed block loads, and its settings.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct FallingBlock {
    pub state: BlockStateId,
    pub drop: Option<ItemStack>,
    /// `Entity.fallDistance` as the last tick in the air left it (the move that lands resets the body's own before `lands` reads it).
    pub fell: f64,
    pub data: Option<Compound>,
    pub settings: FallSettings,
    /// `Entity.tickCount = now - born`, in the entity's own time (the `Time` key).
    pub born: u64,
}

/// The settings of a falling block, the fields of `FallingBlockEntity` with the jar's NBT keys: the defaults are the jar's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FallSettings {
    /// `HurtEntities`: the block hurts the living entities in its box when it lands.
    pub hurt_entities: bool,
    /// `FallHurtAmount`: the damage a block of fall does.
    pub fall_hurt_amount: f32,
    /// `FallHurtMax`: the cap of that damage.
    pub fall_hurt_max: i32,
    /// `DropItem`: the item drops when the block cannot be placed.
    pub drop_item: bool,
    /// `CancelDrop`: nothing is placed and nothing drops.
    pub cancel_drop: bool,
}

impl Default for FallSettings {
    fn default() -> Self {
        Self { hurt_entities: false, fall_hurt_amount: 0.0, fall_hurt_max: 40, drop_item: true, cancel_drop: false }
    }
}

/// The state of the falling block `e` (its `BlockState`); None once it has landed. A read: nothing is written for it.
pub fn falling_state(w: &World, e: Entity) -> Option<BlockStateId> {
    w.get::<FallingBlock>(e).map(|b| b.state)
}

/// `FallingBlockEntity.fall`: the block at `cell` is replaced by air and a falling one starts at its centre, 0.01 above its floor (the body is
/// 0.98 high), at rest.
pub fn fall(w: &mut World, cell: [i32; 3], state: BlockStateId, drop: Option<ItemStack>) -> Entity {
    let mw = crate::blocks::world_of(w);
    if let Some(air) = crate::blocks::resolve(mw, "minecraft:air", &[]) {
        crate::blocks::set_state_id(w, cell, air);
    }
    let name = crate::blocks::world_of(w).facts.get(state).name.to_string();
    // `setHurtsEntities` and the damage per step and its cap, set at the creation of an anvil or a stalactite.
    let hurt = crate::falling_land::hurt_rule(&name);
    let settings = FallSettings { hurt_entities: hurt.is_some(), fall_hurt_amount: hurt.map_or(0.0, |h| h.0), fall_hurt_max: hurt.map_or(40, |h| h.1), ..FallSettings::default() };
    let at = Vec3::new(f64::from(cell[0]) + 0.5, f64::from(cell[1]) + f64::from((1.0 - SIZE) / 2.0), f64::from(cell[2]) + 0.5);
    spawn(w, at, Vec3::ZERO, state, drop, settings)
}

/// The falling block of `state` at `at` (its centre bottom) moving at `velocity`, with `drop` and `settings`: the body `fall` starts, without
/// clearing any cell.
pub fn spawn(w: &mut World, at: Vec3, velocity: Vec3, state: BlockStateId, drop: Option<ItemStack>, settings: FallSettings) -> Entity {
    let body = Body { width: SIZE, height: SIZE, step_height: 0.0, speed: 0.0, fall_factor: 1.0, safe_fall: 3.0, fall_mult: 1.0, movement_efficiency: 0.0, lava_walker: false, object: true, orb: false, tnt: true };
    let e = w.spawn((Pos(at), Vel(velocity), OwnTime, body, Gravity(0.04), Bounds(box_at(at, SIZE, SIZE)), Contact { on_ground: false, horizontal: false, fall_distance: 0.0, stuck: 0 }, FluidState::default(), FallingBlock { state, drop, fell: 0.0, data: None, settings, born: w.resource::<Clock>().0 }, Moving))
.id();
    crate::nbt::mark_saved(w, e, crate::species::lists::FALLING_BLOCK);
    w.entity_mut(e).insert(crate::cow::TypeName(crate::species::lists::FALLING_BLOCK));
    after(w, e, LIMIT + 1, expire);
    crate::zone::settle(w, e);
    e
}

/// `FallingBlockEntity.tick`: a block still in the air after 600 ticks is removed, and drops its item.
pub const LIMIT: i32 = 600;
pub fn expire(w: &mut World, e: Entity) {
    if w.get::<Contact>(e).is_none_or(|c| c.on_ground) {
        return;
    }
    let (Some(b), Some(at)) = (w.get::<FallingBlock>(e).cloned(), w.get::<Pos>(e).map(|p| p.0)) else { return };
    w.entity_mut(e).despawn();
    if let Some(item) = b.drop.filter(|_| b.settings.drop_item) {
        drop_item(w, item, at, Vec3::ZERO, 10);
    }
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(lands.after(travel).before(reindex));
}

/// The tick's end for the blocks in the air: the ones whose move ended on the ground land, and the host hears each landing.
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
        let at3 = [at.x, at.y, at.z];
        w.resource_mut::<Events>().host.falling(FallingEvent::Landed { entity: e, at: at3, fall: b.fell, data: b.data.clone() });
        falling_land::hurt_below(w, e);
        let landing = falling_land::land(w, e);
        w.entity_mut(e).despawn();
        let mut ev = w.resource_mut::<Events>();
        match landing {
            Landing::Place(state, cell) => ev.host.set_block(cell, state),
            Landing::Drop(state) => ev.host.falling(FallingEvent::Broke { at: at3, state, drop: b.settings.drop_item && b.drop.is_some() }),
            Landing::Break(state) => ev.host.falling(FallingEvent::Broke { at: at3, state, drop: false }),
        }
    }
}
