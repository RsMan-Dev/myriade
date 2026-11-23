//! Copper golem chest trips (`brain/behaviours/transport_items.rs`): the search box, the reach, the double chests, the openers' box,
//! the visit's steps, the stack moves and the chest kinds. Values in, values out: the system reads the world and writes the result.
use myriade_engine_lib::containers::BlockEntity;
use myriade_engine_lib::items::ItemStack;
use myriade_engine_lib::mobs::copper_golem as cg;
use myriade_engine_lib::mobs::data::REDSTONE_CONDUCTOR;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::java_hash_map::JavaKey;

/// The visited and unreachable chests expire this many ticks after their last change.
pub const MEM_TTL: u64 = 6000;
/// The cooldown after a trip that found no chest, or too many.
pub const COOLDOWN_TICKS: i64 = 140;
pub const VISITED_MAX: usize = 10;
pub const UNREACHABLE_MAX: usize = 50;
/// The search box around the golem's block: sideways, then up and down.
pub const SEARCH_H: i32 = 32;
pub const SEARCH_V: i32 = 8;
/// The ticks spent in front of the chest before the trade.
pub const WORK_TICKS: u32 = 60;
const CHEST: &str = "minecraft:chest";
const TRAPPED_CHEST: &str = "minecraft:trapped_chest";
/// The block tag of the copper chests.
const COPPER_CHESTS: &str = "minecraft:copper_chests";

/// `Transport.Interaction`: what the golem does at the chest once it has reached it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interaction {
    PickupItem,
    PickupNoItem,
    PlaceItem,
    PlaceNoItem,
}

/// One half of a chest as the pairing sees it: its block name, and its `facing` and `type` properties.
#[derive(Clone, Copy, Debug)]
pub struct Half<'a> {
    pub name: &'a str,
    pub facing: Option<&'a str>,
    pub kind: Option<&'a str>,
}

/// The phase of a visit at tick `ticks` (1-based): the lid opens, the sound plays, the lid shuts, the trade is made.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phase {
    pub open: bool,
    pub sound: bool,
    pub shut: bool,
    pub trade: bool,
}

/// The reach of a travel: 0.5 while a path is followed, else 1.
pub fn reach(has_path: bool) -> f64 {
    if has_path { 0.5 } else { 1.0 }
}

/// The centre of the golem's box: the middle of its height.
pub fn centre_of(me: Vec3, size: Vec3) -> [f64; 3] {
    [me.x, me.y + size.y / 2.0, me.z]
}

/// The golem's box at `from`, grown by `distance` sideways and 0.5 up and down, meets the collision shape of the chest at `chest`.
pub fn within(mw: &MobWorld, size: Vec3, distance: f64, chest: [i32; 3], from: [f64; 3]) -> bool {
    let moved = Aabb::new(
        Vec3::new(from[0] - size.x / 2.0, from[1] - size.y / 2.0, from[2] - size.z / 2.0),
        Vec3::new(from[0] + size.x / 2.0, from[1] + size.y / 2.0, from[2] + size.z / 2.0),
    );
    let Some(shape) = mw.collision_bounds(chest) else { return false };
    let p = Vec3::new(f64::from(chest[0]), f64::from(chest[1]), f64::from(chest[2]));
    shape.inflate(Vec3::new(distance, 0.5, distance)).move_by(p).intersects(moved)
}

/// The chunks around the golem's chunk that the search box reaches, on each side.
pub fn chunk_radius() -> i32 {
    SEARCH_H.div_euclid(16) + 1
}

/// Whether the chest `p` lies in the search box around the golem's block `me`.
pub fn in_search_box(p: [i32; 3], me: [i32; 3]) -> bool {
    (me[0] - SEARCH_H..me[0] + 1 + SEARCH_H).contains(&p[0])
        && (me[1] - SEARCH_V..me[1] + 1 + SEARCH_V).contains(&p[1])
        && (me[2] - SEARCH_H..me[2] + 1 + SEARCH_H).contains(&p[2])
}

/// `wanted`: an empty hand takes from a copper chest, a full one puts into a chest.
pub fn wanted(mw: &MobWorld, empty_hand: bool, name: &str) -> bool {
    if empty_hand { mw.has_tag(COPPER_CHESTS, name) } else { matches!(name, CHEST | TRAPPED_CHEST) }
}

/// `CHEST` and `TRAPPED_CHEST` by name, and every copper chest.
pub fn is_chest_block(name: &str) -> bool {
    matches!(name, CHEST | TRAPPED_CHEST) || name.ends_with("copper_chest")
}

/// `ChestBlock.isChestBlockedAt`: a redstone conductor above the chest at `p`.
pub fn blocked(mw: &MobWorld, p: [i32; 3]) -> bool {
    let above = mw.world.block(p[0], p[1] + 1, p[2]);
    mw.data.state(above).flags & REDSTONE_CONDUCTOR != 0
}

/// `connected`: the other half of a double chest from its `type` and `facing` (`None` for a single chest or an unknown facing).
pub fn other_half(kind: Option<&str>, facing: Option<&str>, p: [i32; 3]) -> Option<[i32; 3]> {
    let kind = kind?;
    if kind == "single" {
        return None;
    }
    let facing = facing?;
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

/// Whether the chest `other` pairs with the chest `p` as a double chest: the same block state, or the same name and facing with another
/// non-single type.
pub fn is_pair(same_state: bool, p: Half, other: Half) -> bool {
    same_state || (p.name == other.name && p.facing == other.facing && other.kind.is_some_and(|t| t != "single") && other.kind != p.kind)
}

/// `PosKey`: a chest position in the `Mob`'s Java hash order.
#[derive(PartialEq, Eq)]
pub struct ChestPos(pub [i32; 3]);
impl JavaKey for ChestPos {
    fn java_hash(&self) -> i32 {
        // Vec3i.hashCode.
        (self.0[1].wrapping_add(self.0[2].wrapping_mul(31))).wrapping_mul(31).wrapping_add(self.0[0])
    }
    fn tie_break(&self, _: &Self) -> std::cmp::Ordering {
        std::cmp::Ordering::Less
    }
}

/// `ChestBlockEntity.getOpenersAround`: the box a golem opens a chest from, the counter's range (`counter`) plus 4.
pub fn opener_area(p: [i32; 3], counter: f64) -> Aabb {
    let range = counter + 4.0;
    Aabb::new(
        Vec3::new(f64::from(p[0]) - range, f64::from(p[1]) - range, f64::from(p[2]) - range),
        Vec3::new(f64::from(p[0]) + 1.0 + range, f64::from(p[1]) + 1.0 + range, f64::from(p[2]) + 1.0 + range),
    )
}

/// `reached_interaction`: what the golem does at the chest, by its empty hand, whether the chest holds an item, and `leaving_ok`.
pub fn interaction(empty_hand: bool, any_item: bool, leaving_ok: bool) -> Interaction {
    match (empty_hand, leaving_ok) {
        (true, _) if any_item => Interaction::PickupItem,
        (true, _) => Interaction::PickupNoItem,
        (false, true) => Interaction::PlaceItem,
        (false, false) => Interaction::PlaceNoItem,
    }
}

/// `leaving_ok`: an empty chest, or one holding the carried item.
pub fn leaving_ok(carried: Option<&ItemStack>, items: &[Option<ItemStack>]) -> bool {
    items.iter().all(Option::is_none) || items.iter().flatten().any(|s| carried.is_some_and(|c| c.item == s.item))
}

/// The golem state shown and the sound played for an interaction.
pub fn feedback(i: Interaction) -> (u8, &'static str) {
    match i {
        Interaction::PickupItem => (cg::GETTING_ITEM, "entity.copper_golem.no_item_get"),
        Interaction::PickupNoItem => (cg::GETTING_NO_ITEM, "entity.copper_golem.no_item_no_get"),
        Interaction::PlaceItem => (cg::DROPPING_ITEM, "entity.copper_golem.item_drop"),
        Interaction::PlaceNoItem => (cg::DROPPING_NO_ITEM, "entity.copper_golem.item_no_drop"),
    }
}

/// The visit's phase at tick `ticks`: the lid opens on the first, the sound on the 9th, the lid shuts and the trade is made on `WORK_TICKS`.
pub fn phase(ticks: u32) -> Phase {
    Phase { open: ticks == 1, sound: ticks == 9, shut: ticks == WORK_TICKS, trade: ticks >= WORK_TICKS }
}

/// `set_visited`: the list with `p` remembered; `None` past `VISITED_MAX`, which enters the cooldown.
pub fn with_visited(mut list: Vec<[i32; 3]>, p: [i32; 3]) -> Option<Vec<[i32; 3]>> {
    if !list.contains(&p) {
        list.push(p);
    }
    (list.len() <= VISITED_MAX).then_some(list)
}

/// `mark_unreachable`: the chest leaves the visited list for the unreachable one; `None` past `UNREACHABLE_MAX`, which enters the cooldown.
pub fn mark_unreachable(mut visited: Vec<[i32; 3]>, mut unreachable: Vec<[i32; 3]>, p: [i32; 3]) -> Option<(Vec<[i32; 3]>, Vec<[i32; 3]>)> {
    visited.retain(|q| *q != p);
    if !unreachable.contains(&p) {
        unreachable.push(p);
    }
    (unreachable.len() <= UNREACHABLE_MAX).then_some((visited, unreachable))
}

/// `valid_travelling_path`'s last test: no path and out of reach is invalid, and a reach needs a side seen (`sees_side` is asked only then).
pub fn path_valid(no_path: bool, reach: bool, sees_side: impl FnOnce() -> bool) -> bool {
    (no_path && !reach) || reach && sees_side()
}

/// `sees_a_side`: a clip from `eye` to the middle of any face of the chest's block at `p` hits that block.
pub fn sees_a_side(mw: &MobWorld, p: [i32; 3], eye: [f64; 3]) -> bool {
    let c = Vec3::new(f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5);
    // Direction order: down, up, north, south, west, east.
    let steps = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];
    let from = Vec3::new(eye[0], eye[1], eye[2]);
    steps.iter().any(|s| {
        let to = Vec3::new(c.x + 0.5 * f64::from(s[0]), c.y + 0.5 * f64::from(s[1]), c.z + 0.5 * f64::from(s[2]));
        mw.clip_block_at(from, to) == Some(p)
    })
}

/// `ItemStack.max_stack` of the stack's item, 64 when unknown, at least 1.
pub fn max_stack(mw: &MobWorld, s: &ItemStack) -> u32 {
    mw.items.and_then(|i| i.get(&s.item)).map_or(64, |i| i.max_stack).max(1)
}

/// `pickUpItems`: up to 16 of the first non-empty slot: its index, the stack taken, and what stays in the chest.
pub fn pick_up(items: &[Option<ItemStack>]) -> Option<(usize, ItemStack, Option<ItemStack>)> {
    let (n, s) = items.iter().enumerate().find_map(|(n, s)| s.as_ref().map(|s| (n, s.clone())))?;
    let take = s.count.min(16);
    let mut left = s.clone();
    left.count -= take;
    let mut got = s;
    got.count = take;
    Some((n, got, (left.count > 0).then_some(left)))
}

/// `putDownItem`: the carried stack fills the matching slots up to their room, then the first empty one. Returns the slot writes, by
/// slot index, and what the golem still carries.
pub fn put_down(carried: Option<ItemStack>, items: &[Option<ItemStack>], max_of: impl Fn(&ItemStack) -> u32) -> (Vec<(usize, ItemStack)>, Option<ItemStack>) {
    let mut writes = Vec::new();
    let mut carried = carried;
    for (n, slot) in items.iter().enumerate() {
        let Some(stack) = carried.clone() else { break };
        match slot {
            None => {
                writes.push((n, stack));
                carried = None;
                break;
            }
            Some(there) => {
                let mut there = there.clone();
                let max = max_of(&there);
                if there.item == stack.item && there.extra == stack.extra && there.damage == stack.damage && there.count < max {
                    let room = max - there.count;
                    there.count += room.min(stack.count);
                    writes.push((n, there));
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
    (writes, carried)
}

/// `BlockEntity` of a chest: its slots, or none when the block there is no chest entity.
pub fn chest_slots(mw: &MobWorld, halves: &[[i32; 3]]) -> Vec<Option<ItemStack>> {
    halves.iter().filter_map(|h| mw.world.block_entities.get(&(h[0], h[1], h[2]))).flat_map(|b| b.slots().to_vec()).collect()
}

/// Whether the position `p` holds a chest entity.
pub fn is_chest_entity(mw: &MobWorld, p: [i32; 3]) -> bool {
    matches!(mw.world.block_entities.get(&(p[0], p[1], p[2])), Some(BlockEntity::Chest(_)))
}
