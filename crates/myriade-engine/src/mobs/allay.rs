use crate::health::DamageLevel;
use crate::items::ItemStack;
use crate::sculk::{self, Context, Info};
use horizon_core::geometry::trig::Mth;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_protocol::entity_data::PositionSource;

pub mod interaction;

pub const HOLD: usize = 0;
pub const SPIN: usize = 1;
/// `Allay.VibrationUser.VIBRATION_EVENT_LISTENER_RANGE`.
const LISTENER_RADIUS: i64 = 16;

/// `AllayAi.TIME_TO_FORGET_NOTEBLOCK`.
const TIME_TO_FORGET_NOTEBLOCK: i32 = 600;
/// `Allay.MAX_NOTEBLOCK_DISTANCE`.
const MAX_NOTEBLOCK_DISTANCE: i32 = 1024;

/// `GlobalPos`: a block position in a dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalPos {
    pub dimension: &'static str,
    pub pos: [i32; 3],
}
impl GlobalPos {
    /// `GlobalPos.isCloseEnough`: the same dimension and at most
    /// `max_distance` blocks away on every axis (`Vec3i.distChessboard`).
    pub fn is_close_enough(&self, dimension: &str, pos: [i32; 3], max_distance: i32) -> bool {
        let chessboard = (0..3)
            .map(|i| self.pos[i].wrapping_sub(pos[i]).wrapping_abs())
            .max()
            .unwrap_or(0);
        self.dimension == dimension && chessboard <= max_distance
    }
}
/// The identifier of a dimension as a `'static` string, for loading saves.
pub fn dimension_id(id: &str) -> &'static str {
    horizon_core::LevelKey::builtin_or_overworld(id).id()
}

/// `AllayAi.allayConsidersItemEqual`: the same item (`ItemStack.isSameItem`)
/// and the same potion contents (`!hasNonMatchingPotion`); no
/// `potion_contents` on a stack is the empty contents, and two empty ones match.
// shortcut: only the potion id is compared, not custom effects or colour, which
// `StackData` does not keep; upgrade when it does.
pub fn considers_item_equal(held: &ItemStack, candidate: &ItemStack) -> bool {
    held.item == candidate.item && held.potion() == candidate.potion()
}

/// `GameEvent.JUKEBOX_PLAY.notificationRadius()`: the `JukeboxListener`'s radius.
const JUKEBOX_LISTENER_RADIUS: i64 = 10;






/// `Mob.getPickupReach` (1, 1, 1) around the box: `Mob.aiStep`'s looting loop
/// takes an equal drop from here with or without AI, so it is the only pickup a
/// `NoAI` allay has (`canPickUpLoot` and `wantsToPickUp` never ask for AI;
/// walking to a drop is the sensor's and throwing it is the brain's).
pub fn pickup_reach(bounds: Aabb) -> Aabb {
    bounds.inflate(Vec3::new(1.0, 1.0, 1.0))
}

/// An allay's vibration listener (`Allay.vibrationData`) and the brain
/// memory `LIKED_NOTEBLOCK_POSITION`; `LIKED_NOTEBLOCK_COOLDOWN_TICKS` is
/// `ints[2]`.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// Allay's separate one-slot SimpleContainer.
    pub inventory: Option<ItemStack>,
    /// The full Codec.INT value, including zero while the memory is present.
    pub pickup_cooldown: Option<i32>,
    pub vibration: sculk::Data,
    pub noteblock: Option<GlobalPos>,
    /// The `LIKED_PLAYER` memory's time to live (`MemorySlot.timeToLive`);
    /// None for a memory that never expires. The memory itself is
    /// `Extra::tame` and `Extra::owner`.
    pub liked_player_ttl: Option<i64>,
    /// `Allay.jukeboxPos`: the jukebox the dance started from. Not saved.
    pub jukebox: Option<[i32; 3]>,
}


impl State {
    /// SimpleContainer.canAddItem: merge only identical item components.
    pub fn room(&self, stack: &ItemStack, max: u32) -> u32 {
        let max = max.min(99);
        match &self.inventory {
            None => max,
            Some(held) if held.same_item_same_components(stack) => max.saturating_sub(held.count),
            Some(_) => 0,
        }
    }

    /// SimpleContainer.addItem for the Allay's single slot; returns the count accepted.
    pub fn add(&mut self, mut stack: ItemStack, max: u32) -> u32 {
        let count = stack.count.min(self.room(&stack, max));
        if count > 0 {
            if let Some(held) = &mut self.inventory {
                held.count += count;
            } else {
                stack.count = count;
                self.inventory = Some(stack);
            }
        }
        count
    }

    pub fn take(&mut self, count: u32) -> Option<ItemStack> {
        let held = self.inventory.as_mut()?;
        let mut taken = held.clone();
        taken.count = count.min(held.count);
        held.count -= taken.count;
        if held.count == 0 {
            self.inventory = None;
        }
        Some(taken)
    }
}

/// `GoToWantedItem`: fly to the wanted drop.
#[derive(Clone, Debug)]
struct Fetch;
/// `GoAndGiveItemsToTarget`: carry the load to the deposit and throw it.
#[derive(Clone, Debug)]
struct Deliver;
/// `StayCloseToTarget.create(.., 4, 16, 2.25)`: acts only once the deposit is
/// 16 blocks away or more, then walks until within 4.
#[derive(Clone, Debug)]
struct StayClose;
#[cfg(test)]
mod allay_parity_tests {
    use super::*;

    fn potion(id: &str) -> ItemStack {
        ItemStack::new("minecraft:potion", 1).with_potion(id)
    }


    #[test]
    fn a_held_potion_matches_only_its_own_contents() {
        assert!(!considers_item_equal(
            &potion("minecraft:swiftness"),
            &potion("minecraft:healing")
        ));
        assert!(considers_item_equal(
            &potion("minecraft:swiftness"),
            &potion("minecraft:swiftness")
        ));
    }

    #[test]
    fn stacks_without_potion_contents_match_by_item() {
        let stone = ItemStack::new("minecraft:stone", 1);
        assert!(considers_item_equal(&stone, &stone));
        assert!(!considers_item_equal(
            &stone,
            &ItemStack::new("minecraft:dirt", 1)
        ));
        // An empty potion component on both sides matches; one is not enough.
        assert!(considers_item_equal(
            &ItemStack::new("minecraft:potion", 1),
            &ItemStack::new("minecraft:potion", 1)
        ));
        assert!(!considers_item_equal(
            &ItemStack::new("minecraft:potion", 1),
            &potion("minecraft:swiftness")
        ));
    }

    #[test]
    fn a_no_ai_allay_does_not_dance_to_a_playing_jukebox() {
        let mut mob = test_allay();
        mob.no_ai = true;
        set_jukebox_playing(&mut mob, [0, 64, 0], true);
        assert!(!mob.extra.flag, "NoAI");
        mob.no_ai = false;
        set_jukebox_playing(&mut mob, [0, 64, 0], true);
        assert!(mob.extra.flag, "AI");
        // A burning allay panics: it does not start dancing.
        let mut burning = test_allay();
        burning.fire_ticks = 20;
        set_jukebox_playing(&mut burning, [0, 64, 0], true);
        assert!(!burning.extra.flag);
    }


    // Added after the baseline: the kept jukebox and the listener's reach.
    #[test]
    fn only_the_kept_jukebox_stops_the_dance() {
        let mut mob = test_allay();
        set_jukebox_playing(&mut mob, [3, 64, 0], true);
        // Another jukebox plays: the first one stays the dance's.
        set_jukebox_playing(&mut mob, [-4, 64, 0], true);
        assert_eq!(mob.extra.allay.as_ref().unwrap().jukebox, Some([3, 64, 0]));
        set_jukebox_playing(&mut mob, [-4, 64, 0], false);
        assert!(mob.extra.flag, "a stop elsewhere does not end it");
        set_jukebox_playing(&mut mob, [3, 64, 0], false);
        assert!(!mob.extra.flag);
        assert_eq!(mob.extra.allay.as_ref().unwrap().jukebox, None);
        // With none kept, any stop ends a dance.
        mob.extra.flag = true;
        set_jukebox_playing(&mut mob, [9, 64, 9], false);
        assert!(!mob.extra.flag);
    }

    #[test]
    fn the_jukebox_listener_hears_ten_blocks_from_the_eye_block() {
        let mob = test_allay();
        let eye = mob.eye_y().floor();
        // Block distance squared from the eye's block: 100 is heard, 101 not.
        assert!(hears_jukebox(&mob, [10.5, eye, 0.5]));
        assert!(!hears_jukebox(&mob, [10.5, eye + 1.0, 0.5]));
        assert!(hears_jukebox(&mob, [6.5, eye + 8.0, 0.5]));
    }

    #[test]
    fn a_no_ai_allay_takes_only_a_drop_already_in_pickup_reach() {
        // An allay at 0.5, 64, 0.5: 0.6 wide and tall.
        let bounds = Aabb::new(Vec3::new(0.2, 64.0, 0.2), Vec3::new(0.8, 64.6, 0.8));
        let reach = pickup_reach(bounds);
        // `Mob.aiStep`'s looting loop takes a drop at its feet, and one just
        // past the box, with no AI.
        assert!(reach.contains(Vec3::new(0.5, 64.0, 0.5)));
        assert!(reach.contains(Vec3::new(0.5, 64.0, 1.7)));
        // Past the one-block grow it would have to walk (`GoToWantedItem` is
        // the brain's), and 32 blocks is the sensor's reach, not this one.
        assert!(!reach.contains(Vec3::new(0.5, 64.0, 1.9)));
        assert!(!reach.contains(Vec3::new(0.5, 64.0, 20.0)));
        assert!(!reach.contains(Vec3::new(0.5, 66.0, 0.5)));
    }
}
