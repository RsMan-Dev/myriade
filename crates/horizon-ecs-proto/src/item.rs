//! The item capability (`Cap::Item`): a stack that can be picked up, merges with the stacks near it and expires. Nothing else: the
//! body that makes it fall is `Body` (with `Body.object`, `ItemEntity.tick`'s motion), the box is `Solid`, and an entity that is also
//! alive, or walks, or has goals, is still an item: no system here asks for another capability or refuses one.
//!
//! Zero cost at rest. The stack's time is counted from its own clock (`age = now - age_base`, `ticks = now - born`): nothing counts a
//! tick for it. Its end of life is a deadline of the wheel (`Due::ItemExpire`); its merge check is vanilla's (`mergeWithNeighbours`:
//! every 2 ticks while it changes block, every 40 otherwise) asked only (a) from the items that move (`Moving`: the cell changed,
//! the tick is a multiple of the rate), and (b) from a deadline at its own multiple of 40 armed when it is visited, and re-armed only
//! while another stack lies within reach: a stack alone on the ground is never looked at again. The merge itself is Horizon's
//! (`merge_with_neighbours`), run on copies of the stacks within reach.
use crate::cow::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::entities::{merge_with_neighbours, ItemCells, ItemEntity};
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::{Aabb, Vec3};
use std::collections::HashMap;

/// A stack on the ground.
#[derive(Component, Clone)]
#[component(on_remove = leave_index)]
pub struct Item {
    pub stack: ItemStack,
    /// `Entity.tickCount = now - born`.
    pub born: u64,
    /// `ItemEntity.age = now - age_base` (a merge takes the smaller age).
    pub age_base: u64,
    /// Own time when the item can be picked up (`None`: never, `ItemEntity.setNeverPickUp`).
    pub ready_at: Option<u64>,
    /// `ItemEntity.target`: the only player who may pick it up.
    pub target: Option<[i32; 4]>,
    /// The block cell it is listed in.
    pub cell: [i32; 3],
    /// The tokens of its pending deadlines (0: none).
    expire: u32,
    check: u32,
    /// `ItemEntity.id`, the order the stacks joined (a merge visits them in it).
    pub id: u32,
}
/// The size of the box of an item (`EntityType.ITEM`).
pub const SIZE: f64 = 0.25;
/// `ItemEntity.getDespawnTime`'s 6 000 ticks.
pub const LIFE: u64 = 6000;
/// An item that moves (`Moving` was put on it): the item system visits it, and only it, until it is at rest.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_add = awake_added, on_remove = awake_removed)]
pub struct ItemAwake;
fn awake_added(mut world: bevy_ecs::world::DeferredWorld, _: bevy_ecs::lifecycle::HookContext) {
    world.resource_mut::<ItemIndex>().awake += 1;
}
fn awake_removed(mut world: bevy_ecs::world::DeferredWorld, _: bevy_ecs::lifecycle::HookContext) {
    world.resource_mut::<ItemIndex>().awake -= 1;
}
/// The dropped items by block cell, so that a merge, a pickup or a search finds them without a scan.
#[derive(Resource, Default)]
pub struct ItemIndex {
    cells: HashMap<[i32; 3], Vec<Entity>>,
    next_id: u32,
    token: u32,
    /// The items in motion (`ItemAwake`): the item system does not run while there is none.
    awake: usize,
}
impl ItemIndex {
    fn add(&mut self, e: Entity, cell: [i32; 3]) {
        self.cells.entry(cell).or_default().push(e);
    }
    fn remove(&mut self, e: Entity, cell: [i32; 3]) {
        if let Some(v) = self.cells.get_mut(&cell) {
            v.retain(|&x| x != e);
            if v.is_empty() {
                self.cells.remove(&cell);
            }
        }
    }
    fn token(&mut self) -> u32 {
        self.token += 1;
        self.token
    }
    /// The items whose cells could hold a box meeting `area` (an item's box reaches 0.125 sideways and 0.25 up from its position).
    pub fn near(&self, area: Aabb) -> Vec<Entity> {
        let (min, max) = (area.min_position(), area.max_position());
        let (lo, hi) = (cell_of(Vec3::new(min.x - 0.125, min.y - 0.25, min.z - 0.125)), cell_of(Vec3::new(max.x + 0.125, max.y, max.z + 0.125)));
        let mut out = Vec::new();
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    if let Some(v) = self.cells.get(&[x, y, z]) {
                        out.extend_from_slice(v);
                    }
                }
            }
        }
        out
    }
    pub fn len(&self) -> usize {
        self.cells.values().map(Vec::len).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}
fn cell_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
pub(crate) fn box_at(p: Vec3) -> Aabb {
    let h = SIZE / 2.0;
    Aabb::new(Vec3::new(p.x - h, p.y, p.z - h), Vec3::new(p.x + h, p.y + SIZE, p.z + h))
}
fn leave_index(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let cell = world.get::<Item>(ctx.entity).map(|d| d.cell);
    if let Some(c) = cell {
        world.resource_mut::<ItemIndex>().remove(ctx.entity, c);
    }
}

pub fn insert_resources(w: &mut World) {
    w.insert_resource(ItemIndex::default());
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(item_moved.after(travel).before(reindex));
}

/// The components of the item on the ground of vanilla: a position, a velocity, a box in the index and a body of an object.
pub fn drop_item(world: &mut World, stack: ItemStack, at: Vec3, velocity: Vec3, pickup_delay: i64) -> Entity {
    let id = {
        let mut ix = world.resource_mut::<ItemIndex>();
        ix.next_id += 1;
        ix.next_id
    };
    let cell = cell_of(at);
    let ready = if pickup_delay == i64::from(i16::MAX) || pickup_delay == i64::from(u32::MAX) { None } else { Some(pickup_delay.max(0) as u64) };
    let e = world
        .spawn((
            Pos(at),
            Vel(velocity),
            crate::goals::Uid(crate::goals::next_uid()),
            OwnTime,
            PushFree,
            Body { width: SIZE as f32, height: SIZE as f32, step_height: 0.0, gravity: 0.04, speed: 0.0, fall_factor: 1.0, object: true, orb: false, lava_walker: false },
            Bounds(box_at(at)),
            Cell(u32::MAX, [i32::MIN; 8], u32::MAX),
            Contact { on_ground: false, horizontal: false, fall_distance: 0.0 },
            FluidParams { eye_height: f64::from(0.2125_f32), can_float: false, water_slowdown: 0.8, water_efficiency: 0.0 },
            FluidState::default(),
            Recheck,
            Moving,
        ))
        .id();
    world.entity_mut(e).insert((
            // Its health: 5 (`ItemEntity.health`); the lava, the fire and the cactus of the frame hurt it as they hurt a creature.
            Sizes { adult: (SIZE as f32, SIZE as f32), baby: (SIZE as f32, SIZE as f32) },
            Health(5.0),
            HurtAt { at: 0, last: 0.0 },
            Frozen(0),
            Air(300),
            Resist(0.0),
            Immune(BREATHES_WATER | NO_HURT_COOLDOWN),
            TypeName("minecraft:item"),
    ));
    let now = now_of(world, e);
    world.entity_mut(e).insert(ItemAwake);
    world.entity_mut(e).insert(Item { stack, born: now, age_base: now, ready_at: ready.map(|d| now + d), target: None, cell, expire: 0, check: 0, id });
    world.resource_mut::<ItemIndex>().add(e, cell);
    if let Some(mut ev) = world.get_resource_mut::<crate::goals::Events>() {
        ev.item_dropped.push(e);
    }
    enter_index(world, e);
    arm_expire(world, e);
    arm_check(world, e);
    e
}
fn max_stack(mw: &horizon_client_lib::mobs::MobWorld<'static>, s: &ItemStack) -> u32 {
    mw.items.and_then(|i| i.get(&s.item)).map_or(64, |i| i.max_stack).max(1)
}
/// The wheel reaches 4 095 ticks: a longer wait is a deadline at its end, which waits on.
const REACH: u64 = 4000;
fn arm_expire(world: &mut World, e: Entity) {
    let now = now_of(world, e);
    let token = world.resource_mut::<ItemIndex>().token();
    let Some(mut it) = world.get_mut::<Item>(e) else { return };
    it.expire = token;
    let at = it.age_base + LIFE;
    arm_at(world, e, at.min(now + REACH).max(now + 1), Due::ItemExpire(token));
}
/// Arms the end of life again after the item's age was changed from outside.
pub fn rearm_expiry(world: &mut World, e: Entity) {
    arm_expire(world, e);
}
pub fn expire_due(world: &mut World, e: Entity, token: u32) {
    let Some(it) = world.get::<Item>(e) else { return };
    if it.expire != token {
        return;
    }
    if now_of(world, e) < it.age_base + LIFE {
        arm_expire(world, e);
        return;
    }
    world.entity_mut(e).despawn();
}
/// The deadline at the item's next multiple of 40 ticks (`ItemEntity.tick`'s rate for an item that stays in its block).
fn arm_check(world: &mut World, e: Entity) {
    let now = now_of(world, e);
    let token = world.resource_mut::<ItemIndex>().token();
    let Some(mut it) = world.get_mut::<Item>(e) else { return };
    it.check = token;
    let to40 = 40 - (now - it.born) % 40;
    arm_at(world, e, now + to40, Due::ItemCheck(token));
}
pub fn check_due(world: &mut World, e: Entity, token: u32) {
    let Some(it) = world.get::<Item>(e) else { return };
    if it.check != token {
        return;
    }
    world.get_mut::<Item>(e).unwrap().check = 0;
    let mw = world.resource::<Level>().mw.0;
    merge(world, e, mw);
    // Another stack within reach (one that did not merge: full, or not yet mergeable): look again in 40 ticks.
    if world.get::<Item>(e).is_some() && !partners(world, e, mw).is_empty() {
        arm_check(world, e);
    }
}
/// The stacks within reach that could merge with `e` (`ItemEntity.areMergable`'s test of the stacks: the same item and components, the same
/// player it is kept for, both below their size and fitting one stack); the pickup delays and the ages are the merge's own.
fn partners(world: &World, e: Entity, mw: &horizon_client_lib::mobs::MobWorld<'static>) -> Vec<Entity> {
    let (Some(p), Some(me)) = (world.get::<Pos>(e), world.get::<Item>(e)) else { return Vec::new() };
    let area = box_at(p.0).inflate(Vec3::new(0.5, 0.0, 0.5));
    let max = max_stack(mw, &me.stack);
    if me.stack.count >= max {
        return Vec::new();
    }
    world
        .resource::<ItemIndex>()
        .near(area)
        .into_iter()
        .filter(|&o| o != e)
        .filter(|&o| {
            let (Some(q), Some(it)) = (world.get::<Pos>(o), world.get::<Item>(o)) else { return false };
            it.target == me.target && it.stack.count < max && it.stack.count + me.stack.count <= max && it.stack.same_item_same_components(&me.stack) && box_at(q.0).intersects(area)
        })
        .collect()
}

/// Each tick, the items that move: the cell they are in (the index follows), and `ItemEntity.tick`'s merge: every 2 ticks while the block
/// changes, every 40 otherwise (a check pending at its own multiple of 40 is armed for a stack that was not looked at).
fn item_moved(world: &mut World) {
    if world.resource::<ItemIndex>().awake == 0 {
        return;
    }
    let mut list: Vec<(u32, Entity)> = world.query_filtered::<(Entity, &Item), With<ItemAwake>>().iter(world).map(|(e, it)| (it.id, e)).collect();
    if list.is_empty() {
        return;
    }
    list.sort_unstable();
    let mw = world.resource::<Level>().mw.0;
    for (_, e) in list {
        let Some(p) = world.get::<Pos>(e).map(|p| p.0) else { continue };
        let now = now_of(world, e);
        let cell = cell_of(p);
        let resting = world.get::<Moving>(e).is_none();
        let Some(it) = world.get::<Item>(e) else { continue };
        let (old, ticks, pending) = (it.cell, now - it.born, it.check != 0);
        let changed = old != cell;
        if changed {
            let mut ix = world.resource_mut::<ItemIndex>();
            ix.remove(e, old);
            ix.add(e, cell);
            world.get_mut::<Item>(e).unwrap().cell = cell;
        }
        if ticks % if changed { 2 } else { 40 } == 0 {
            merge(world, e, mw);
        }
        if world.get::<Item>(e).is_some() && !pending {
            arm_check(world, e);
        }
        // At rest: its last visit; it is listed again when something moves it.
        if resting && world.get::<Item>(e).is_some() {
            world.entity_mut(e).remove::<ItemAwake>();
        }
    }
}
/// `ItemEntity.mergeWithNeighbours` for `e`: the real rule (Horizon's), on copies of the stacks within reach.
fn merge(world: &mut World, e: Entity, mw: &horizon_client_lib::mobs::MobWorld<'static>) {
    let near = partners(world, e, mw);
    if near.is_empty() {
        return;
    }
    let mut group: Vec<(u32, Entity)> = near.into_iter().filter_map(|o| world.get::<Item>(o).map(|i| (i.id, o))).collect();
    group.push((world.get::<Item>(e).unwrap().id, e));
    group.sort_unstable();
    group.dedup();
    let now = now_of(world, e);
    let mut copies: Vec<ItemEntity> = group.iter().map(|&(_, o)| copy_of(world, o, now)).collect();
    let before: Vec<(u32, i32, i64)> = copies.iter().map(|c| (c.stack.count, c.age, c.pickup_delay)).collect();
    let i = group.iter().position(|&(_, o)| o == e).unwrap();
    let mut removed = vec![false; copies.len()];
    let mut cells = ItemCells::new(&copies);
    merge_with_neighbours(&mut copies, &mut removed, &mut cells, i, true, &|s: &ItemStack| max_stack(mw, s));
    for (k, &(_, o)) in group.iter().enumerate() {
        if removed[k] {
            world.entity_mut(o).despawn();
            continue;
        }
        let c = &copies[k];
        if (c.stack.count, c.age, c.pickup_delay) != before[k] {
            let mut it = world.get_mut::<Item>(o).unwrap();
            it.stack.count = c.stack.count;
            it.age_base = now.saturating_sub(c.age.max(0) as u64);
            it.ready_at = if c.pickup_delay == i64::from(i16::MAX) || c.pickup_delay == i64::from(u32::MAX) { None } else { Some(now + c.pickup_delay.max(0) as u64) };
            arm_expire(world, o);
        }
    }
}
/// Horizon's `ItemEntity` of an item of the world, for its rules (the merge): the age and the pickup delay at `now`.
fn copy_of(world: &World, e: Entity, now: u64) -> ItemEntity {
    let it = world.get::<Item>(e).unwrap();
    let at = world.get::<Pos>(e).unwrap().0;
    let delay = it.ready_at.map_or(i64::from(i16::MAX), |r| r.saturating_sub(now) as i64);
    ItemEntity {
        random: horizon_core::random::EntityRandom::own(horizon_core::random::LegacyRandom::new(i64::from(it.id))),
        attachment_lifetime: Default::default(),
        rotation: [0.0, 0.0],
        join: Default::default(),
        health: 5,
        fire_ticks: 0,
        id: it.id,
        stack: it.stack.clone(),
        thrower: None,
        target: it.target,
        position: at,
        previous: at,
        velocity: Vec3::ZERO,
        on_ground: true,
        fall_distance: 0.0,
        age: (now - it.age_base) as i32,
        pickup_delay: delay,
        bob_offset: 0.0,
        ticks: (now - it.born) as u32,
        needs_sync: false,
        stuck: Vec3::ZERO,
        was_touching_water: false,
        no_gravity: false,
        vertical_collision: false,
        horizontal_collision: false,
    }
}

// ---- what the rest of the frame does with the items on the ground -------------------------------------------------------
/// `Entity.spawnAtLocation`: a stack at `at`, thrown as `ItemEntity`'s constructor throws it (`nextDouble() * 0.2 - 0.1` sideways, 0.2 up
/// from `random`) and not to be picked up for 10 ticks.
pub fn spawn_at_location(world: &mut World, stack: ItemStack, at: Vec3, random: &mut impl horizon_core::random::RandomSource) -> Entity {
    let v = Vec3::new(random.next_double() * 0.2 - 0.1, 0.2, random.next_double() * 0.2 - 0.1);
    drop_item(world, stack, at, v, 10)
}
/// The items on the ground whose box meets `area` (`level.getEntitiesOfClass(ItemEntity.class, area)`, in the order they joined).
pub fn items_in(world: &World, area: Aabb) -> Vec<Entity> {
    let mut v: Vec<(u32, Entity)> = world
        .resource::<ItemIndex>()
        .near(area)
        .into_iter()
        .filter_map(|e| {
            let (p, it) = (world.get::<Pos>(e)?, world.get::<Item>(e)?);
            box_at(p.0).intersects(area).then_some((it.id, e))
        })
        .collect();
    v.sort_unstable();
    v.into_iter().map(|x| x.1).collect()
}
/// `!ItemEntity.hasPickUpDelay()`.
pub fn pickable(world: &World, item: Entity) -> bool {
    world.get::<Item>(item).is_some_and(|it| it.ready_at.is_some_and(|r| now_of(world, item) >= r))
}
/// A creature takes `count` of the stack of `item` (`Fox.pickUpItem` takes one): what it took, or `None` when it is not to be picked up yet.
/// The item is gone when its stack is used up, and the creature is in `Events.picked_up` (read at the next tick).
pub fn pick_up(world: &mut World, creature: Entity, item: Entity, count: u32) -> Option<ItemStack> {
    if !pickable(world, item) || count == 0 {
        return None;
    }
    let mut it = world.get_mut::<Item>(item)?;
    let n = count.min(it.stack.count);
    let mut taken = it.stack.clone();
    taken.count = n;
    it.stack.count -= n;
    let empty = it.stack.count == 0;
    if empty {
        world.entity_mut(item).despawn();
    }
    world.resource_mut::<crate::goals::Events>().picked_up.push(creature);
    // `Mob.pickUpItem` sets `persistenceRequired`: what holds something never goes away (and the spawn caps stop counting it).
    crate::despawn::keep(world, creature);
    Some(taken)
}
/// `LivingEntity.drop` of the main hand: the stack in the hand goes out at the creature's mouth, thrown as `spawn_at_location` throws it.
pub fn drop_main_hand(world: &mut World, e: Entity, random: &mut impl horizon_core::random::RandomSource) -> Option<Entity> {
    let stack = crate::goals::main_hand(world, e)?.clone();
    let at = world.get::<Pos>(e)?.0;
    let eye = world.get::<FluidParams>(e).map_or(0.0, |f| f.eye_height);
    crate::goals::set_main_hand(world, e, None);
    Some(spawn_at_location(world, stack, Vec3::new(at.x, at.y + eye - 0.3, at.z), random))
}

/// The registry name of the item of the stack on the ground (`item::kind_of`), for the goals that list items by name.
pub fn kind_of(world: &World, item: Entity) -> Option<&'static str> {
    let it = world.get::<Item>(item)?;
    crate::items::item_name(crate::items::item_id(&it.stack.item))
}
