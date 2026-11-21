//! `Transport` (`CopperGolemAi`, `transport_items.rs`, the copper golem's idle activity at priority 0): with an empty hand the golem takes
//! the items of a copper chest, else it puts what it holds into a chest. It walks to the chest of its search box (32 blocks sideways, 8 up
//! and down) that the `Mob` picks first, queues behind a golem already at it, works there for 60 ticks with the lid open, then keeps going.
//! The visited and unreachable chests are memories that expire 6000 ticks after their last change; a trip that finds no chest, or that
//! visits more than 10 (50 unreachable), enters the cooldown. The contents are read from the world's block entities; the changes, the lid
//! and the animation state are host events. Left out: the path search (a travel is tested with the path memory only, not a path end)
//! and the golem's leash; the block entities are scanned whole (the store has no index of chests).
use crate::behaviours::Cap;
use crate::blocks;
use crate::brain::*;
use crate::cow::{Bounds, Pos};
use crate::goals::{d2, main_hand, set_main_hand, Events};
use bevy_ecs::prelude::Component;
use horizon_client_lib::containers::BlockEntity;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::copper_golem as cg;
use horizon_client_lib::mobs::data::REDSTONE_CONDUCTOR;
use horizon_client_lib::mobs::transport_items::chest_hinge_sound;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::java_hash_map::{JavaHashMap, JavaKey};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:transport_items",
    mems: &[&TRANSPORT_ITEMS_COOLDOWN_TICKS, &IS_PANICKING, &WALK_TARGET, &LOOK_TARGET, &VISITED, &UNREACHABLE, &PATH],
    requires: &[Cap::Body, Cap::Locomotion],
    equip,
    can_start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

/// The visited and unreachable chests: their memories, expiring 6000 ticks after the last change (`set_visited`).
static VISITED: MemKey = MemKey::new("minecraft:visited_block_positions", Shape::Blocks);
static UNREACHABLE: MemKey = MemKey::new("minecraft:unreachable_transport_block_positions", Shape::Blocks);
const MEM_TTL: u64 = 6000;
/// The cooldown after a trip that found no chest, or too many (`enter_cooldown`).
const COOLDOWN_TICKS: i64 = 140;
const VISITED_MAX: usize = 10;
const UNREACHABLE_MAX: usize = 50;
/// The search box around the golem's block (`find_target`).
const SEARCH_H: i32 = 32;
const SEARCH_V: i32 = 8;
/// The ticks spent in front of the chest before the trade (`on_reached`).
const WORK_TICKS: u32 = 60;

/// `Transport.State`: travelling, queuing behind another golem at the chest, working in front of it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum State {
    #[default]
    Travelling,
    Queuing,
    Interacting,
}

/// `Transport.Interaction`: what the golem does at the chest once it has reached it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Interaction {
    PickupItem,
    PickupNoItem,
    PlaceItem,
    PlaceNoItem,
}

/// The trip's chest: its position, its name and its halves (two for a double chest, in slot order).
#[derive(Clone, Debug, Default)]
struct Target {
    pos: [i32; 3],
    name: &'static str,
    halves: Vec<[i32; 3]>,
}

/// The trip's state. A component of the entity, as the door's state: a memory write would wake the brain at each step.
#[derive(Component, Clone, Debug, Default)]
pub struct Carry {
    target: Option<Target>,
    state: State,
    interaction: Option<Interaction>,
    ticks: u32,
    opened: Option<[i32; 3]>,
}

fn equip(w: &mut World, e: Entity, _: &Ctx) {
    if w.get::<Carry>(e).is_none() {
        w.entity_mut(e).insert(Carry::default());
    }
}

/// `Transport.try_start`: no transport cooldown and not panicking.
fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    !present(w, e, &TRANSPORT_ITEMS_COOLDOWN_TICKS) && !present(w, e, &IS_PANICKING)
}

/// `Transport.tick_or_stop` and `tick`: an invalid chest is replaced (the golem stops there and goes on the next tick), then the walk,
/// the queue and the visit.
fn tick(w: &mut World, e: Entity, _: &Ctx) -> bool {
    if present(w, e, &TRANSPORT_ITEMS_COOLDOWN_TICKS) || present(w, e, &IS_PANICKING) {
        return false;
    }
    let (Some(me), Some(size)) = (w.get::<Pos>(e).map(|q| q.0), w.get::<Bounds>(e).map(|b| b.0.size())) else { return false };
    let mw = blocks::world_of(w);
    if update_invalid_target(w, e, mw, me, size) {
        return true;
    }
    let Some(t) = target_of(w, e) else { return true };
    if state(w, e) == State::Queuing && !another_interacting(w, e, mw, &t) {
        set_state(w, e, State::Travelling);
        walk_towards(w, e, &t);
    }
    if state(w, e) == State::Travelling {
        let c = centre_of(me, size);
        if within(mw, size, 3.0, &t, c) && another_interacting(w, e, mw, &t) {
            erase(w, e, &WALK_TARGET);
            set_state(w, e, State::Queuing);
        } else if within(mw, size, range(w, e), &t, c) {
            let it = reached_interaction(w, e, mw, &t.halves);
            if let Some(mut c) = w.get_mut::<Carry>(e) {
                c.interaction = Some(it);
                c.state = State::Interacting;
            }
        } else {
            walk_towards(w, e, &t);
        }
    }
    if state(w, e) == State::Interacting {
        on_reached(w, e, mw, me, size, &t);
    }
    true
}

/// `Transport.do_stop`: the trip restarts from its travel, the chest kept.
fn stop(w: &mut World, e: Entity, _: &Ctx) {
    on_start_travelling(w, e);
}

fn state(w: &World, e: Entity) -> State {
    w.get::<Carry>(e).map_or(State::Travelling, |c| c.state)
}
fn set_state(w: &mut World, e: Entity, s: State) {
    if let Some(mut c) = w.get_mut::<Carry>(e) {
        c.state = s;
    }
}
fn target_of(w: &World, e: Entity) -> Option<Target> {
    w.get::<Carry>(e)?.target.clone()
}

/// `range`: the reach of a travel, 1 once the path is done (here: no path memory), else 0.5.
fn range(w: &World, e: Entity) -> f64 {
    if present(w, e, &PATH) { 0.5 } else { 1.0 }
}

/// `CopperGolemAi.onTravelling`: the opened chest is forgotten and the animation is idle.
fn on_start_travelling(w: &mut World, e: Entity) {
    if let Some(mut c) = w.get_mut::<Carry>(e) {
        c.opened = None;
        c.state = State::Travelling;
        c.interaction = None;
        c.ticks = 0;
    }
    w.resource_mut::<Events>().host.golem_state(e, cg::IDLE);
}

/// `stop_targeting`: the chest and the walk to it are dropped.
fn stop_targeting(w: &mut World, e: Entity) {
    if let Some(mut c) = w.get_mut::<Carry>(e) {
        c.target = None;
        c.state = State::Travelling;
        c.ticks = 0;
    }
    erase(w, e, &WALK_TARGET);
}

/// `clear_memories`: the trip stops and the visited and unreachable chests are forgotten.
fn clear_memories(w: &mut World, e: Entity) {
    stop_targeting(w, e);
    erase(w, e, &VISITED);
    erase(w, e, &UNREACHABLE);
}

/// `enter_cooldown`: the trip stops, the cooldown is set (expiring by its deadline) and the lists are forgotten.
fn enter_cooldown(w: &mut World, e: Entity) {
    stop_targeting(w, e);
    set(w, e, &TRANSPORT_ITEMS_COOLDOWN_TICKS, MemValue::Int(COOLDOWN_TICKS), Some(COOLDOWN_TICKS as u64));
    erase(w, e, &VISITED);
    erase(w, e, &UNREACHABLE);
}

fn mem_list(w: &World, e: Entity, k: &MemKey) -> Vec<[i32; 3]> {
    match get(w, e, k) {
        Some(MemValue::Blocks(v)) => v.clone(),
        _ => Vec::new(),
    }
}

/// `set_visited`: the chest is remembered at its pick; more than `VISITED_MAX` of them enter the cooldown.
fn set_visited(w: &mut World, e: Entity, p: [i32; 3]) {
    let mut list = mem_list(w, e, &VISITED);
    if !list.contains(&p) {
        list.push(p);
    }
    if list.len() > VISITED_MAX {
        enter_cooldown(w, e);
    } else {
        set(w, e, &VISITED, MemValue::Blocks(list), Some(MEM_TTL));
    }
}

/// `mark_unreachable`: the chest leaves the visited list for the unreachable one.
fn mark_unreachable(w: &mut World, e: Entity, p: [i32; 3]) {
    let mut visited = mem_list(w, e, &VISITED);
    visited.retain(|q| *q != p);
    let mut unreachable = mem_list(w, e, &UNREACHABLE);
    if !unreachable.contains(&p) {
        unreachable.push(p);
    }
    if unreachable.len() > UNREACHABLE_MAX {
        enter_cooldown(w, e);
    } else {
        set(w, e, &VISITED, MemValue::Blocks(visited), Some(MEM_TTL));
        set(w, e, &UNREACHABLE, MemValue::Blocks(unreachable), Some(MEM_TTL));
    }
}

/// `update_invalid_target`: a chest that is no longer valid is replaced by the nearest wanted one; true when the tick is spent on it.
fn update_invalid_target(w: &mut World, e: Entity, mw: &MobWorld, me: Vec3, size: Vec3) -> bool {
    if has_valid_target(w, e, mw, me, size) {
        return false;
    }
    stop_targeting(w, e);
    match find_target(w, e, mw, me) {
        Some(t) => {
            let pos = t.pos;
            if let Some(mut c) = w.get_mut::<Carry>(e) {
                c.target = Some(t);
            }
            on_start_travelling(w, e);
            set_visited(w, e, pos);
        }
        None => enter_cooldown(w, e),
    }
    true
}

/// `has_valid_target`: the chest is still a chest of the kind wanted, not blocked; a travel that cannot reach it marks it unreachable.
fn has_valid_target(w: &mut World, e: Entity, mw: &MobWorld, me: Vec3, size: Vec3) -> bool {
    let Some(t) = target_of(w, e) else { return false };
    let unchanged = matches!(mw.world.block_entities.get(&(t.pos[0], t.pos[1], t.pos[2])), Some(BlockEntity::Chest(_)))
        && is_chest_block(mw.block_name(t.pos[0], t.pos[1], t.pos[2]));
    if !(wanted(w, e, mw, t.name) && unchanged) || blocked(mw, t.pos) {
        return false;
    }
    if state(w, e) != State::Travelling {
        return true;
    }
    if valid_travelling_path(w, e, mw, me, size, &t) {
        return true;
    }
    mark_unreachable(w, e, t.pos);
    false
}

/// `valid_travelling_path`, without a path search: the eye is where the golem stands; a path memory says there is one.
fn valid_travelling_path(w: &World, e: Entity, mw: &MobWorld, me: Vec3, size: Vec3, t: &Target) -> bool {
    let path = present(w, e, &PATH);
    let from = centre_of(me, size);
    let reach = within(mw, size, range(w, e), t, from);
    (!path && !reach) || reach && sees_a_side(mw, t.pos, from)
}

/// `within`: the golem's box at `from`, grown by `distance` sideways and 0.5 up and down, meets the collision shape of the chest.
fn within(mw: &MobWorld, size: Vec3, distance: f64, t: &Target, from: [f64; 3]) -> bool {
    let moved = Aabb::new(
        Vec3::new(from[0] - size.x / 2.0, from[1] - size.y / 2.0, from[2] - size.z / 2.0),
        Vec3::new(from[0] + size.x / 2.0, from[1] + size.y / 2.0, from[2] + size.z / 2.0),
    );
    let Some(shape) = mw.collision_bounds(t.pos) else { return false };
    let p = Vec3::new(f64::from(t.pos[0]), f64::from(t.pos[1]), f64::from(t.pos[2]));
    shape.inflate(Vec3::new(distance, 0.5, distance)).move_by(p).intersects(moved)
}

/// `Entity.getEyePosition`-like centre of the golem: the middle of its height.
fn centre_of(me: Vec3, size: Vec3) -> [f64; 3] {
    [me.x, me.y + size.y / 2.0, me.z]
}

/// `sees_a_side`: a clip from `eye` to the middle of any face of the chest's block hits that block.
fn sees_a_side(mw: &MobWorld, p: [i32; 3], eye: [f64; 3]) -> bool {
    let c = Vec3::new(f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5);
    // Direction order: down, up, north, south, west, east.
    let steps = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];
    let from = Vec3::new(eye[0], eye[1], eye[2]);
    steps.iter().any(|s| {
        let to = Vec3::new(c.x + 0.5 * f64::from(s[0]), c.y + 0.5 * f64::from(s[1]), c.z + 0.5 * f64::from(s[2]));
        mw.clip_block_at(from, to) == Some(p)
    })
}

fn is_chest_block(name: &str) -> bool {
    matches!(name, "minecraft:chest" | "minecraft:trapped_chest") || name.ends_with("copper_chest")
}

/// `wanted`: an empty hand takes from a copper chest, a full one puts into a chest.
fn wanted(w: &World, e: Entity, mw: &MobWorld, name: &str) -> bool {
    if main_hand(w, e).is_none() {
        mw.has_tag("minecraft:copper_chests", name)
    } else {
        matches!(name, "minecraft:chest" | "minecraft:trapped_chest")
    }
}

/// `blocked` (`ChestBlock.isChestBlockedAt`): a redstone conductor above.
fn blocked(mw: &MobWorld, p: [i32; 3]) -> bool {
    let above = mw.world.block(p[0], p[1] + 1, p[2]);
    mw.data.state(above).flags & REDSTONE_CONDUCTOR != 0
}

/// `connected`: the other half of a double chest, from its `type` and `facing`.
fn connected(mw: &MobWorld, p: [i32; 3]) -> Option<[i32; 3]> {
    let kind = blocks::property(mw, p, "type")?;
    if kind == "single" {
        return None;
    }
    let facing = blocks::property(mw, p, "facing")?;
    let order = ["north", "east", "south", "west"];
    let i = order.iter().position(|f| *f == facing)?;
    let dir = order[if kind == "left" { (i + 1) % 4 } else { (i + 3) % 4 }];
    let (dx, dz) = match dir {
        "north" => (0, -1),
        "south" => (0, 1),
        "east" => (1, 0),
        _ => (-1, 0),
    };
    Some([p[0] + dx, p[1], p[2] + dz])
}

/// `ChestBlock.getContainer`: the chest's halves in slot order, none when either half is blocked.
fn container(mw: &MobWorld, p: [i32; 3]) -> Option<Vec<[i32; 3]>> {
    if blocked(mw, p) {
        return None;
    }
    let mut halves = vec![p];
    if let Some(other) = connected(mw, p) {
        let same = mw.world.block(other[0], other[1], other[2]) == mw.world.block(p[0], p[1], p[2])
            || mw.block_name(other[0], other[1], other[2]) == mw.block_name(p[0], p[1], p[2])
                && blocks::property(mw, other, "facing") == blocks::property(mw, p, "facing")
                && blocks::property(mw, other, "type").is_some_and(|t| t != "single")
                && blocks::property(mw, other, "type") != blocks::property(mw, p, "type");
        if same && matches!(mw.world.block_entities.get(&(other[0], other[1], other[2])), Some(BlockEntity::Chest(_))) {
            if blocked(mw, other) {
                return None;
            }
            if blocks::property(mw, p, "type").as_deref() == Some("right") {
                halves.push(other);
            } else {
                halves.insert(0, other);
            }
        }
    }
    Some(halves)
}

/// `PosKey`: a chest position in the `Mob`'s Java hash order.
#[derive(PartialEq, Eq)]
struct PosKey([i32; 3]);
impl JavaKey for PosKey {
    fn java_hash(&self) -> i32 {
        // Vec3i.hashCode.
        (self.0[1].wrapping_add(self.0[2].wrapping_mul(31))).wrapping_mul(31).wrapping_add(self.0[0])
    }
    fn tie_break(&self, _: &Self) -> std::cmp::Ordering {
        std::cmp::Ordering::Less
    }
}

/// `find_target`: the chunks around the golem's chunk that the search box reaches, each one's chests in Java hash order; the nearest
/// wanted one wins, a tie keeps the first found. The chunks are grouped from one scan of the block entities (no index of chests).
fn find_target(w: &World, e: Entity, mw: &MobWorld, me: Vec3) -> Option<Target> {
    let (bx, by, bz) = (me.x.floor() as i32, me.y.floor() as i32, me.z.floor() as i32);
    let (h, v) = (SEARCH_H, SEARCH_V);
    let visited = mem_list(w, e, &VISITED);
    let unreachable = mem_list(w, e, &UNREACHABLE);
    let (cx, cz) = (bx >> 4, bz >> 4);
    let r = h.div_euclid(16) + 1;
    let mut by_chunk: std::collections::HashMap<(i32, i32), Vec<[i32; 3]>> = Default::default();
    for (k, be) in &mw.world.block_entities {
        if matches!(be, BlockEntity::Chest(_)) {
            by_chunk.entry((k.0 >> 4, k.2 >> 4)).or_default().push([k.0, k.1, k.2]);
        }
    }
    let mut best: Option<(f64, Target)> = None;
    for z in cz - r..=cz + r {
        for x in cx - r..=cx + r {
            let Some(list) = by_chunk.get_mut(&(x, z)) else { continue };
            list.sort();
            let mut map: JavaHashMap<PosKey, ()> = JavaHashMap::default();
            for p in list.iter() {
                map.put(PosKey(*p), ());
            }
            for (key, _) in map.entries() {
                let p = key.0;
                let d = d2(me, Vec3::new(f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5));
                if best.as_ref().map_or(f64::from(f32::MAX), |b| b.0) <= d {
                    continue;
                }
                let inside = (bx - h..bx + 1 + h).contains(&p[0]) && (by - v..by + 1 + v).contains(&p[1]) && (bz - h..bz + 1 + h).contains(&p[2]);
                if !inside {
                    continue;
                }
                let Some(halves) = container(mw, p) else { continue };
                let name = mw.block_name(p[0], p[1], p[2]);
                let seen = std::iter::once(p)
                    .chain(connected(mw, p).filter(|o| mw.world.block_entities.contains_key(&(o[0], o[1], o[2]))))
                    .any(|q| visited.contains(&q) || unreachable.contains(&q));
                if wanted(w, e, mw, name) && !seen {
                    best = Some((d, Target { pos: p, name, halves }));
                }
            }
        }
    }
    best.map(|b| b.1)
}

/// `isAnotherMobInteractingWithTarget`: either half has an opener.
fn another_interacting(w: &mut World, e: Entity, mw: &MobWorld, t: &Target) -> bool {
    let mut chests = vec![t.pos];
    if let Some(o) = connected(mw, t.pos) {
        if mw.world.block_entities.contains_key(&(o[0], o[1], o[2])) {
            chests.push(o);
        }
    }
    chests.iter().any(|p| openers(w, e, mw, *p) > 0)
}

/// `openers`: the golems holding the chest `p` (or its other half) open within the counter's range, the golem itself included.
fn openers(w: &mut World, e: Entity, mw: &MobWorld, p: [i32; 3]) -> usize {
    let area = opener_area(mw, p);
    let me = usize::from(has_open(w, e, mw, p));
    let mut others = w.query::<(Entity, &Carry, &Bounds)>();
    me + others.iter(w).filter(|(o, c, b)| *o != e && holds(mw, c.opened, p) && b.0.intersects(area)).count()
}

/// `hasContainerOpen`: its opened chest is `p` or `p`'s other half.
fn holds(mw: &MobWorld, opened: Option<[i32; 3]>, p: [i32; 3]) -> bool {
    opened.is_some_and(|o| o == p || connected(mw, o) == Some(p))
}

/// Whether the golem `e` lists the counter at `p`.
fn has_open(w: &World, e: Entity, mw: &MobWorld, p: [i32; 3]) -> bool {
    w.get::<Carry>(e).is_some_and(|c| holds(mw, c.opened, p)) && w.get::<Bounds>(e).is_some_and(|b| b.0.intersects(opener_area(mw, p)))
}

/// The box a golem opens a chest from (`ChestBlockEntity.getOpenersAround`): the counter's range plus 4.
fn opener_area(mw: &MobWorld, p: [i32; 3]) -> Aabb {
    let range = mw.world.chest_openers.get(&(p[0], p[1], p[2])).map_or(0.0, |o| o.1) + 4.0;
    Aabb::new(
        Vec3::new(f64::from(p[0]) - range, f64::from(p[1]) - range, f64::from(p[2]) - range),
        Vec3::new(f64::from(p[0]) + 1.0 + range, f64::from(p[1]) + 1.0 + range, f64::from(p[2]) + 1.0 + range),
    )
}

/// `ChestBlockEntity.startOpen` / `stopOpen` by a golem: the lid event; the hinge sound of the block.
fn open_or_close(w: &mut World, mw: &MobWorld, p: [i32; 3], open: bool, e: Entity) {
    let sound = chest_hinge_sound(mw.block_name(p[0], p[1], p[2]), open).trim_start_matches("minecraft:");
    let mut events = w.resource_mut::<Events>();
    events.host.chest_open(p, open, e);
    events.host.sound(sound, [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5], 0.5, 1.0);
}

fn walk_towards(w: &mut World, e: Entity, t: &Target) {
    // BehaviorUtils.setWalkAndLookTargetMemories(body, pos, 1, 0).
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Block(t.pos)), None);
    set(w, e, &WALK_TARGET, MemValue::Walk(WalkTarget { target: Tracker::Block(t.pos), speed: 1.0, close: 0 }), None);
}

/// `reached_interaction`: what the golem does at the chest, by its hand and the chest's items.
fn reached_interaction(w: &World, e: Entity, mw: &MobWorld, halves: &[[i32; 3]]) -> Interaction {
    let items = slots(mw, halves);
    if main_hand(w, e).is_none() {
        if items.iter().any(Option::is_some) {
            Interaction::PickupItem
        } else {
            Interaction::PickupNoItem
        }
    } else if leaving_ok(w, e, &items) {
        Interaction::PlaceItem
    } else {
        Interaction::PlaceNoItem
    }
}

/// `on_reached`: a golem that drifted off leaves the visit; else the lid opens on the first tick, the sound comes on the 9th, the lid
/// shuts on the 60th (single chest), and the trade is made.
fn on_reached(w: &mut World, e: Entity, mw: &MobWorld, me: Vec3, size: Vec3, t: &Target) {
    if !within(mw, size, 2.0, t, centre_of(me, size)) {
        on_start_travelling(w, e);
        return;
    }
    let ticks = w.get_mut::<Carry>(e).map_or(0, |mut c| {
        c.ticks += 1;
        c.ticks
    });
    erase(w, e, &WALK_TARGET);
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Block(t.pos)), None);
    let interaction = w.get::<Carry>(e).and_then(|c| c.interaction);
    if let Some(i) = interaction {
        let (state, sound) = match i {
            Interaction::PickupItem => (cg::GETTING_ITEM, "entity.copper_golem.no_item_get"),
            Interaction::PickupNoItem => (cg::GETTING_NO_ITEM, "entity.copper_golem.no_item_no_get"),
            Interaction::PlaceItem => (cg::DROPPING_ITEM, "entity.copper_golem.item_drop"),
            Interaction::PlaceNoItem => (cg::DROPPING_NO_ITEM, "entity.copper_golem.item_no_drop"),
        };
        if ticks == 1 {
            for h in &t.halves {
                open_or_close(w, mw, *h, true, e);
            }
            if let Some(mut c) = w.get_mut::<Carry>(e) {
                c.opened = Some(t.pos);
            }
            w.resource_mut::<Events>().host.golem_state(e, state);
        }
        if ticks == 9 {
            w.resource_mut::<Events>().host.sound(sound, [me.x, me.y, me.z], 1.0, 1.0);
        }
        if ticks == WORK_TICKS {
            if t.halves.len() == 1 && has_open(w, e, mw, t.pos) {
                open_or_close(w, mw, t.pos, false, e);
            }
            if let Some(mut c) = w.get_mut::<Carry>(e) {
                c.opened = None;
            }
        }
    }
    if ticks >= WORK_TICKS {
        let items = slots(mw, &t.halves);
        if main_hand(w, e).is_none() {
            if items.iter().any(Option::is_some) {
                pick_up(w, e, mw, &t.halves, &items);
            } else {
                stop_targeting(w, e);
            }
        } else if leaving_ok(w, e, &items) {
            put_down(w, e, mw, &t.halves, items);
        } else {
            stop_targeting(w, e);
        }
        on_start_travelling(w, e);
    }
}

/// The slots of the halves, one after the other.
fn slots(mw: &MobWorld, halves: &[[i32; 3]]) -> Vec<Option<ItemStack>> {
    halves.iter().filter_map(|h| mw.world.block_entities.get(&(h[0], h[1], h[2]))).flat_map(|b| b.slots().to_vec()).collect()
}

/// Where slot `n` of the container is: the half and its own slot.
fn slot_at(mw: &MobWorld, halves: &[[i32; 3]], mut n: usize) -> Option<([i32; 3], usize)> {
    for h in halves {
        let len = mw.world.block_entities.get(&(h[0], h[1], h[2]))?.slots().len();
        if n < len {
            return Some((*h, n));
        }
        n -= len;
    }
    None
}

fn max_stack(mw: &MobWorld, s: &ItemStack) -> u32 {
    mw.items.and_then(|i| i.get(&s.item)).map_or(64, |i| i.max_stack).max(1)
}

/// `pickUpItems`: up to 16 of the first non-empty slot; the trip's memories are cleared.
fn pick_up(w: &mut World, e: Entity, mw: &MobWorld, halves: &[[i32; 3]], items: &[Option<ItemStack>]) {
    if let Some((n, s)) = items.iter().enumerate().find_map(|(n, s)| s.as_ref().map(|s| (n, s.clone()))) {
        let take = s.count.min(16);
        let mut left = s.clone();
        left.count -= take;
        let mut got = s;
        got.count = take;
        if let Some((h, slot)) = slot_at(mw, halves, n) {
            w.resource_mut::<Events>().host.chest_slot(h, slot, (left.count > 0).then_some(left));
        }
        set_main_hand(w, e, Some(got));
    }
    clear_memories(w, e);
}

/// `putDownItem`: the carried stack fills the matching slots up to their room, then the first empty one; the trip's memories are
/// cleared when the hand is empty, else the trip stops at this chest.
fn put_down(w: &mut World, e: Entity, mw: &MobWorld, halves: &[[i32; 3]], items: Vec<Option<ItemStack>>) {
    let mut carried = main_hand(w, e).cloned();
    for (n, slot) in items.into_iter().enumerate() {
        let Some(stack) = carried.clone() else { break };
        let Some((h, at)) = slot_at(mw, halves, n) else { break };
        match slot {
            None => {
                w.resource_mut::<Events>().host.chest_slot(h, at, Some(stack));
                carried = None;
                break;
            }
            Some(mut there) => {
                let max = max_stack(mw, &there);
                if there.item == stack.item && there.extra == stack.extra && there.damage == stack.damage && there.count < max {
                    let room = max - there.count;
                    there.count += room.min(stack.count);
                    w.resource_mut::<Events>().host.chest_slot(h, at, Some(there));
                    let left = i64::from(stack.count) - i64::from(room);
                    if left <= 0 {
                        carried = None;
                        break;
                    }
                    let mut rest = stack;
                    rest.count = left as u32;
                    carried = Some(rest);
                }
            }
        }
    }
    let emptied = carried.is_none();
    set_main_hand(w, e, carried);
    if emptied {
        clear_memories(w, e);
    } else {
        stop_targeting(w, e);
    }
}

/// `leaving_ok`: an empty chest, or one holding the carried item.
fn leaving_ok(w: &World, e: Entity, items: &[Option<ItemStack>]) -> bool {
    let carried = main_hand(w, e);
    items.iter().all(Option::is_none) || items.iter().flatten().any(|s| carried.is_some_and(|c| c.item == s.item))
}
