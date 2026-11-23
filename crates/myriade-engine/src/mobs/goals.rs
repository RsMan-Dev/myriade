use horizon_core::{
    geometry::{Aabb, Vec3},
    jmath,
    random::RandomSource,
};
use std::sync::Arc;

pub const MOVE: u8 = 1;
pub const LOOK: u8 = 2;
pub const JUMP: u8 = 4;
pub const TARGET: u8 = 8;


pub trait Goal: std::fmt::Debug + Send + Sync {
    fn flags(&self) -> u8;
    /// A goal that draws its first start delay in its constructor (a schooling fish's
    /// `FollowFlockLeaderGoal`): the delay again, when the mob's random is seeded afterwards.
    fn set_start_delay(&mut self, _ticks: i32) {}
    fn interruptable(&self) -> bool {
        true
    }
    fn every_tick(&self) -> bool {
        false
    }
    /// `RandomStrollGoal.trigger`: the goal's next `canUse` skips the
    /// interval's draw (only a stroll has one).
    fn trigger(&mut self) {}
    /// `GuardianAttackGoal.stop` triggers the guardian's `randomStrollGoal`
    /// when it stops; the selector is what holds the goals then, so it asks.
    fn strolls_on_stop(&self) -> bool {
        false
    }
    fn box_clone(&self) -> Box<dyn Goal>;
    /// The goal's name for debugging (`Goal.toString`).
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
            .rsplit("::")
            .next()
            .unwrap_or("Goal")
    }
}
impl Clone for Box<dyn Goal> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}
macro_rules! goal_clone {
    () => {
        fn box_clone(&self) -> Box<dyn crate::mobs::goals::Goal> {
            Box::new(self.clone())
        }
    };
}
pub(crate) use goal_clone;

/// Goal.reducedTickDelay
pub fn reduced(ticks: i32) -> i32 {
    (ticks + 1) / 2
}
fn adjusted(every_tick: bool, ticks: i32) -> i32 {
    if every_tick { ticks } else { reduced(ticks) }
}

#[derive(Clone, Debug)]
struct Wrapped {
    priority: i32,
    goal: Box<dyn Goal>,
    running: bool,
}
#[derive(Clone, Debug, Default)]
pub struct GoalSelector {
    goals: Vec<Wrapped>,
    locked: [Option<usize>; 4],
    pub disabled: u8,
}
impl GoalSelector {
    pub fn add(&mut self, priority: i32, goal: impl Goal + 'static) {
        self.add_boxed(priority, Box::new(goal));
    }
    /// `add` for a goal built at runtime (`Box<dyn Goal>`).
    pub fn add_boxed(&mut self, priority: i32, goal: Box<dyn Goal>) {
        self.goals.push(Wrapped {
            priority,
            goal,
            running: false,
        });
    }
    /// `Goal::set_start_delay` of the goal of this name.
    pub fn set_start_delay(&mut self, name: &str, ticks: i32) {
        for g in &mut self.goals {
            if g.goal.name() == name {
                g.goal.set_start_delay(ticks);
            }
        }
    }
    pub fn clear(&mut self) {
        self.goals.clear();
        self.locked = [None; 4];
    }
    pub fn running_names(&self) -> Vec<&'static str> {
        self.goals
            .iter()
            .filter(|g| g.running)
            .map(|g| g.goal.name())
            .collect()
    }
    /// Whether a goal of this name is registered, running or not.
    pub fn has(&self, name: &str) -> bool {
        self.goals.iter().any(|g| g.goal.name() == name)
    }
    pub fn is_running(&self, name: &str) -> bool {
        self.goals
            .iter()
            .any(|g| g.running && g.goal.name() == name)
    }
    /// `RandomStrollGoal.trigger` of the mob's stroll goals: the guardian's
    /// attack goal asks for one when its beam ends, and its hurt too.
    pub fn trigger_stroll(&mut self) {
        for g in &mut self.goals {
            g.goal.trigger();
        }
    }
    /// Every goal as (priority, name, running), in registration order.
    pub fn entries(&self) -> impl Iterator<Item = (i32, &'static str, bool)> + '_ {
        self.goals
            .iter()
            .map(|g| (g.priority, g.goal.name(), g.running))
    }
}

// --- random positions (DefaultRandomPos / LandRandomPos / RandomPos) -------

/// `Mob.isWithinHome(BlockPos)`: `Vec3i.distSqr` in doubles, so no
/// coordinate pair overflows, against the radius squared as a Java `int`
/// (which wraps past 46 340, as vanilla's does).
pub fn within_home(c: [i32; 3], r: i32, p: [i32; 3]) -> bool {
    let d = |i: usize| f64::from(c[i]) - f64::from(p[i]);
    d(0) * d(0) + d(1) * d(1) + d(2) * d(2) < f64::from(r.wrapping_mul(r))
}


// --- goals ---------------------------------------------------------------

/// FloatGoal: jump (swim up) while deeper than the jump threshold or in lava.
#[derive(Clone, Debug)]
pub struct Float;
impl Goal for Float {
    goal_clone!();
    fn flags(&self) -> u8 {
        JUMP
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// `ClimbOnTopOfPowderSnowGoal`: a powder-snow walker standing in powder snow
/// jumps while the block above is powder snow or has no collision.
#[derive(Clone, Debug)]
pub struct ClimbOnTopOfPowderSnow;
impl Goal for ClimbOnTopOfPowderSnow {
    goal_clone!();
    fn flags(&self) -> u8 {
        JUMP
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// PanicGoal: flee to a random position after panic-causing damage (or
/// towards water while burning).
#[derive(Clone, Debug)]
pub struct Panic {
    pub speed: f64,
    /// Only the damage of `#panic_environmental_causes` (a wolf).
    pub environmental: bool,
    /// `TurtlePanicGoal`: always look for water this far away, not only
    /// when on fire.
    pub water_range: Option<i32>,
    pos: [f64; 3],
}
impl Panic {
    pub fn new(speed: f64) -> Self {
        Self {
            speed,
            environmental: false,
            water_range: None,
            pos: [0.0; 3],
        }
    }
    /// `TurtlePanicGoal`.
    pub fn looking_for_water(mut self, range: i32) -> Self {
        self.water_range = Some(range);
        self
    }
}
#[path = "animal_panic.rs"]
mod animal_panic;

impl Goal for Panic {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
}
#[path = "goals/stroll.rs"]
mod stroll;
pub use stroll::Stroll;
/// RandomLookAroundGoal.
#[derive(Clone, Debug, Default)]
pub struct RandomLookAround {
    rel: [f64; 2],
    time: i32,
}
impl Goal for RandomLookAround {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// MeleeAttackGoal (and ZombieAttackGoal's raised arms when `zombie`).
#[derive(Clone, Debug)]
pub struct MeleeAttack {
    pub speed: f64,
    pub follow_unseen: bool,
    pub zombie: bool,
    /// `PolarBear.PolarBearMeleeAttackGoal`: rears up before each strike.
    pub polar_bear: bool,
    path_target: [f64; 3],
    recalc: i32,
    pub until_attack: i32,
    last_check: i64,
    raise_arm: i32,
    /// Ticks between attacks (20 unless set).
    pub interval: i32,
    /// Ticks between attacks for a baby, when they differ.
    pub baby_interval: Option<i32>,
    /// The sound played after each strike (a fox's bite).
    pub bite: Option<&'static str>,
}
impl MeleeAttack {
    pub fn new(speed: f64, follow_unseen: bool) -> Self {
        Self {
            speed,
            follow_unseen,
            zombie: false,
            polar_bear: false,
            path_target: [0.0; 3],
            recalc: 0,
            until_attack: 0,
            last_check: 0,
            raise_arm: 0,
            interval: 20,
            baby_interval: None,
            bite: None,
        }
    }
    pub fn zombie(mut self) -> Self {
        self.zombie = true;
        self
    }
    pub fn polar_bear(mut self) -> Self {
        self.polar_bear = true;
        self
    }
    pub fn interval(mut self, ticks: i32) -> Self {
        self.interval = ticks;
        self
    }
    /// `MeleeAttack.create(adult)` for adults, `create(baby)` for babies.
    pub fn intervals(mut self, adult: i32, baby: i32) -> Self {
        self.interval = adult;
        self.baby_interval = Some(baby);
        self
    }
    /// `FoxMeleeAttackGoal.checkAndPerformAttack`: the bite after each strike.
    pub fn bite(mut self, sound: &'static str) -> Self {
        self.bite = Some(sound);
        self
    }
}
impl Goal for MeleeAttack {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn every_tick(&self) -> bool {
        true
    }
}
/// LeapAtTargetGoal.
#[derive(Clone, Debug)]
pub struct LeapAtTarget {
    pub yd: f32,
    target: [f64; 3],
}
impl LeapAtTarget {
    pub fn new(yd: f32) -> Self {
        Self {
            yd,
            target: [0.0; 3],
        }
    }
}
impl Goal for LeapAtTarget {
    goal_clone!();
    fn flags(&self) -> u8 {
        JUMP | MOVE
    }
}
#[derive(Clone, Debug)]
pub struct TargetState {
    unseen: i32,
    pub unseen_memory: i32,
    /// `TargetGoal.getFollowDistance` as a fraction of the follow range.
    pub follow_scale: f64,
}
impl Default for TargetState {
    fn default() -> Self {
        Self {
            unseen: 0,
            unseen_memory: 0,
            follow_scale: 1.0,
        }
    }
}
/// TemptGoal: follow a player holding one of the items.
#[derive(Clone, Debug)]
pub struct Tempt {
    pub speed: f64,
    pub items: Arc<[String]>,
    pub can_scare: bool,
    pub stop_distance: f64,
    p: [f64; 3],
    rot: [f64; 2],
    calm_down: i32,
}
impl Tempt {
    pub fn new(speed: f64, items: Arc<[String]>, can_scare: bool) -> Self {
        Self {
            speed,
            items,
            can_scare,
            stop_distance: 2.5,
            p: [0.0; 3],
            rot: [0.0; 2],
            calm_down: 0,
        }
    }
    fn wants(&self, item: &Option<Arc<str>>) -> bool {
        item.as_ref()
            .is_some_and(|i| self.items.iter().any(|t| t.as_str() == &**i))
    }
}
impl Goal for Tempt {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
}
fn look_angles(look: [f64; 3]) -> (f64, f64) {
    let yaw = (-look[0]).atan2(look[2]).to_degrees();
    let pitch = (-look[1]).asin().to_degrees();
    (yaw, pitch)
}
/// BreedGoal: walk to a partner in love and breed after 60 ticks.
#[derive(Clone, Debug)]
pub struct Breed {
    pub speed: f64,
    partner: Option<u32>,
    love_time: i32,
}
impl Breed {
    pub fn new(speed: f64) -> Self {
        Self {
            speed,
            partner: None,
            love_time: 0,
        }
    }
}
impl Goal for Breed {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
}
/// FollowParentGoal.
#[derive(Clone, Debug)]
pub struct FollowParent {
    pub speed: f64,
    parent: Option<u32>,
    recalc: i32,
}
impl FollowParent {
    pub fn new(speed: f64) -> Self {
        Self {
            speed,
            parent: None,
            recalc: 0,
        }
    }
}
impl Goal for FollowParent {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}
/// `FollowMobGoal` `(mob, speed, stopDistance, areaSize)`: follows the first
/// visible mob of another class within `area` blocks and keeps `stop` away
/// from it (a parrot hovers by the other animals).
///
/// Left out: vanilla also backs away when the followed mob's look control
/// wants this mob's position; only another `FollowMobGoal` mob of a different
/// class would look at it, and parrots are the only ones that have the goal.
#[derive(Clone, Debug)]
pub struct FollowMob {
    speed: f64,
    stop: f64,
    area: f64,
    following: Option<u32>,
    recalc: i32,
    old_water: f32,
}
impl FollowMob {
    pub fn new(speed: f64, stop: f32, area: f32) -> Self {
        Self {
            speed,
            stop: f64::from(stop),
            area: f64::from(area),
            following: None,
            recalc: 0,
            old_water: 0.0,
        }
    }
}
impl Goal for FollowMob {
    fn name(&self) -> &'static str {
        "FollowMobGoal"
    }
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
}
/// RestrictSunGoal: avoid sunlit nodes while it is bright outside.
#[derive(Clone, Debug)]
pub struct RestrictSun;
impl Goal for RestrictSun {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}
/// FleeSunGoal: find shade while burning.
#[derive(Clone, Debug)]
pub struct FleeSun {
    pub speed: f64,
    wanted: [f64; 3],
}
impl FleeSun {
    pub fn new(speed: f64) -> Self {
        Self {
            speed,
            wanted: [0.0; 3],
        }
    }
}
impl Goal for FleeSun {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
}
/// EatBlockGoal (sheep): eat short grass or turn grass blocks to dirt.
#[derive(Clone, Debug, Default)]
pub struct EatBlock {
    tick: i32,
}
impl Goal for EatBlock {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK | JUMP
    }
}

/// BreathAirGoal: swim up to air when the air supply runs low.
#[derive(Clone, Debug, Default)]
pub struct BreathAir;
impl BreathAir {
    /// `BreathAirGoal.findAirPosition`: `BlockPos.neighborColumn` walks the
    /// center, north, east, south and west columns, each bottom to top.
    fn air_position(
        [x, y, z]: [i32; 3],
        mut gives_air: impl FnMut([i32; 3]) -> bool,
    ) -> Option<[i32; 3]> {
        for (dx, dz) in [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0)] {
            for yy in y..=y + 8 {
                let pos = [x + dx, yy, z + dz];
                if gives_air(pos) {
                    return Some(pos);
                }
            }
        }
        None
    }
}
impl Goal for BreathAir {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn interruptable(&self) -> bool {
        false
    }
}
/// TryFindLiquidGoal (water): head for nearby water when stranded on land.
#[derive(Clone, Debug, Default)]
pub struct TryFindWater {
    wanted: [f64; 3],
}
impl Goal for TryFindWater {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
}
/// `MoveToBlockGoal`'s shared state: search, walk to and stay at a block.
#[derive(Clone, Debug, Default)]
pub struct MoveToBlock {
    pub speed: f64,
    pub range: i32,
    pub vertical_range: i32,
    pub vertical_start: i32,
    pub next_start: i32,
    pub try_ticks: i32,
    pub max_stay: i32,
    pub reached: bool,
    pub block: [i32; 3],
}
impl MoveToBlock {
    pub fn new(speed: f64, range: i32, vertical_range: i32) -> Self {
        Self {
            speed,
            range,
            vertical_range,
            ..Default::default()
        }
    }
    /// `findNearestBlock`'s walk out from `base`: the first cell `valid` takes.
    fn walk(&self, base: [i32; 3], mut valid: impl FnMut([i32; 3]) -> bool) -> Option<[i32; 3]> {
        let next = |v: i32| if v > 0 { -v } else { 1 - v };
        let mut y = self.vertical_start;
        while y <= self.vertical_range {
            for r in 0..self.range {
                let mut x = 0;
                while x <= r {
                    let mut z = if x < r && x > -r { r } else { 0 };
                    while z <= r {
                        let p = [base[0] + x, base[1] + y - 1, base[2] + z];
                        if valid(p) {
                            return Some(p);
                        }
                        z = next(z);
                    }
                    x = next(x);
                }
            }
            y = next(y);
        }
        None
    }
    /// The cells [`Self::walk`] can reach from `base`, as a box (inclusive), or
    /// `None` when it walks none.
    fn walk_bounds(&self, base: [i32; 3]) -> Option<([i32; 3], [i32; 3])> {
        if self.range <= 0 || self.vertical_start > self.vertical_range {
            return None;
        }
        // The heights go out from `vertical_start` both ways, past neither it
        // nor `vertical_range` by more than one, a block below.
        let reach = self.range - 1;
        let height = self
            .vertical_start
            .saturating_abs()
            .max(self.vertical_range.saturating_abs())
            .saturating_add(1);
        Some((
            [
                base[0].saturating_sub(reach),
                base[1].saturating_sub(height).saturating_sub(1),
                base[2].saturating_sub(reach),
            ],
            [
                base[0].saturating_add(reach),
                base[1].saturating_add(height),
                base[2].saturating_add(reach),
            ],
        ))
    }
}

#[cfg(test)]
mod breeding_variant_tests {
    use super::*;
    use crate::mobs::{BreedExperience, world::with_test_world};
    use horizon_core::{geometry::Vec3, random::RandomSource};
    use std::sync::Arc;


}

#[cfg(test)]
mod eat_block_tests {
    use super::*;
    use crate::mobs::world::with_built_test_world_rules;

    /// A sheep on a wide grass shelf, ticked long enough for its `EatBlockGoal`
    /// (1 in 1000 a tick) to fire several times: every event it pushed, with
    /// the `mob_griefing` game rule at `griefing` and `mob_drops` on. `tufts`
    /// puts short grass at the sheep's own feet, so the goal takes the
    /// `destroyBlock` half; without it the sheep stands on the grass and takes
    /// the grass-to-dirt half.
    fn sheep_events(griefing: bool, tufts: bool) -> Option<Vec<MobEvent>> {
        sheep_events_hooked(griefing, tufts, None)
    }


    /// `EatBlockGoal.tick` puts the `levelEvent` + `setBlock` behind
    /// `MOB_GRIEFING` and leaves `Mob.ate` outside it, so with the rule off the
    /// goal still runs (`EntityEvent` 10) and still shears the coat, and the
    /// grass stays.
    #[test]
    #[ignore = "daily"]
    fn the_eat_goal_turns_grass_to_dirt_only_with_mob_griefing() {
        let Some(on) = sheep_events(true, false) else {
            return;
        };
        let Some(off) = sheep_events(false, false) else {
            return;
        };
        let dirt = |events: &[MobEvent]| {
            events
                .iter()
                .filter(|e| matches!(e, MobEvent::SetBlockName { name, .. } if *name == "minecraft:dirt"))
                .count()
        };
        let ate = |events: &[MobEvent]| {
            events
                .iter()
                .filter(|e| matches!(e, MobEvent::EntityEvent { event: 10, .. }))
                .count()
        };
        assert!(dirt(&on) > 0, "mob_griefing on: {}", dirt(&on));
        assert_eq!(dirt(&off), 0, "mob_griefing off must leave the grass alone");
        assert!(ate(&off) > 0, "the goal still runs with the rule off");
    }

    /// Grants griefing to mob 1 only: what a mod's `entity_mob_griefing` does.
    struct GriefFor1;
    impl crate::mobs::MobHooks for GriefFor1 {
        fn enderman_anger_suppressed(&self, _: u32, _: &crate::mobs::PlayerView) -> bool {
            false
        }
        fn can_grief(&self, mob: u32, rule: bool) -> bool {
            rule || mob == 1
        }
        fn grief_hooked(&self) -> bool {
            true
        }
        fn living_visibility(&self, _: &crate::mobs::PlayerView, _: u32, visibility: f64) -> f64 {
            visibility
        }
    }

    /// NeoForge's `canEntityGrief`: with the rule off, a mod's yes for this
    /// sheep lets it eat the grass (and only through `MobWorld::can_grief`).
    #[test]
    #[ignore = "daily"]
    fn a_mods_grief_answer_overrides_the_rule_for_its_mob() {
        let Some(off) = sheep_events_hooked(false, false, Some(&GriefFor1)) else {
            return;
        };
        let dirt = off
            .iter()
            .filter(
                |e| matches!(e, MobEvent::SetBlockName { name, .. } if *name == "minecraft:dirt"),
            )
            .count();
        assert!(dirt > 0, "the hook's yes eats the grass with the rule off");
    }

    /// The edible half of the same gate (`ifeq 89` skips
    /// `Level.destroyBlock(pos, false)`): short grass at the sheep's feet
    /// disappears with the rule on and stays with it off, while the goal eats
    /// either way.
    #[test]
    #[ignore = "daily"]
    fn the_eat_goal_breaks_short_grass_only_with_mob_griefing() {
        let Some(on) = sheep_events(true, true) else {
            return;
        };
        let Some(off) = sheep_events(false, true) else {
            return;
        };
        let bitten = |events: &[MobEvent]| {
            events
                .iter()
                .filter(|e| matches!(e, MobEvent::BreakBlock { drop: false, .. }))
                .count()
        };
        let ate = |events: &[MobEvent]| {
            events
                .iter()
                .filter(|e| matches!(e, MobEvent::EntityEvent { event: 10, .. }))
                .count()
        };
        assert!(bitten(&on) > 0, "mob_griefing on: {}", bitten(&on));
        assert_eq!(
            bitten(&off),
            0,
            "mob_griefing off must leave the tufts alone"
        );
        assert!(ate(&off) > 0, "the goal still runs with the rule off");
    }
}

#[cfg(test)]
mod built_by_player_tests {
    use super::*;


    /// `IronGolem.canAttack`: only a golem a player built spares players, so
    /// the save restore drops that target (`Mob.asValidTarget`) while another
    /// mob's `PlayerCreated` byte means nothing.
    #[test]
    fn only_a_golem_spares_the_player_that_built_it() {
        let mut built = mob(Species::IronGolem);
        built.extra.player_created = true;
        assert!(built.built_by_player());
        assert!(!mob(Species::IronGolem).built_by_player());
        let mut zombie = mob(Species::Zombie);
        zombie.extra.player_created = true;
        assert!(!zombie.built_by_player());
    }

    #[test]
    fn pets_and_horses_are_tamable_but_an_allay_is_not() {
        for s in [
            Species::Wolf,
            Species::Parrot,
            Species::Horse,
            Species::Camel,
        ] {
            assert!(mob(s).is_tamable(), "{s:?}");
        }
        for s in [Species::Allay, Species::Fox, Species::Zombie] {
            assert!(!mob(s).is_tamable(), "{s:?}");
        }
    }
}

#[cfg(test)]
mod remove_all_tests {
    use super::*;
    use crate::mobs::world::with_built_test_world_rules;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

    /// A goal that always wants to walk and leaves a mark while it runs, so
    /// `removeAllGoals` has one running to stop.
    #[derive(Clone, Debug)]
    struct Walk;
    impl Goal for Walk {
        goal_clone!();
        fn flags(&self) -> u8 {
            MOVE
        }
        fn name(&self) -> &'static str {
            "Walk"
        }
    }

}
#[cfg(test)]
mod avoid_entity_targeting_tests {
    use super::*;
    use crate::mobs::world::test_player;
    use crate::mobs::world::{with_built_test_world, with_test_world};
    use std::sync::Arc;


    /// `Sensing.hasLineOfSight`: the player is ten blocks away, well inside the
    /// 20 block range, but the wall is between them, so the goal must not pick
    /// them (it did before: only the distance was checked).
    #[test]
    #[ignore = "daily"]
    fn a_rabbit_does_not_flee_a_player_behind_a_wall() {
        if let Some((in_range, picked)) = target(2, true) {
            assert!(in_range, "the player is inside the plain distance range");
            assert!(!picked, "the rabbit picked a player it cannot see");
        }
    }

    /// `LivingEntity.canAttack`: in Peaceful no player is a target at all, so
    /// the goal does not pick them even in plain sight.
    #[test]
    #[ignore = "daily"]
    fn a_rabbit_does_not_flee_in_peaceful() {
        if let Some((in_range, picked)) = target(0, false) {
            assert!(in_range, "the player is inside the plain distance range");
            assert!(!picked, "the rabbit picked a player in Peaceful");
        }
    }

    /// The control: the same world on normal difficulty with nothing in the way
    /// still picks the player.
    #[test]
    #[ignore = "daily"]
    fn a_rabbit_flees_a_player_it_can_see() {
        if let Some((in_range, picked)) = target(2, false) {
            assert!(in_range, "the player is inside the plain distance range");
            assert!(picked, "the rabbit ignored a player in the open");
        }
    }



}


#[cfg(test)]
mod peaceful_targets_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};


}
#[cfg(test)]
mod move_to_block_tests {
    use super::MoveToBlock;

    /// Every cell the walk tries lies in the box `may_find` asks the
    /// palettes about, for the ranges the goals use and odd ones; and the
    /// walk stops at the first cell it is given.
    #[test]
    fn the_walk_stays_in_its_bounds() {
        for range in [-1, 0, 1, 2, 8, 24] {
            for vertical_range in [-2, 0, 1, 2, 3, 6] {
                for vertical_start in [-3, -1, 0, 1, 2] {
                    let goal = MoveToBlock {
                        range,
                        vertical_range,
                        vertical_start,
                        ..Default::default()
                    };
                    for base in [[0, 64, 0], [-17, -60, 31], [100, 319, -100]] {
                        let bounds = goal.walk_bounds(base);
                        let mut walked = 0;
                        let found = goal.walk(base, |p| {
                            walked += 1;
                            let (lo, hi) = bounds.expect("a walk with no bounds");
                            assert!((0..3).all(|a| lo[a] <= p[a] && p[a] <= hi[a]), "{p:?}");
                            false
                        });
                        assert_eq!(found, None);
                        assert_eq!(
                            walked > 0,
                            bounds.is_some(),
                            "{range} {vertical_range} {vertical_start}"
                        );
                    }
                }
            }
        }
        let goal = MoveToBlock::new(1.0, 24, 2);
        assert_eq!(goal.walk([5, 64, 5], |_| true), Some([5, 63, 5]));
    }
}

#[cfg(test)]
mod visibility_hook_tests {
    use crate::mobs::world::{test_player, with_test_world};
    use crate::mobs::{MobHooks, PlayerView};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

    /// A mod's `living_visibility`: a quarter as visible to mob 1, a
    /// hundred times as visible to anyone else.
    struct Scale;
    impl MobHooks for Scale {
        fn enderman_anger_suppressed(&self, _: u32, _: &PlayerView) -> bool {
            false
        }
        fn can_grief(&self, _: u32, rule: bool) -> bool {
            rule
        }
        fn grief_hooked(&self) -> bool {
            false
        }
        fn living_visibility(&self, _: &PlayerView, looking: u32, visibility: f64) -> f64 {
            if looking == 1 {
                visibility * 0.25
            } else {
                visibility * 100.0
            }
        }
    }


}

#[cfg(test)]
mod target_change_hook_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use crate::mobs::{MobHooks, PlayerView, TargetChange};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Cancel(AtomicUsize);
    impl MobHooks for Cancel {
        fn enderman_anger_suppressed(&self, _: u32, _: &PlayerView) -> bool {
            false
        }
        fn can_grief(&self, _: u32, rule: bool) -> bool {
            rule
        }
        fn grief_hooked(&self) -> bool {
            false
        }
        fn living_visibility(&self, _: &PlayerView, _: u32, v: f64) -> f64 {
            v
        }
    }

}



#[cfg(test)]
mod within_home_tests {
    use super::within_home;

    /// `Mob.isWithinHome`: `Vec3i.distSqr` in doubles against the radius
    /// squared as a Java `int`.
    #[test]
    fn within_home_matches_vanilla_at_the_edges() {
        assert!(within_home([10, 64, -5], 6, [12, 64, -2]));
        assert!(
            !within_home([10, 64, -5], 6, [16, 64, -5]),
            "the radius itself is out"
        );
        assert!(!within_home([i32::MIN, 0, 0], 6, [i32::MAX, 0, 0]));
        assert!(!within_home([i32::MIN; 3], i32::MAX, [i32::MAX; 3]));
        // 65 536² wraps to 0; 46 341² wraps negative.
        assert!(!within_home([0, 0, 0], 65_536, [0, 0, 0]));
        assert!(!within_home([0, 0, 0], 46_341, [0, 0, 0]));
        assert!(within_home([0, 0, 0], 46_340, [46_339, 0, 0]));
    }
}

#[cfg(test)]
mod melee_range_tests {
    use super::*;


    /// `Mob.isWithinMeleeAttackRange` with `getActiveItem().get(ATTACK_RANGE)`:
    /// no component reaches sqrt(2.04) - 0.6 (about 0.83); a spear 4.5 and 2.0
    /// times its mob factor 0.5; a patched component with the codec's
    /// defaults (`mob_factor` 1, `min_reach` 0); a removed one the default.
    #[test]
    fn the_active_items_attack_range_sets_the_reach() {
        let sword = mob_holding("minecraft:iron_sword", None);
        assert!(sword.melee_reaches_beside(None, target(&sword, 0.8)));
        assert!(!sword.melee_reaches_beside(None, target(&sword, 0.9)));

        let spear = mob_holding("minecraft:iron_spear", None);
        assert!(spear.melee_reaches_beside(None, target(&spear, 2.2)));
        assert!(!spear.melee_reaches_beside(None, target(&spear, 2.3)));
        assert!(
            !spear.melee_reaches_beside(None, target(&spear, 0.5)),
            "inside the minimum range"
        );
        assert!(spear.melee_reaches_beside(None, target(&spear, 1.1)));

        let long = mob_holding(
            "minecraft:iron_sword",
            Some(r#"{"minecraft:attack_range":{max_reach:6.0f}}"#),
        );
        assert!(long.melee_reaches_beside(None, target(&long, 5.9)));
        assert!(
            long.melee_reaches_beside(None, target(&long, 0.0)),
            "no minimum by default"
        );
        assert!(!long.melee_reaches_beside(None, target(&long, 6.1)));

        let blunt = mob_holding(
            "minecraft:iron_spear",
            Some(r#"{"!minecraft:attack_range":{}}"#),
        );
        assert!(blunt.melee_reaches_beside(None, target(&blunt, 0.5)));
        assert!(!blunt.melee_reaches_beside(None, target(&blunt, 2.0)));
    }

    /// `Mob.getAttackBoundingBox`: a rider's box widened on x and z to its
    /// vehicle's (a 1.4 wide horse under a 0.6 wide zombie) before the reach
    /// is added; `LivingEntity.getHitbox`: a rider is hit from its seat up.
    #[test]
    fn a_riders_attack_box_takes_its_vehicles_width() {
        let mut rider = mob_holding("minecraft:iron_sword", None);
        let at = rider.position;
        let horse = Aabb::new(
            Vec3::new(at.x - 0.7, at.y - 1.0, at.z - 0.7),
            Vec3::new(at.x + 0.7, at.y + 0.6, at.z + 0.7),
        );
        // Its near face 0.9 past the zombie's box: past the default reach of
        // the zombie alone (about 0.83), within it from the horse's side.
        let target = target(&rider, 0.9);
        assert!(!rider.melee_reaches_beside(None, target));
        rider.extra.mount = Some(9);
        rider.seated(9, 0.7, horse);
        assert!(rider.melee_reaches_beside(Some(horse), target));
        // Its own box keeps its height: the horse's lower box adds nothing.
        let below = Aabb::new(
            Vec3::new(at.x + 0.9, at.y - 1.0, at.z),
            Vec3::new(at.x + 1.1, at.y - 0.5, at.z + 1.0),
        );
        assert!(!rider.melee_reaches_beside(Some(horse), below));

        assert_eq!(rider.hitbox().min_position().y, at.y + 0.7);
        // A seat left from another vehicle is not this one's.
        rider.extra.mount = Some(10);
        assert_eq!(rider.hitbox().min_position().y, at.y);
        rider.extra.mount = None;
        assert_eq!(rider.hitbox().min_position().y, at.y);
    }
}

#[cfg(test)]
mod melee_continue_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;



}

#[cfg(test)]
mod breath_air_tests {
    use super::BreathAir;

    #[test]
    fn searches_neighbor_columns_in_vanilla_order() {
        let origin = [10, 20, 30];
        let air = [[10, 27, 29], [11, 21, 30], [10, 22, 31], [9, 23, 30]];
        assert_eq!(
            BreathAir::air_position(origin, |p| air.contains(&p)),
            Some([10, 27, 29])
        );
        assert_eq!(
            BreathAir::air_position(origin, |p| p == [10, 28, 30] || air.contains(&p)),
            Some([10, 28, 30])
        );
        for p in air {
            assert_eq!(BreathAir::air_position(origin, |q| q == p), Some(p));
        }
        // One block above the 9-block columns: not searched.
        assert_eq!(BreathAir::air_position(origin, |p| p == [10, 29, 30]), None);
    }
}

#[cfg(test)]
mod hurt_by_anger_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}


#[cfg(test)]
mod breed_continue_tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}
