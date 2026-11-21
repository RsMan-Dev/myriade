//! `Transport` (`CopperGolemAi`, `transport_items.rs`, the copper golem's idle activity at priority 0): with an empty hand the golem takes
//! the items of a copper chest, else it puts what it holds into a chest. It walks to the chest of its search box (32 blocks sideways, 8 up
//! and down) that the `Mob` picks first, queues behind a golem already at it, works there for 60 ticks with the lid open, then keeps going.
//! The visited and unreachable chests are memories that expire 6000 ticks after their last change; a trip that finds no chest, or that
//! visits more than 10 (50 unreachable), enters the cooldown. The contents are read from the world's block entities; the changes, the lid
//! and the animation state are host events. Left out: the path search (a travel is tested with the path memory only, not a path end)
//! and the golem's leash. The chests come from `ChestIndex` (by chunk, kept by block events). The calculations are in `rules::transport_items`.
use crate::behaviours::Cap;
use crate::blocks;
use crate::chest_index::ChestIndex;
use crate::brain::*;
use crate::cow::{Bounds, Pos};
use crate::goals::{d2, main_hand, set_main_hand, Events};
use crate::rules::transport_items::{self as rules, Half, Interaction, COOLDOWN_TICKS, MEM_TTL};
use bevy_ecs::prelude::Component;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::copper_golem as cg;
use horizon_client_lib::mobs::transport_items::chest_hinge_sound;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::java_hash_map::JavaHashMap;

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

/// `Transport.State`: travelling, queuing behind another golem at the chest, working in front of it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum State {
    #[default]
    Travelling,
    Queuing,
    Interacting,
}

/// The trip's chest: its position, its name and its halves (two for a double chest, in slot order).
#[derive(Clone, Debug, Default)]
struct Target {
    pos: [i32; 3],
    name: String,
    halves: Vec<[i32; 3]>,
}

/// The trip's state. A component of the entity, as the door's state: a memory write would wake the brain at each step.
#[derive(Component, Clone, Debug, Default)]
#[component(on_remove = carry_removed)]
pub struct Carry {
    target: Option<Target>,
    state: State,
    interaction: Option<Interaction>,
    ticks: u32,
    opened: Option<[i32; 3]>,
}

/// A golem that goes (killed, despawned) with a chest open lets it go: the host's opener count drops by event, as `recheckOpeners` finds the
/// golem gone (no scan of the golems on the host side). The hinge sound needs the block's name, which a hook has no level to read.
fn carry_removed(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let Some(p) = world.get::<Carry>(ctx.entity).and_then(|c| c.opened) else { return };
    if let Some(mut events) = world.get_resource_mut::<Events>() {
        events.host.chest_open(p, false, ctx.entity);
    }
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
        let c = rules::centre_of(me, size);
        if rules::within(mw, size, 3.0, t.pos, c) && another_interacting(w, e, mw, &t) {
            erase(w, e, &WALK_TARGET);
            set_state(w, e, State::Queuing);
        } else if rules::within(mw, size, range(w, e), t.pos, c) {
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
    rules::reach(present(w, e, &PATH))
}

/// `CopperGolemAi.onTravelling`: the opened chest is forgotten and the animation is idle.
fn on_start_travelling(w: &mut World, e: Entity) {
    if let Some(mut c) = w.get_mut::<Carry>(e) {
        c.opened = None;
        c.state = State::Travelling;
        c.interaction = None;
        c.ticks = 0;
    }
    crate::behaviours::golem_state::set(w, e, cg::IDLE);
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
    match rules::with_visited(mem_list(w, e, &VISITED), p) {
        Some(list) => set(w, e, &VISITED, MemValue::Blocks(list), Some(MEM_TTL)),
        None => enter_cooldown(w, e),
    }
}

/// `mark_unreachable`: the chest leaves the visited list for the unreachable one.
fn mark_unreachable(w: &mut World, e: Entity, p: [i32; 3]) {
    match rules::mark_unreachable(mem_list(w, e, &VISITED), mem_list(w, e, &UNREACHABLE), p) {
        Some((visited, unreachable)) => {
            set(w, e, &VISITED, MemValue::Blocks(visited), Some(MEM_TTL));
            set(w, e, &UNREACHABLE, MemValue::Blocks(unreachable), Some(MEM_TTL));
        }
        None => enter_cooldown(w, e),
    }
}

/// `update_invalid_target`: a chest that is no longer valid is replaced by the nearest wanted one; true when the tick is spent on it.
fn update_invalid_target(w: &mut World, e: Entity, mw: &MobWorld, me: Vec3, size: Vec3) -> bool {
    if has_valid_target(w, e, mw, me, size) {
        return false;
    }
    stop_targeting(w, e);
    w.init_resource::<ChestIndex>();
    w.resource_mut::<ChestIndex>().ensure(mw);
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
    let unchanged = rules::is_chest_entity(mw, t.pos) && rules::is_chest_block(mw.block_name(t.pos[0], t.pos[1], t.pos[2]));
    if !(wanted(w, e, mw, &t.name) && unchanged) || rules::blocked(mw, t.pos) {
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

/// `valid_travelling_path`: the walk's path (the one followed, else one is searched as `create_path_to(pos, 0)` does), its end
/// the eye's place; no path and out of reach is invalid, and a reach needs a side seen.
fn valid_travelling_path(w: &World, e: Entity, mw: &MobWorld, me: Vec3, size: Vec3, t: &Target) -> bool {
    use horizon_client_lib::mobs::path::{Evaluator, Finder};
    let followed = w.get::<crate::cow::Nav>(e).and_then(|n| n.path.clone());
    let found = followed.or_else(|| {
        crate::behaviours::nearest_bed::path_mob(w, e).and_then(|(pm, follow)| {
            let max = follow.max(1.0);
            Finder::new(mw, &pm, Evaluator::Walk).find(&[t.pos], max, 0, (max * 16.0).floor() as i32)
        })
    });
    let no_path = found.is_none();
    let from = match found.as_ref().and_then(|p| p.nodes.last()) {
        Some(n) => rules::centre_of(Vec3::new(f64::from(n.x) + 0.5, f64::from(n.y), f64::from(n.z) + 0.5), size),
        None => rules::centre_of(me, size),
    };
    let reach = rules::within(mw, size, range(w, e), t.pos, from);
    rules::path_valid(no_path, reach, || rules::sees_a_side(mw, t.pos, from))
}

/// `wanted`: an empty hand takes from a copper chest, a full one puts into a chest.
fn wanted(w: &World, e: Entity, mw: &MobWorld, name: &str) -> bool {
    rules::wanted(mw, main_hand(w, e).is_none(), name)
}

/// `connected`: the other half of a double chest, from its `type` and `facing`.
fn connected(mw: &MobWorld, p: [i32; 3]) -> Option<[i32; 3]> {
    rules::other_half(blocks::property(mw, p, "type").as_deref(), blocks::property(mw, p, "facing").as_deref(), p)
}

/// `ChestBlock.getContainer`: the chest's halves in slot order, none when either half is blocked.
fn container(mw: &MobWorld, p: [i32; 3]) -> Option<Vec<[i32; 3]>> {
    if rules::blocked(mw, p) {
        return None;
    }
    let mut halves = vec![p];
    if let Some(other) = connected(mw, p) {
        let (p_kind, p_facing) = (blocks::property(mw, p, "type"), blocks::property(mw, p, "facing"));
        let (o_kind, o_facing) = (blocks::property(mw, other, "type"), blocks::property(mw, other, "facing"));
        let same_state = mw.world.block(other[0], other[1], other[2]) == mw.world.block(p[0], p[1], p[2]);
        let pair = rules::is_pair(
            same_state,
            Half { name: mw.block_name(p[0], p[1], p[2]), facing: p_facing.as_deref(), kind: p_kind.as_deref() },
            Half { name: mw.block_name(other[0], other[1], other[2]), facing: o_facing.as_deref(), kind: o_kind.as_deref() },
        );
        if pair && rules::is_chest_entity(mw, other) {
            if rules::blocked(mw, other) {
                return None;
            }
            if p_kind.as_deref() == Some("right") {
                halves.push(other);
            } else {
                halves.insert(0, other);
            }
        }
    }
    Some(halves)
}

/// `find_target`: the chunks around the golem's chunk that the search box reaches, each one's chests in Java hash order; the nearest
/// wanted one wins, a tie keeps the first found. The chests of a chunk come from `ChestIndex`.
fn find_target(w: &World, e: Entity, mw: &MobWorld, me: Vec3) -> Option<Target> {
    let b = [me.x.floor() as i32, me.y.floor() as i32, me.z.floor() as i32];
    let visited = mem_list(w, e, &VISITED);
    let unreachable = mem_list(w, e, &UNREACHABLE);
    let (cx, cz) = (b[0] >> 4, b[2] >> 4);
    let r = rules::chunk_radius();
    let index = w.get_resource::<ChestIndex>()?;
    let mut best: Option<(f64, Target)> = None;
    for z in cz - r..=cz + r {
        for x in cx - r..=cx + r {
            let mut list = index.chunk(x, z).to_vec();
            list.sort();
            let mut map: JavaHashMap<rules::ChestPos, ()> = JavaHashMap::default();
            for p in list.iter() {
                map.put(rules::ChestPos(*p), ());
            }
            for (key, _) in map.entries() {
                let p = key.0;
                let d = d2(me, Vec3::new(f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5));
                if best.as_ref().map_or(f64::from(f32::MAX), |b| b.0) <= d {
                    continue;
                }
                if !rules::in_search_box(p, b) {
                    continue;
                }
                let Some(halves) = container(mw, p) else { continue };
                let name = mw.block_name(p[0], p[1], p[2]);
                let seen = std::iter::once(p)
                    .chain(connected(mw, p).filter(|o| mw.world.block_entities.contains_key(&(o[0], o[1], o[2]))))
                    .any(|q| visited.contains(&q) || unreachable.contains(&q));
                if wanted(w, e, mw, name) && !seen {
                    best = Some((d, Target { pos: p, name: name.to_string(), halves }));
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
    rules::opener_area(p, mw.world.chest_openers.get(&(p[0], p[1], p[2])).map_or(0.0, |o| o.1))
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
    let items = rules::chest_slots(mw, halves);
    let carried = main_hand(w, e);
    rules::interaction(carried.is_none(), items.iter().any(Option::is_some), rules::leaving_ok(carried, &items))
}

/// `on_reached`: a golem that drifted off leaves the visit; else the lid opens on the first tick, the sound comes on the 9th, the lid
/// shuts on the 60th (single chest), and the trade is made.
fn on_reached(w: &mut World, e: Entity, mw: &MobWorld, me: Vec3, size: Vec3, t: &Target) {
    if !rules::within(mw, size, 2.0, t.pos, rules::centre_of(me, size)) {
        on_start_travelling(w, e);
        return;
    }
    let ticks = w.get_mut::<Carry>(e).map_or(0, |mut c| {
        c.ticks += 1;
        c.ticks
    });
    erase(w, e, &WALK_TARGET);
    set(w, e, &LOOK_TARGET, MemValue::Look(Tracker::Block(t.pos)), None);
    let phase = rules::phase(ticks);
    let interaction = w.get::<Carry>(e).and_then(|c| c.interaction);
    if let Some(i) = interaction {
        let (state, sound) = rules::feedback(i);
        if phase.open {
            for h in &t.halves {
                open_or_close(w, mw, *h, true, e);
            }
            if let Some(mut c) = w.get_mut::<Carry>(e) {
                c.opened = Some(t.pos);
            }
            crate::behaviours::golem_state::set(w, e, state);
        }
        if phase.sound {
            w.resource_mut::<Events>().host.sound(sound, [me.x, me.y, me.z], 1.0, 1.0);
        }
        if phase.shut {
            if t.halves.len() == 1 && has_open(w, e, mw, t.pos) {
                open_or_close(w, mw, t.pos, false, e);
            }
            if let Some(mut c) = w.get_mut::<Carry>(e) {
                c.opened = None;
            }
        }
    }
    if phase.trade {
        let items = rules::chest_slots(mw, &t.halves);
        if main_hand(w, e).is_none() {
            if items.iter().any(Option::is_some) {
                pick_up(w, e, mw, &t.halves, &items);
            } else {
                stop_targeting(w, e);
            }
        } else if rules::leaving_ok(main_hand(w, e), &items) {
            put_down(w, e, mw, &t.halves, &items);
        } else {
            stop_targeting(w, e);
        }
        on_start_travelling(w, e);
    }
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

/// `pickUpItems`: up to 16 of the first non-empty slot; the trip's memories are cleared.
fn pick_up(w: &mut World, e: Entity, mw: &MobWorld, halves: &[[i32; 3]], items: &[Option<ItemStack>]) {
    if let Some((n, got, left)) = rules::pick_up(items) {
        if let Some((h, slot)) = slot_at(mw, halves, n) {
            w.resource_mut::<Events>().host.chest_slot(h, slot, left);
        }
        set_main_hand(w, e, Some(got));
    }
    clear_memories(w, e);
}

/// `putDownItem`: the carried stack fills the matching slots up to their room, then the first empty one; the trip's memories are
/// cleared when the hand is empty, else the trip stops at this chest.
fn put_down(w: &mut World, e: Entity, mw: &MobWorld, halves: &[[i32; 3]], items: &[Option<ItemStack>]) {
    let (writes, carried) = rules::put_down(main_hand(w, e).cloned(), items, |s| rules::max_stack(mw, s));
    for (n, stack) in writes {
        if let Some((h, at)) = slot_at(mw, halves, n) {
            w.resource_mut::<Events>().host.chest_slot(h, at, Some(stack));
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
