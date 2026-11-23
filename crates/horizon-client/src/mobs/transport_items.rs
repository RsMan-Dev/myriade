//! `TransportItemsBetweenContainers` as the copper golem's brain runs it
//! (`CopperGolemAi`): find the nearest wanted chest (a copper chest with
//! nothing in hand, a chest or trapped chest holding something), walk to it,
//! queue while another golem has it open, open it for 60 ticks and take up
//! to 16 items or put the carried stack down, remembering visited and
//! unreachable chests.
//!
//! The chests are read from the world; what changes them comes out as
//! [`MobEvent::ChestOpen`] and [`MobEvent::ChestSlot`] for the host to apply
//! before the next mob ticks.
use super::{
    MobEvent, MobWorld,
    animal_brain::{Bx, Tracker, WalkTarget, mem, mem_ref},
    copper_golem as cg,
};
use crate::{containers::BlockEntity, items::ItemStack};
use horizon_core::java_hash_map::{JavaHashMap, JavaKey};
use horizon_core::{
    geometry::{Aabb, Vec3},
    random::RandomSource,
};

/// `TransportItemState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Travelling,
    Queuing,
    Interacting,
}
/// `ContainerInteractionState`.
#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Interaction {
    PickupItem,
    PickupNoItem,
    PlaceItem,
    PlaceNoItem,
}

/// `TransportItemTarget`: the chest and its block state when it was chosen.
#[derive(Clone, Debug)]
struct Target {
    pos: [i32; 3],
    name: String,
    /// The halves of the container in slot order (the right half first).
    halves: Vec<[i32; 3]>,
}

#[derive(Clone, Debug)]
pub(super) struct Transport {
    pub(super) running: bool,
    end: i64,
    target: Option<Target>,
    state: State,
    interaction: Option<Interaction>,
    ticks: i32,
}
impl Default for Transport {
    fn default() -> Self {
        Self {
            running: false,
            end: 0,
            target: None,
            state: State::Travelling,
            interaction: None,
            ticks: 0,
        }
    }
}


fn name_at(w: &MobWorld, p: [i32; 3]) -> String {
    w.facts
        .get(w.world.block(p[0], p[1], p[2]))
        .name
        .to_string()
}
/// `ChestBlock.getOpenChestSound` / `getCloseChestSound` of a chest block:
/// a copper chest's comes from `CopperChestBlock.getHingeSound`, by its
/// weather state (a waxed chest's is its state's).
pub fn chest_hinge_sound(block: &str, open: bool) -> &'static str {
    let base = block
        .trim_start_matches("minecraft:")
        .trim_start_matches("waxed_");
    match (base.ends_with("copper_chest"), base.split('_').next(), open) {
        (false, _, true) => "minecraft:block.chest.open",
        (false, _, false) => "minecraft:block.chest.close",
        (true, Some("weathered"), true) => "minecraft:block.copper_chest_weathered.open",
        (true, Some("weathered"), false) => "minecraft:block.copper_chest_weathered.close",
        (true, Some("oxidized"), true) => "minecraft:block.copper_chest_oxidized.open",
        (true, Some("oxidized"), false) => "minecraft:block.copper_chest_oxidized.close",
        (true, _, true) => "minecraft:block.copper_chest.open",
        (true, _, false) => "minecraft:block.copper_chest.close",
    }
}
fn is_chest_block(name: &str) -> bool {
    matches!(name, "minecraft:chest" | "minecraft:trapped_chest") || name.ends_with("copper_chest")
}
/// `ChestBlock.getConnectedBlockPos`: the other half of a double chest.
fn connected(w: &MobWorld, p: [i32; 3]) -> Option<[i32; 3]> {
    let kind = w.prop(p[0], p[1], p[2], "type")?;
    if kind == "single" {
        return None;
    }
    let facing = w.prop(p[0], p[1], p[2], "facing")?;
    // LEFT: facing.getClockWise(), else getCounterClockWise().
    let order = ["north", "east", "south", "west"];
    let i = order.iter().position(|f| *f == facing)?;
    let dir = order[if kind == "left" {
        (i + 1) % 4
    } else {
        (i + 3) % 4
    }];
    let (dx, dz) = match dir {
        "north" => (0, -1),
        "south" => (0, 1),
        "east" => (1, 0),
        _ => (-1, 0),
    };
    Some([p[0] + dx, p[1], p[2] + dz])
}
/// `ChestBlock.isChestBlockedAt` (a cat sitting on it aside): a redstone
/// conductor above.
fn blocked(w: &MobWorld, p: [i32; 3]) -> bool {
    let above = w.world.block(p[0], p[1] + 1, p[2]);
    w.data.state(above).flags & super::data::REDSTONE_CONDUCTOR != 0
}
/// `ChestBlock.getContainer(.., false)`: the chest's halves in slot order,
/// none when either half is blocked.
fn container(w: &MobWorld, p: [i32; 3]) -> Option<Vec<[i32; 3]>> {
    if blocked(w, p) {
        return None;
    }
    let mut halves = vec![p];
    if let Some(other) = connected(w, p) {
        let same = w.world.block(other[0], other[1], other[2]) == w.world.block(p[0], p[1], p[2])
            || name_at(w, other) == name_at(w, p)
                && w.prop(other[0], other[1], other[2], "facing")
                    == w.prop(p[0], p[1], p[2], "facing")
                && w.prop(other[0], other[1], other[2], "type")
                    .is_some_and(|t| t != "single")
                && w.prop(other[0], other[1], other[2], "type") != w.prop(p[0], p[1], p[2], "type");
        if same
            && matches!(
                w.world.block_entities.get(&(other[0], other[1], other[2])),
                Some(BlockEntity::Chest(_))
            )
        {
            if blocked(w, other) {
                return None;
            }
            if w.prop(p[0], p[1], p[2], "type") == Some("right") {
                halves.push(other);
            } else {
                halves.insert(0, other);
            }
        }
    }
    Some(halves)
}
fn slots(w: &MobWorld, halves: &[[i32; 3]]) -> Vec<Option<ItemStack>> {
    halves
        .iter()
        .filter_map(|h| w.world.block_entities.get(&(h[0], h[1], h[2])))
        .flat_map(|e| e.slots().to_vec())
        .collect()
}
/// Where slot `n` of the container is: the half and its own slot.
fn slot_at(w: &MobWorld, halves: &[[i32; 3]], mut n: usize) -> Option<([i32; 3], usize)> {
    for h in halves {
        let len = w
            .world
            .block_entities
            .get(&(h[0], h[1], h[2]))?
            .slots()
            .len();
        if n < len {
            return Some((*h, n));
        }
        n -= len;
    }
    None
}
fn max_stack(w: &MobWorld, s: &ItemStack) -> u32 {
    w.items
        .and_then(|i| i.get(&s.item))
        .map_or(64, |i| i.max_stack)
        .max(1)
}

/// `BlockPos` keys in a chunk's block entity map.
#[derive(PartialEq, Eq)]
struct PosKey([i32; 3]);
impl JavaKey for PosKey {
    fn java_hash(&self) -> i32 {
        // Vec3i.hashCode.
        (self.0[1].wrapping_add(self.0[2].wrapping_mul(31)))
            .wrapping_mul(31)
            .wrapping_add(self.0[0])
    }
    fn tie_break(&self, _: &Self) -> std::cmp::Ordering {
        std::cmp::Ordering::Less
    }
}

/// `CopperGolem.hasContainerOpen`: its opened chest is `p` or `p`'s other half.
fn holds(w: &MobWorld, opened: Option<[i32; 3]>, p: [i32; 3]) -> bool {
    opened.is_some_and(|o| o == p || connected(w, o) == Some(p))
}
fn opener_area(w: &MobWorld, p: [i32; 3]) -> Aabb {
    let range = w
        .world
        .chest_openers
        .get(&(p[0], p[1], p[2]))
        .map_or(0.0, |o| o.1)
        + 4.0;
    Aabb::new(
        Vec3::new(
            f64::from(p[0]) - range,
            f64::from(p[1]) - range,
            f64::from(p[2]) - range,
        ),
        Vec3::new(
            f64::from(p[0]) + 1.0 + range,
            f64::from(p[1]) + 1.0 + range,
            f64::from(p[2]) + 1.0 + range,
        ),
    )
}
/// `ChestBlockEntity.startOpen` / `stopOpen` by a golem (interaction range
/// 3): the opener count changes and the first opener or the last closer
/// plays the chest's sound (`ChestBlockEntity.playSound`, at the middle of
/// a double chest from its right half, none from its left).
fn open_or_close(cx: &mut Bx, p: [i32; 3], open: bool, mob: u32) {
    let w = cx.w;
    let count = w
        .world
        .chest_openers
        .get(&(p[0], p[1], p[2]))
        .map_or(0, |o| o.0);
    let sounds = if open { count == 0 } else { count == 1 };
    if sounds
        && let Some(kind) = w.prop(p[0], p[1], p[2], "type")
        && kind != "left"
    {
        let mut at = [
            f64::from(p[0]) + 0.5,
            f64::from(p[1]) + 0.5,
            f64::from(p[2]) + 0.5,
        ];
        if kind == "right"
            && let Some(o) = connected(w, p)
        {
            at[0] += f64::from(o[0] - p[0]) * 0.5;
            at[2] += f64::from(o[2] - p[2]) * 0.5;
        }
        let sound = chest_hinge_sound(&name_at(w, p), open).trim_start_matches("minecraft:");
        let pitch = cx.level.random().next_float() * 0.1 + 0.9;
        cx.events.push(MobEvent::Sound(sound, at, 0.5, pitch));
    }
    cx.events.push(MobEvent::ChestOpen {
        chest: p,
        open,
        mob,
    });
}

impl Transport {
}
/// `canSeeAnyTargetSide`: a clip from `eye` to the middle of any face of the
/// chest's block hits that block.
fn sees_a_side(w: &MobWorld, p: [i32; 3], eye: [f64; 3]) -> bool {
    let c = Vec3::new(
        f64::from(p[0]) + 0.5,
        f64::from(p[1]) + 0.5,
        f64::from(p[2]) + 0.5,
    );
    // Direction order: down, up, north, south, west, east.
    let steps = [
        [0, -1, 0],
        [0, 1, 0],
        [0, 0, -1],
        [0, 0, 1],
        [-1, 0, 0],
        [1, 0, 0],
    ];
    let from = Vec3::new(eye[0], eye[1], eye[2]);
    steps.iter().any(|s| {
        let to = Vec3::new(
            c.x + 0.5 * f64::from(s[0]),
            c.y + 0.5 * f64::from(s[1]),
            c.z + 0.5 * f64::from(s[2]),
        );
        w.clip_block_at(from, to) == Some(p)
    })
}
