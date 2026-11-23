//! Villagers (`Villager`): professions, sleeping, the unhappy head shake,
//! food, gossip and the raid state. Their AI is the brain in
//! `villager_brain`.
use super::{
    MobEvent, MobWorld, Species,
    anim::{Poses, State},
};
use crate::items::{ItemStack, Items};
use horizon_core::geometry::trig::Mth;
use horizon_core::random::RandomSource;
use std::f32::consts::PI;

// Villager state: `variant` type, `profession` identity (index into
// the frozen profession table), `villager_level` level, `home` the claimed job site,
// `bed` and `meeting` the claimed bed head and bell, `ints[0]` the unhappy
// counter, `ints[1]` the bed's facing while asleep (1 north, 2 south,
// 3 west, 4 east), `trade` the offers, experience and restocks.

pub const NONE: u8 = 0;
pub const NITWIT: u8 = 11;

#[path = "villager_professions.rs"]
pub mod professions;

pub use professions::{Profession, ResolvedProfessions};
#[path = "villager_identity.rs"]
mod identity;
pub use identity::profession_id;
pub use identity::reported_profession;
pub(crate) use identity::{command_name, seen_profession_name};

/// The frozen vanilla-only generation shared by the existing context-free consumers.
pub fn profession_table() -> &'static ResolvedProfessions {
    static TABLE: std::sync::OnceLock<ResolvedProfessions> = std::sync::OnceLock::new();
    TABLE.get_or_init(ResolvedProfessions::vanilla)
}

/// Existing u8 adapter; invalid values retain the historic wrapping behavior.
pub fn profession(index: u8) -> &'static Profession {
    profession_table().get(index)
}

/// A vanilla profession or an unresolved saved resource identity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ProfessionState {
    #[default]
    None,
    Vanilla(u8),
    Unresolved(String),
    Registered {
        id: std::sync::Arc<str>,
        slot: u16,
    },
}

impl ProfessionState {
    pub fn from_index(index: u8) -> Self {
        if index == NONE {
            Self::None
        } else {
            Self::Vanilla(index)
        }
    }

    pub fn from_id(id: &str) -> Self {
        let vanilla = id
            .strip_prefix("minecraft:")
            .or_else(|| (!id.contains(':')).then_some(id));
        vanilla
            .and_then(profession_index)
            .map_or_else(|| Self::Unresolved(id.into()), Self::from_index)
    }

    pub fn index(&self) -> Option<u8> {
        match self {
            Self::None => Some(NONE),
            Self::Vanilla(index) => Some(*index),
            Self::Unresolved(_) | Self::Registered { .. } => None,
        }
    }

    pub fn id(&self) -> String {
        match self {
            Self::None => "minecraft:none".into(),
            Self::Vanilla(index) => format!("minecraft:{}", profession(*index).id.as_str()),
            Self::Unresolved(id) => id.clone(),
            Self::Registered { id, .. } => id.to_string(),
        }
    }
}

/// Finds the existing storage index for a bare profession name.
pub fn profession_index(name: &str) -> Option<u8> {
    profession_table().find(name)
}


/// The `villager_activity` / `baby_villager_activity` timeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Activity {
    Idle,
    Work,
    Play,
    Meet,
    Rest,
}
pub fn activity_at(baby: bool, day_time: i64) -> Activity {
    let t = day_time.rem_euclid(24000);
    if baby {
        match t {
            10..3000 | 6000..10000 => Activity::Idle,
            3000..6000 | 10000..12000 => Activity::Play,
            _ => Activity::Rest,
        }
    } else {
        match t {
            10..2000 | 11000..12000 => Activity::Idle,
            2000..9000 => Activity::Work,
            9000..11000 => Activity::Meet,
            _ => Activity::Rest,
        }
    }
}
/// `LivingEntityRenderer.sleepDirectionToRotation` and the bed's direction
/// (the body is shifted back from the pillow by the standing eye height).
/// A bed's `facing` as `sleep_pose` keeps it in `ints[1]`.
pub fn bed_facing_code(facing: Option<&str>) -> i32 {
    match facing {
        Some("north") => 1,
        Some("south") => 2,
        Some("west") => 3,
        _ => 4,
    }
}

// ---- Raids: the raid activities and the gift for a hero.

/// No raid where the villager stands (`ServerLevel.getRaidAt` is null).
pub const RAID_NONE: u8 = 0;
/// A raid is near, before or between its waves.
pub const PRE_RAID: u8 = 1;
/// A raid is near and a wave is on.
pub const RAID: u8 = 2;
/// The raid there was won: the villagers celebrate.
pub const RAID_VICTORY: u8 = 3;
/// The raid there was lost (still found by `getRaidAt` until it stops).
pub const RAID_LOSS: u8 = 4;

/// Villager state beyond `Extra`'s scratch fields.
#[derive(Clone, Debug, Default)]
pub struct Ext {
    /// The raid `getRaidAt` finds where the villager stands (`RAID_NONE`
    /// ... `RAID_LOSS`), set by the game every tick.
    pub raid: u8,
    /// That raid has spawned a wave and is not between waves
    /// (`hasFirstWaveSpawned && !isBetweenWaves`).
    pub raid_wave: bool,
    /// What the villager remembers of players (`gossips`).
    pub gossips: crate::gossip::Gossips,
    /// `Villager.lastTradedPlayer`: the most recent trader, consumed by the
    /// next customServerAiStep. This is runtime state, not saved gossip.
    pub last_traded_player: Option<[i32; 4]>,
    pub last_gossip_time: i64,
    pub last_gossip_decay: i64,
    /// `foodLevel`: eaten but not yet digested, in food points.
    pub food_level: i32,
    /// The eight-slot inventory (`InventoryCarrier`).
    pub inventory: Vec<ItemStack>,
    /// `LAST_SLEPT` memory.
    pub last_slept: Option<i64>,
    /// `LivingEntity.getSleepingPos`: the bed head it lies in, saved as
    /// `sleeping_pos`.
    pub sleeping_pos: Option<[i32; 3]>,
    /// `GOLEM_DETECTED_RECENTLY` expires at this game time.
    pub golem_until: i64,
    /// The `last_hurt_by_time` of the player's hit already counted.
    pub hurt_seen: i32,
    /// A zombie villager's cure under way.
    pub cure: Option<super::zombie_villager::Cure>,
    /// `CanContinueSleepingEvent.mayContinueSleeping`, set by the game before the tick
    /// when a native mod may overwrite the bed check; empty means vanilla's.
    pub may_continue_sleeping: Option<bool>,
    /// `ZombieVillager.villagerDataFinalized`: only a save with
    /// `VillagerDataFinalized` or `VillagerData` sets it.
    pub data_finalized: bool,
    /// The villager's brain, built at its first tick.
    pub brain: Option<Box<super::villager_brain::Brain>>,
    /// Saved memories awaiting the first brain initialization.
    pub saved_memories: Option<Box<super::villager_brain::Memories>>,
    /// `[entities] villager_lobotomize`: whether the last check found the villager stuck
    /// (`villager_lobotomy`). Not saved; `None` until the first check.
    pub lobotomized: Option<bool>,
}
pub mod lobotomy;

/// `VillagerProfession.workSound`.
pub fn work_sound(index: u8) -> Option<&'static str> {
    Some(match profession(index).id.as_str() {
        "armorer" => "entity.villager.work_armorer",
        "butcher" => "entity.villager.work_butcher",
        "cartographer" => "entity.villager.work_cartographer",
        "cleric" => "entity.villager.work_cleric",
        "farmer" => "entity.villager.work_farmer",
        "fisherman" => "entity.villager.work_fisherman",
        "fletcher" => "entity.villager.work_fletcher",
        "leatherworker" => "entity.villager.work_leatherworker",
        "librarian" => "entity.villager.work_librarian",
        "mason" => "entity.villager.work_mason",
        "shepherd" => "entity.villager.work_shepherd",
        "toolsmith" => "entity.villager.work_toolsmith",
        "weaponsmith" => "entity.villager.work_weaponsmith",
        _ => return None,
    })
}
/// A `CropBlock` at `p`.
pub fn crop(w: &MobWorld, p: [i32; 3]) -> bool {
    w.facts
        .get(w.world.block(p[0], p[1], p[2]))
        .has_class("CropBlock")
}
/// `CropBlock.isMaxAge` for the block at `p`.
pub fn ripe_crop(w: &MobWorld, p: [i32; 3]) -> bool {
    let facts = w.facts.get(w.world.block(p[0], p[1], p[2]));
    if !facts.has_class("CropBlock") {
        return false;
    }
    // The class chain survives content registration, including the two
    // getMaxAge overrides. PitcherCropBlock is not a CropBlock.
    let max = if facts.has_class("BeetrootBlock") {
        3
    } else if facts.has_class("TorchflowerCropBlock") {
        2
    } else {
        7
    };
    w.prop(p[0], p[1], p[2], "age")
        .and_then(|age| age.parse::<i32>().ok())
        .is_some_and(|age| age >= max)
}

// ---- Gossip, food, breeding and golems.

/// `VillagerFood` nutrition (`DataComponents.VILLAGER_FOOD`).
///
/// The value in `items`, the registry the villager lives with: a data pack's
/// or a mod's item with the component counts, as one with vanilla's four does;
/// an item the registry lacks feeds nothing.
pub fn food_points(items: &Items, item: &str) -> i32 {
    items.villager_food(item).unwrap_or(0)
}
/// The farmer's `requestedItems` (`VillagerProfession.FARMER`). Food is picked
/// up and traded separately, through its villager food value.
pub fn farmer_items() -> &'static [&'static str] {
    &[
        "minecraft:wheat",
        "minecraft:wheat_seeds",
        "minecraft:beetroot_seeds",
        "minecraft:bone_meal",
    ]
}
/// `SimpleContainer(8)`: stacks the villager carries.
const INVENTORY_SLOTS: usize = 8;

/// `SimpleContainer.addItem` on a container of `slots` slots held as its
/// occupied stacks in slot order: fill the stacks of the same item and
/// components below `max_stack`, then free slots; the part that did not fit
/// is returned.
pub(crate) fn add_to_container(
    inventory: &mut Vec<ItemStack>,
    mut stack: ItemStack,
    max_stack: u32,
    slots: usize,
) -> Option<ItemStack> {
    for s in inventory.iter_mut() {
        if stack.count > 0 && s.same_item_same_components(&stack) && s.count < max_stack {
            let n = (max_stack - s.count).min(stack.count);
            s.count += n;
            stack.count -= n;
        }
    }
    while stack.count > 0 && inventory.len() < slots {
        let n = stack.count.min(max_stack);
        inventory.push(ItemStack {
            count: n,
            ..stack.clone()
        });
        stack.count -= n;
    }
    (stack.count > 0).then_some(stack)
}


impl Ext {
    /// `countFoodPointsInInventory`, by the food values of `items`.
    pub fn food_in_inventory(&self, items: &Items) -> i32 {
        self.inventory
            .iter()
            .map(|s| food_points(items, &s.item) * s.count as i32)
            .sum()
    }
    /// `hasExcessFood` and `wantsMoreFood`.
    pub fn has_excess_food(&self, items: &Items) -> bool {
        self.food_in_inventory(items) >= 24
    }
    pub fn wants_more_food(&self, items: &Items) -> bool {
        self.food_in_inventory(items) < 12
    }
    fn hungry(&self) -> bool {
        self.food_level < 12
    }
    /// `Villager.eatAndDigestFood`: eat until full, then digest 12 points.
    pub fn eat_and_digest(&mut self, items: &Items) {
        if self.hungry() {
            let mut slot = 0;
            while slot < self.inventory.len() && self.hungry() {
                let points = food_points(items, &self.inventory[slot].item);
                if points > 0 {
                    while self.inventory[slot].count > 0 && self.hungry() {
                        self.food_level += points;
                        self.inventory[slot].count -= 1;
                    }
                    if self.inventory[slot].count == 0 {
                        self.inventory.remove(slot);
                        continue;
                    }
                }
                slot += 1;
            }
        }
        self.food_level -= 12;
    }
    /// Whether the stack fits (`SimpleContainer.canAddItem`).
    pub fn can_add(&self, stack: &ItemStack, max_stack: u32) -> bool {
        self.inventory.len() < INVENTORY_SLOTS
            || self
                .inventory
                .iter()
                .any(|s| s.same_item_same_components(stack) && s.count < max_stack)
    }
    /// `SimpleContainer.addItem`: merge into stacks, then free slots; the
    /// part that did not fit is returned.
    pub fn add_item(&mut self, stack: ItemStack, max_stack: u32) -> Option<ItemStack> {
        add_to_container(&mut self.inventory, stack, max_stack, INVENTORY_SLOTS)
    }
    /// `SimpleContainer.removeItemType`: take up to `count` of the item from
    /// the first stacks that hold it.
    pub fn remove_item(&mut self, item: &str, mut count: u32) {
        let mut slot = 0;
        while slot < self.inventory.len() && count > 0 {
            let s = &mut self.inventory[slot];
            if &*s.item == item {
                let n = s.count.min(count);
                s.count -= n;
                count -= n;
                if s.count == 0 {
                    self.inventory.remove(slot);
                    continue;
                }
            }
            slot += 1;
        }
    }
    pub fn count_item(&self, item: &str) -> u32 {
        self.inventory
            .iter()
            .filter(|s| &*s.item == item)
            .map(|s| s.count)
            .sum()
    }
    /// `TradeWithVillager.throwHalfStack`: the half of a big matching stack
    /// (or whatever is over 24) leaves the inventory.
    pub fn take_half_stack(
        &mut self,
        wanted: impl Fn(&ItemStack) -> bool,
        max_stack: impl Fn(&str) -> u32,
    ) -> Option<ItemStack> {
        for i in 0..self.inventory.len() {
            let s = &self.inventory[i];
            if s.count == 0 || !wanted(s) {
                continue;
            }
            let count = if s.count > max_stack(&s.item) / 2 {
                s.count / 2
            } else if s.count > 24 {
                s.count - 24
            } else {
                continue;
            };
            let taken = ItemStack { count, ..s.clone() };
            self.inventory[i].count -= count;
            if self.inventory[i].count == 0 {
                self.inventory.remove(i);
            }
            return Some(taken);
        }
        None
    }
    /// `Villager.wantsToSpawnGolem`: slept within the last day and no golem
    /// seen lately.
    pub fn wants_golem(&self, now: i64) -> bool {
        self.last_slept.is_some_and(|t| now - t < 24000) && now >= self.golem_until
    }
    /// `maybeDecayGossip`: once a day.
    pub fn maybe_decay(&mut self, now: i64) {
        if self.last_gossip_decay == 0 {
            self.last_gossip_decay = now;
        } else if now >= self.last_gossip_decay + 24000 {
            self.gossips.decay();
            self.last_gossip_decay = now;
        }
    }
    /// `Villager.gossip` timing: hearing again waits 1200 ticks.
    pub fn can_gossip(&self, now: i64) -> bool {
        now < self.last_gossip_time || now >= self.last_gossip_time + 1200
    }
}
/// What a villager ticking later reads of another one this tick (vanilla
/// reads the other villager itself).
#[derive(Clone, Debug, Default)]
pub struct Seen {
    /// Visibility inputs for another villager's noncombat sensor.
    pub invisible: bool,
    pub crouching: bool,
    pub armor: [Option<std::sync::Arc<str>>; 4],
    pub gossips: crate::gossip::Gossips,
    pub last_gossip_time: i64,
    /// `countFoodPointsInInventory`.
    pub food_points: i32,
    /// The `INTERACTION_TARGET` memory (play tag's chases).
    pub interaction_target: Option<super::Who>,
    /// The `PATH` memory, when not done: its previous and next nodes.
    pub path_nodes: Option<(Option<[i32; 3]>, [i32; 3])>,
    /// `LAST_SLEPT` and whether `GOLEM_DETECTED_RECENTLY` is remembered.
    pub last_slept: Option<i64>,
    pub golem_detected: bool,
}
impl Seen {
    /// `LivingEntity.getVisibilityPercent`, for a targeting villager.
    pub fn visibility(&self, items: Option<&Items>) -> f64 {
        let mut value = if self.crouching { 0.8 } else { 1.0 };
        if self.invisible {
            let cover = self.armor.iter().filter(|i| i.is_some()).count() as f32 / 4.0;
            value *= 0.7 * f64::from(cover).max(0.1);
        }
        if let Some(items) = items {
            for (slot, item) in ["feet", "legs", "chest", "head"]
                .into_iter()
                .zip(&self.armor)
            {
                if let Some(factor) = item
                    .as_deref()
                    .and_then(|i| items.worn_visibility(i, slot, "minecraft:villager"))
                {
                    value *= f64::from(factor);
                }
            }
        }
        if value < 0.0 { 0.0 } else { value.min(10.0) }
    }
    /// `Villager.wantsToSpawnGolem`.
    pub fn wants_golem(&self, now: i64) -> bool {
        self.last_slept.is_some_and(|t| now - t < 24000) && !self.golem_detected
    }
    /// `Villager.wantsMoreFood`.
    pub fn wants_more_food(&self) -> bool {
        self.food_points < 12
    }
}
/// What `TradeWithVillager.tick` hands over: stacks tossed from the first
/// villager's hand (`from`, its eye height) toward the second (`to`), and
/// whether the gossip happened (`spawnGolemIfNeeded` follows).
pub struct Meeting {
    pub thrown: Vec<ItemStack>,
    pub from: horizon_core::geometry::Vec3,
    pub eye: f64,
    pub to: horizon_core::geometry::Vec3,
    pub gossiped: bool,
    /// How many gossips `GossipContainer.transferFrom` picked up.
    pub new_gossip: usize,
}
/// `SpawnUtil.Strategy.LEGACY_IRON_GOLEM`'s refused floors.
fn golem_floor_ok(name: &str) -> bool {
    !matches!(
        name,
        "minecraft:cobweb"
            | "minecraft:cactus"
            | "minecraft:glass_pane"
            | "minecraft:conduit"
            | "minecraft:ice"
            | "minecraft:tnt"
            | "minecraft:glowstone"
            | "minecraft:beacon"
            | "minecraft:sea_lantern"
            | "minecraft:frosted_ice"
            | "minecraft:tinted_glass"
            | "minecraft:glass"
    ) && !name.ends_with("_stained_glass_pane")
        && !name.ends_with("_stained_glass")
        && !name.ends_with("leaves")
}
/// One attempt of `SpawnUtil.trySpawnMob(IRON_GOLEM, ..., 8 across, 6 up and
/// down, LEGACY_IRON_GOLEM)`: a column drawn near `start`, searched down from
/// 6 above for a solid floor (or powder snow) under air or a liquid block.
/// Returns the block the golem would stand in.
fn golem_attempt<L: super::spawn_rules::SpawnLevel>(
    l: &L,
    start: [i32; 3],
    random: &mut impl RandomSource,
) -> Option<[i32; 3]> {
    let dx = random.next_int_bound(17).unwrap_or(8) - 8;
    let dz = random.next_int_bound(17).unwrap_or(8) - 8;
    let (x, z) = (start[0] + dx, start[2] + dz);
    let name = |y: i32| -> &str { &l.facts().get(l.state(x, y, z)).name };
    let mut y = start[1] + 6;
    let mut above = l.state(x, y, z);
    for _ in 0..=12 {
        y -= 1;
        let here = l.state(x, y, z);
        let above_name = &*l.facts().get(above).name;
        // BlockState.liquid(): a water or lava block (not a waterlogged one).
        let open = l.facts().is_air(above)
            || above_name == "minecraft:water"
            || above_name == "minecraft:lava";
        let solid = l.mob_data().state(here).flags & super::data::SOLID != 0
            || name(y) == "minecraft:powder_snow";
        if golem_floor_ok(name(y)) && open && solid {
            return Some([x, y + 1, z]);
        }
        above = here;
    }
    None
}
/// `IronGolem.checkSpawnObstruction`: a floor with a full top face, its feet
/// free whatever fluid is there, and the two blocks above free.
fn golem_fits<L: super::spawn_rules::SpawnLevel>(l: &L, [x, y, z]: [i32; 3]) -> bool {
    use super::spawn_rules::is_valid_empty_spawn;
    // entityCanStandOn: Block.isFaceFull(collision shape, UP).
    let floor = l.mob_data().state(l.state(x, y - 1, z));
    let stands = floor.flags & super::data::STURDY_TOP != 0 && floor.top >= 1.0;
    // isValidEmptySpawnBlock with Fluids.EMPTY: open water is free too.
    let feet = is_valid_empty_spawn(Species::IronGolem, l, x, y, z)
        || &*l.facts().get(l.state(x, y, z)).name == "minecraft:water";
    stands
        && feet
        && is_valid_empty_spawn(Species::IronGolem, l, x, y + 1, z)
        && is_valid_empty_spawn(Species::IronGolem, l, x, y + 2, z)
}
/// `SpawnUtil.trySpawnMob(IRON_GOLEM, MOB_SUMMONED, level, start, 10, 8, 6,
/// LEGACY_IRON_GOLEM, false)`: up to ten columns tried. In each spot found
/// `create` makes the golem (`EntityType.create`: the yaw drawn here, then its
/// `finalizeSpawn` draws, see [`finalize_summoned_golem`]); it is kept only
/// when `IronGolem.checkSpawnObstruction` passes, and discarded otherwise.
pub fn summon_golem<L: super::spawn_rules::SpawnLevel, R: RandomSource, T>(
    l: &L,
    start: [i32; 3],
    random: &mut R,
    mut create: impl FnMut(&mut R, horizon_core::geometry::Vec3, f32) -> T,
) -> Option<T> {
    for _ in 0..10 {
        let Some([x, y, z]) = golem_attempt(l, start, random) else {
            continue;
        };
        // Mth.wrapDegrees(nextFloat() * 360).
        let mut yaw = random.next_float() * 360.0;
        if yaw >= 180.0 {
            yaw -= 360.0;
        }
        let at =
            horizon_core::geometry::Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5);
        let golem = create(random, at, yaw);
        if golem_fits(l, [x, y, z]) {
            return Some(golem);
        }
    }
    None
}





/// A seed for one villager's brain tick: a splitmix64 mix of the mob's id and the game time.
fn brain_seed(id: u32, game_time: i64) -> i64 {
    let mut z = (u64::from(id) << 32) ^ (game_time as u64);
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    (z ^ (z >> 31)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    // 26.3 VillagerProfession.bootstrap, PoiTypes.bootstrap and TradeSets.
    #[test]
    fn vanilla_profession_table_and_job_sites() {
        let cases = [
            ("none", &[][..]),
            ("armorer", &["blast_furnace"][..]),
            ("butcher", &["smoker"][..]),
            ("cartographer", &["cartography_table"][..]),
            ("cleric", &["brewing_stand"][..]),
            ("farmer", &["composter"][..]),
            ("fisherman", &["barrel"][..]),
            ("fletcher", &["fletching_table"][..]),
            (
                "leatherworker",
                &[
                    "cauldron",
                    "water_cauldron",
                    "lava_cauldron",
                    "powder_snow_cauldron",
                ][..],
            ),
            ("librarian", &["lectern"][..]),
            ("mason", &["stonecutter"][..]),
            ("nitwit", &[][..]),
            ("shepherd", &["loom"][..]),
            ("toolsmith", &["smithing_table"][..]),
            ("weaponsmith", &["grindstone"][..]),
        ];
        for (index, (name, sites)) in cases.iter().enumerate() {
            let index = index as u8;
            assert_eq!(profession(index).id.as_str(), *name);
            assert_eq!(profession_index(name), Some(index));
            assert_eq!(profession(index).render_index, index);
            for site in *sites {
                let state = horizon_world::chunk::BlockState {
                    name: format!("minecraft:{site}"),
                    properties: Default::default(),
                };
                let poi = horizon_world::poi::saved_poi(&state).unwrap();
                assert_eq!(poi.id(), format!("minecraft:{name}"));
                assert_eq!(profession_table().by_poi(poi), Some(index));
                assert!(profession_table().held(index, poi));
                assert!(profession_table().acquirable(index, poi));
            }
            for level in 1..=5 {
                assert_eq!(
                    profession_table().trade_key(name, level).unwrap_or(""),
                    if matches!(*name, "none" | "nitwit") {
                        String::new()
                    } else {
                        format!("minecraft:{name}/level_{level}")
                    }
                );
            }
        }
        assert_eq!(profession(255).id.as_str(), "none");
        assert_eq!(profession_index("other:farmer"), None);
        assert_eq!(profession_table().trade_key("other:farmer", 2), None);
    }

    /// Added after the H138 baseline: `add_item` delegates to
    /// `add_to_container` with the eight villager slots, with the same
    /// inventories and remainders in the same order.
    #[test]
    fn add_item_is_add_to_container_with_eight_slots() {
        let st = |id: &str, n: u32| ItemStack::new(id, n);
        let sequences: [&[(&str, u32, u32)]; 3] = [
            &[
                ("minecraft:stone", 40, 64),
                ("minecraft:stone", 40, 64),
                ("minecraft:dirt", 1, 64),
            ],
            &[
                ("minecraft:ender_pearl", 10, 16),
                ("minecraft:ender_pearl", 10, 16),
            ],
            &[("minecraft:stone", 64, 64); 10],
        ];
        for seq in sequences {
            let mut ext = Ext::default();
            let mut vec = Vec::new();
            for &(id, n, max) in seq {
                let a = ext
                    .add_item(st(id, n), max)
                    .map(|s| (s.item.to_string(), s.count));
                let b = add_to_container(&mut vec, st(id, n), max, INVENTORY_SLOTS)
                    .map(|s| (s.item.to_string(), s.count));
                assert_eq!(a, b);
                let counts = |v: &[ItemStack]| {
                    v.iter()
                        .map(|s| (s.item.to_string(), s.count))
                        .collect::<Vec<_>>()
                };
                assert_eq!(counts(&ext.inventory), counts(&vec));
            }
        }
    }

    /// Added after the H138 baseline: eight kinds fill the eight slots; a
    /// later stack of a kind already held still fills its stack, and a
    /// ninth kind does not fit (fromItemList drops it).
    #[test]
    fn a_later_stack_still_merges_into_a_full_set_of_slots() {
        let kinds = [
            "stone",
            "dirt",
            "cobblestone",
            "oak_planks",
            "sand",
            "gravel",
            "oak_log",
            "glass",
        ];
        let mut vec = Vec::new();
        for k in kinds {
            assert!(
                add_to_container(
                    &mut vec,
                    ItemStack::new(&format!("minecraft:{k}"), 1),
                    64,
                    8
                )
                .is_none()
            );
        }
        assert!(add_to_container(&mut vec, ItemStack::new("minecraft:stone", 5), 64, 8).is_none());
        assert_eq!(vec.len(), 8);
        assert_eq!((vec[0].item.as_ref(), vec[0].count), ("minecraft:stone", 6));
        let rest = add_to_container(&mut vec, ItemStack::new("minecraft:clay", 1), 64, 8);
        assert_eq!(rest.map(|s| s.count), Some(1));
        assert_eq!(vec.len(), 8);
    }


    #[test]
    fn villager_memory_nbt_shape() {
        use horizon_core::nbt::Tag;
        let m = memory_fixture();
        let mut entries = Vec::new();
        write_memories(&m, &mut entries);
        let actual = Tag::compound(entries);
        let expected = Tag::compound([
            (
                "minecraft:golem_detected_recently",
                Tag::compound([("value", Tag::Byte(1)), ("ttl", Tag::Long(42))]),
            ),
            (
                "minecraft:last_slept",
                Tag::compound([("value", Tag::Long(101)), ("ttl", Tag::Long(17))]),
            ),
            (
                "minecraft:last_woken",
                Tag::compound([("value", Tag::Long(202))]),
            ),
            (
                "minecraft:potential_job_site",
                Tag::compound([
                    (
                        "value",
                        Tag::compound([
                            ("dimension", Tag::String("minecraft:overworld".into())),
                            ("pos", Tag::IntArray(vec![4, 5, 6])),
                        ]),
                    ),
                    ("ttl", Tag::Long(9)),
                ]),
            ),
        ]);
        assert_eq!(actual, expected);
    }

    /// `Villager.readAdditionalSaveData` reads `FoodLevel` with `getByteOr`:
    /// an int is narrowed (`(byte)(value & 255)`), so 300 is 44 and 200 -56.
    #[test]
    fn a_villagers_food_level_is_narrowed_to_a_byte() {
        use horizon_core::nbt::Tag;
        for (saved, expected) in [
            (Tag::Int(300), 44),
            (Tag::Int(200), -56),
            (Tag::Byte(12), 12),
        ] {
            let mut loaded = villager();
            read_nbt(&mut loaded, &Tag::compound([("FoodLevel", saved)]));
            assert_eq!(ext(&loaded).unwrap().food_level, expected);
        }
    }

    #[test]
    fn villager_memory_round_trip() {
        use horizon_core::nbt::Tag;
        let m = memory_fixture();
        let mut entries = Vec::new();
        write_memories(&m, &mut entries);
        entries.push((
            "minecraft:last_worked_at_poi",
            Tag::compound([("value", Tag::Long(303)), ("ttl", Tag::Long(7))]),
        ));
        let tag = Tag::compound([(
            "Brain",
            Tag::compound([("memories", Tag::compound(entries))]),
        )]);
        let mut loaded = villager();
        read_nbt(&mut loaded, &tag);
        super::super::villager_brain::init(&mut loaded, 500, 0, 1);
        let mm = &mut ext_mut(&mut loaded).brain.as_mut().unwrap().mem;
        assert_eq!(
            (
                mm.golem_detected_recently.value(),
                mm.golem_detected_recently.ttl()
            ),
            (Some(true), 42)
        );
        assert_eq!(
            (mm.last_slept.value(), mm.last_slept.ttl()),
            (Some(101), 17)
        );
        assert_eq!(
            (mm.last_woken.value(), mm.last_woken.ttl()),
            (Some(202), i64::MAX)
        );
        assert_eq!(
            (mm.last_worked_at_poi.value(), mm.last_worked_at_poi.ttl()),
            (None, i64::MAX)
        );
        assert_eq!(
            (mm.potential_job_site.value(), mm.potential_job_site.ttl()),
            (Some([4, 5, 6]), 9)
        );
        for _ in 0..42 {
            mm.golem_detected_recently.tick();
        }
        assert!(mm.golem_detected_recently.present());
        mm.golem_detected_recently.tick();
        assert!(!mm.golem_detected_recently.present());
    }


    #[test]
    fn waking_faces_the_bed_and_resets_pitch_without_moving_the_chosen_position() {
        let mut m = villager();
        m.yaw = -25.0;
        m.head_pitch = 23.0;
        let stand = horizon_core::geometry::Vec3::new(3.5, 200.0, 6.5);
        set_wake_pose(&mut m, [2, 200, 6], stand, None);
        // Fresh official 26.3 occupied-bed interaction.
        assert_eq!(m.yaw, 89.99999);
        assert_eq!(m.head_pitch, 0.0);
        assert_eq!(m.position, stand);
    }
    /// A sleeper's box is 0.2 high, but `LivingEntityRenderer` moves it
    /// toward the foot of the bed by its standing eye height, so its head
    /// lies on the pillow.
    #[test]
    fn a_sleeper_lies_by_its_standing_eye_height() {
        let mut m = villager();
        m.extra.ints[1] = bed_facing_code(Some("east"));
        assert!(sleeping(&m));
        assert_eq!(m.eye_height(), f64::from(0.2_f32));
        assert_eq!(m.awake_dims().2, f64::from(1.62_f32));
    }
    /// `LivingEntity.add/readAdditionalSaveData`: a sleeper is saved with
    /// its bed (`sleeping_pos`) and loads asleep in it.
    #[test]
    fn a_sleeper_is_saved_asleep_in_its_bed() {
        let mut m = villager();
        m.extra.ints[1] = bed_facing_code(Some("north"));
        ext_mut(&mut m).sleeping_pos = Some([3, 64, -7]);
        let mut map = horizon_core::nbt::Compound::new();
        write_nbt(&m, &mut map);
        let mut loaded = villager();
        read_nbt(&mut loaded, &horizon_core::nbt::Tag::Compound(map));
        assert!(sleeping(&loaded));
        assert_eq!(sleeping_pos(&loaded), Some([3, 64, -7]));
        let mut awake = villager();
        let mut map = horizon_core::nbt::Compound::new();
        write_nbt(&awake, &mut map);
        assert!(!map.contains_key(&"sleeping_pos".into()));
        read_nbt(&mut awake, &horizon_core::nbt::Tag::Compound(map));
        assert!(!sleeping(&awake));
    }
    #[test]
    fn saved_sleeping_position_follows_live_state_over_stale_input() {
        use horizon_core::nbt::Tag;
        for asleep in [false, true] {
            let mut m = villager();
            if asleep {
                m.extra.ints[1] = bed_facing_code(Some("north"));
                ext_mut(&mut m).sleeping_pos = Some([3, 64, -7]);
            }
            let expected = asleep.then_some(Tag::IntArray(vec![3, 64, -7]));
            for stale in [false, true] {
                let mut map = horizon_core::nbt::Compound::new();
                map.insert("h232_control".into(), Tag::Int(42));
                if stale {
                    map.insert("sleeping_pos".into(), Tag::IntArray(vec![9, 90, 9]));
                }
                write_nbt(&m, &mut map);
                assert_eq!(map.get(&"sleeping_pos".into()), expected.as_ref());
                assert_eq!(map.get(&"h232_control".into()), Some(&Tag::Int(42)));
            }
            let saved = crate::entity_nbt::mob_to_nbt(&m, [0; 4], None, None, |_| None);
            assert_eq!(saved.get("sleeping_pos"), expected.as_ref());
        }
    }
    /// `Villager.setUnhappy` -> `Mob.makeSound(VILLAGER_NO)` plays with
    /// `LivingEntity.getVoicePitch`: a baby is pitched about 1.5, an adult 1.0.
    #[test]
    fn the_unhappy_sound_uses_the_voice_pitch() {
        let sound_pitch = |age: i32| {
            let mut m = villager();
            m.extra.age = age;
            let mut events = Vec::new();
            set_unhappy(&mut m, &mut events);
            match events[0] {
                MobEvent::Sound(_, _, _, pitch) => pitch,
                _ => panic!("expected a sound"),
            }
        };
        assert!((0.8..=1.2).contains(&sound_pitch(0)));
        assert!((1.3..=1.7).contains(&sound_pitch(-24000)));
    }
    #[test]
    fn a_new_profession_rolls_new_offers() {
        let mut m = villager();
        set_profession(&mut m, profession_table(), 1);
        // Offers made for the first profession.
        m.extra.trade.get_or_insert_default().unset = false;
        // The same profession keeps them.
        set_profession(&mut m, profession_table(), 1);
        assert!(!m.extra.trade.as_ref().unwrap().unset);
        // A new one forgets them (offers = null), so the next getOffers
        // (ensure_offers) rolls the new profession's trades.
        set_profession(&mut m, profession_table(), 2);
        let t = m.extra.trade.as_ref().unwrap();
        assert!(t.offers.is_empty());
        assert!(t.unset);
        assert_eq!(m.extra.profession.index().unwrap_or(0), 2);
    }
    /// The reported sequence: a villager works at a bench, the bench is
    /// destroyed (its job site is gone, so `ResetProfession` makes it
    /// unemployed) and it takes another bench's job. The new profession's
    /// trades are offered again.
    ///
    /// `Villager.setVillagerData` drops the offers (`offers = null`) when the
    /// profession changes, so `AbstractVillager.getOffers` rolls the new
    /// profession's trade set the next time the offers are asked for. A
    /// villager that keeps its old offers marked as made never trades again:
    /// it shakes its head instead of opening the trading menu.
    #[test]
    fn a_villager_that_loses_its_job_site_offers_its_new_profession_trades() {
        let mut m = villager();
        // The composter's job: the villager is a farmer.
        set_profession(&mut m, profession_table(), 5);
        assert_eq!(m.extra.profession.index().unwrap_or(0), 5);
        // Its trades were made, so the offers are there.
        let trade = m.extra.trade.get_or_insert_default();
        trade.unset = false;
        trade.offers = vec![crate::merchant::MerchantOffer {
            base_cost_a: crate::merchant::ItemCost {
                stack: ItemStack::new("minecraft:wheat", 20),
            },
            cost_b: None,
            result: ItemStack::new("minecraft:emerald", 1),
            uses: 0,
            max_uses: 16,
            reward_exp: true,
            special_price_diff: 0,
            demand: 0,
            price_multiplier: 0.0,
            xp: 1,
        }];
        // The work bench is destroyed: the job site is gone, so the villager
        // is unemployed and its offers are dropped (`offers = null`).
        set_profession(&mut m, profession_table(), usize::from(NONE));
        let t = m.extra.trade.as_ref().unwrap();
        assert!(t.offers.is_empty());
        assert!(t.unset);
        // It takes the smithing table's job. The new profession's trades are
        // rolled by the next `getOffers` (ensure_offers).
        set_profession(&mut m, profession_table(), 13);
        let t = m.extra.trade.as_ref().unwrap();
        assert!(t.offers.is_empty());
        assert!(t.unset);
        assert_eq!(m.extra.profession.index().unwrap_or(0), 13);
    }

    fn stack(item: &str, count: u32) -> ItemStack {
        ItemStack::new(item, count)
    }
    /// The registry these fixtures' villagers live with: vanilla's four foods.
    fn vanilla_food() -> Items {
        crate::test_resources::villager_food_items(&crate::test_resources::VANILLA_VILLAGER_FOOD)
    }
    #[test]
    fn eating_fills_the_food_level_then_digests() {
        let items = &vanilla_food();
        let mut e = Ext {
            inventory: vec![stack("minecraft:bread", 3), stack("minecraft:carrot", 5)],
            ..Default::default()
        };
        assert_eq!(e.food_in_inventory(items), 17);
        // Hungry below 12: three breads fill it exactly, then 12 digest away.
        e.eat_and_digest(items);
        assert_eq!(e.food_level, 0);
        assert_eq!(e.inventory.len(), 1);
        assert_eq!(e.inventory[0].count, 5);
        // Five carrots are not enough to be full again, and are all eaten.
        e.eat_and_digest(items);
        assert_eq!(e.food_level, 5 - 12);
        assert!(e.inventory.is_empty());
    }
    #[test]
    fn willingness_to_breed_needs_twelve_food_points() {
        let items = &vanilla_food();
        let mut e = Ext::default();
        assert!(e.wants_more_food(items) && !e.has_excess_food(items));
        e.inventory.push(stack("minecraft:bread", 6));
        assert!(e.has_excess_food(items) && !e.wants_more_food(items));
        e.food_level = -5;
        assert_eq!(e.food_level + e.food_in_inventory(items), 19);
    }
    #[test]
    fn the_inventory_has_eight_stacking_slots() {
        let mut e = Ext::default();
        assert!(e.add_item(stack("minecraft:bread", 100), 64).is_none());
        assert_eq!(e.inventory.len(), 2);
        assert_eq!(e.inventory[0].count, 64);
        for i in 0..6 {
            assert!(
                e.add_item(stack(&format!("minecraft:item_{i}"), 1), 64)
                    .is_none()
            );
        }
        assert_eq!(e.inventory.len(), 8);
        // Full: another kind is refused, the same kind tops up the stack.
        assert!(!e.can_add(&stack("minecraft:stick", 1), 64));
        let left = e.add_item(stack("minecraft:bread", 40), 64).unwrap();
        assert_eq!(left.count, 40 - 28);
    }
    #[test]
    fn half_stacks_are_thrown() {
        let mut e = Ext {
            inventory: vec![
                stack("minecraft:bread", 10),
                stack("minecraft:wheat", 50),
                stack("minecraft:wheat", 20),
            ],
            ..Default::default()
        };
        let max = |_: &str| 64;
        // Bread is under half a stack and under 24: nothing.
        assert!(
            e.take_half_stack(|s| &*s.item == "minecraft:bread", max)
                .is_none()
        );
        // 50 wheat is over half a stack: 25 go.
        let t = e
            .take_half_stack(|s| &*s.item == "minecraft:wheat", max)
            .unwrap();
        assert_eq!(t.count, 25);
        assert_eq!(e.count_item("minecraft:wheat"), 25 + 20);
    }
    #[test]
    fn golems_need_sleep_and_no_golem_nearby() {
        let mut e = Ext::default();
        assert!(!e.wants_golem(100));
        e.last_slept = Some(0);
        assert!(e.wants_golem(23_999));
        assert!(!e.wants_golem(24_000));
        e.last_slept = Some(1000);
        e.golem_until = 2000 + 599;
        assert!(!e.wants_golem(2500));
        assert!(e.wants_golem(2700));
    }
    #[test]
    fn gossip_decays_once_a_day_and_heard_gossip_waits() {
        let player = [1, 2, 3, 4];
        let mut e = Ext::default();
        e.gossips
            .add(player, crate::gossip::GossipType::MajorNegative, 25);
        e.maybe_decay(100);
        assert_eq!(e.last_gossip_decay, 100);
        e.maybe_decay(24_099);
        assert_eq!(
            e.gossips
                .get(player, crate::gossip::GossipType::MajorNegative),
            25
        );
        e.maybe_decay(24_100);
        assert_eq!(
            e.gossips
                .get(player, crate::gossip::GossipType::MajorNegative),
            15
        );
        e.last_gossip_time = 5000;
        assert!(!e.can_gossip(5100));
        assert!(e.can_gossip(6200));
        assert!(e.can_gossip(4000));
    }
    #[test]
    fn villager_food_values() {
        let items = &vanilla_food();
        assert_eq!(food_points(items, "minecraft:bread"), 4);
        assert_eq!(food_points(items, "minecraft:beetroot"), 1);
        assert_eq!(food_points(items, "minecraft:apple"), 0);
    }
    /// A villager's snapshot counts its food by the registry it is taken with:
    /// another registry built since, or alongside, never answers for it.
    #[test]
    fn a_snapshot_counts_food_by_its_own_registry() {
        let vanilla = vanilla_food();
        let starved = crate::test_resources::villager_food_items(&[("minecraft:bread", 0)]);
        let mut m = villager();
        ext_mut(&mut m).inventory.push(stack("minecraft:bread", 3));
        let fed = m.seen(None, &vanilla);
        let hungry = m.seen(None, &starved);
        assert_eq!(fed.villager.as_ref().map(|v| v.food_points), Some(12));
        assert!(fed.in_love && !fed.villager.unwrap().wants_more_food());
        assert_eq!(hungry.villager.as_ref().map(|v| v.food_points), Some(0));
        assert!(!hungry.in_love && hungry.villager.unwrap().wants_more_food());
        assert!(can_breed(&m, &vanilla) && !can_breed(&m, &starved));
    }
}
