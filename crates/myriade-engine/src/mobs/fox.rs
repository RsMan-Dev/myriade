//! Foxes (`net.minecraft.world.entity.animal.fox.Fox`): the goal list, the
//! flags byte (sleeping, sitting, crouching, interested, pouncing,
//! faceplanted), stalking and pouncing, napping in the shade by day, seeking
//! shelter, perching, eating berries and the food held in the mouth: its whole
//! `Consumable.onConsume` (the item's effects, its sounds and the eat game
//! event), so a chorus fruit teleports it and a golden apple feeds it, and
//! defending trusted entities from whoever hurt them.
//! The goal that walks to a dropped item is here; the game side takes the
//! items (`Fox.pickUpItem`, `game/mobs/foxes.rs`). A totem in the mouth is a
//! `death_protection` item like any other held one (`Mob::damage`).
use super::{
    MobEvent, MobWorld, Seen, Species, Who,
    goals::{
        Avoid, AvoidEntity, Breed, ClimbOnTopOfPowderSnow, Cx, Float, FollowParent, Goal, JUMP,
        LOOK, LeapAtTarget, LookAtPlayer, MOVE, MeleeAttack, MoveToBlock, NearestTarget, Panic,
        Prey, Stroll, TARGET, TargetState, goal_clone, hide_pos, reduced, target_continue,
        within_home,
    },
    living::wrap,
    raider::heightmap_pos,
    riders,
    villager_brain::{closer_to_center, sections_to_village, village_sections},
};
use horizon_core::position::BlockPos;
use horizon_core::{
    geometry::{Axis, Vec3},
    jmath,
    random::{LegacyRandom, RandomSource},
};
use horizon_items::items::ConsumeEffect;
use horizon_world::consumable::EmissionWorld;
use horizon_world::registry::BlockStateId;
use horizon_world::teleport_consume::TeleportConsumeWorld;
use std::convert::Infallible;

// Fox.DATA_FLAGS_ID, kept in `extra.ints[0]` (and sent to clients as is).
pub const SITTING: i32 = 1;
pub const CROUCHING: i32 = 4;
pub const INTERESTED: i32 = 8;
pub const POUNCING: i32 = 16;
pub const SLEEPING: i32 = 32;
pub const FACEPLANTED: i32 = 64;
pub const DEFENDING: i32 = 128;
/// `Fox.ticksSinceEaten`.
pub(crate) const TICKS_SINCE_EATEN: usize = 1;
/// `Fox.interestedAngle` and `crouchAmount` in `anim.v`, the previous tick's
/// values in `anim.vo`.
const INTERESTED_ANGLE: usize = 0;
const CROUCH_AMOUNT: usize = 1;
/// `Fox.MAX_CROUCH_AMOUNT`.
const MAX_CROUCH_AMOUNT: f32 = 5.0;

/// `SoundEvents.FOX_SCREECH`.
pub const SCREECH: &str = "entity.fox.screech";
/// `EntityReference.getEntity`: the entity a trusted UUID names, in the level.
fn trusted_who(w: &MobWorld, uuid: [i32; 4]) -> Option<Who> {
    if w.player.as_ref().is_some_and(|p| p.uuid == uuid) {
        Some(Who::Player)
    } else {
        w.who_by_uuid(uuid)
    }
}
/// `LivingEntity.getLastHurtByMob` of a trusted entity and its timestamp (0
/// when nothing hurt it).
fn hurt_by(w: &MobWorld, who: Who) -> (Option<Who>, i64) {
    match who {
        // `PlayerView.hurt_by` keeps the 100 ticks `LivingEntity.tick` keeps it.
        Who::Player => w
            .player
            .as_ref()
            .and_then(|p| p.hurt_by)
            .map_or((None, 0), |(by, at)| (Some(by), at)),
        Who::Mob(id) => w
            .seen(id)
            .and_then(|s| s.hurt_by)
            .map_or((None, 0), |(by, at)| (Some(by), i64::from(at))),
    }
}

/// `Fox.DefendTrustedTargetGoal`: take as target whoever last hurt a trusted
/// entity, growling and raising `DEFENDING` while the goal runs.
#[derive(Clone, Debug, Default)]
struct DefendTrusted {
    /// `trustedLastHurtBy`: the attacker `start` targets.
    attacker: Option<Who>,
    /// `timestamp`: the trusted entity's hurt timestamp last defended.
    timestamp: i64,
    /// The timestamp `canUse` read, which `start` stores in the same tick.
    hurt_at: i64,
    /// `TargetGoal`'s caches; the goal never needs sight (`mustSee` false).
    st: TargetState,
}
impl Goal for DefendTrusted {
    goal_clone!();
    fn name(&self) -> &'static str {
        "DefendTrustedTargetGoal"
    }
    fn flags(&self) -> u8 {
        TARGET
    }
}


fn chicken_or_rabbit(s: &Seen) -> bool {
    matches!(s.species, Species::Chicken | Species::Rabbit)
}
fn baby_turtle_on_land(s: &Seen) -> bool {
    s.species == Species::Turtle && s.baby && !s.in_water
}
fn schooling_fish(s: &Seen) -> bool {
    matches!(
        s.species,
        Species::Cod | Species::Salmon | Species::TropicalFish
    )
}


/// The mouth-item snapshot at `Fox.aiStep`'s consumption boundary.
/// It owns the full stack so a listener never observes a later hand value.
#[derive(Clone, Debug)]
pub struct Consumption {
    pub mob: u32,
    pub stack: horizon_items::items::ItemStack,
    pub from: Vec3,
    ticks_since_eaten: i32,
}
impl Consumption {
}






/// `Fox.isConsumableFood`: the item has food and consumable components.
fn eats(w: &MobWorld, stack: &crate::items::ItemStack) -> bool {
    let info = w.items.and_then(|items| items.items.get(&stack.item));
    crate::item_sync::food(stack, info.and_then(|i| i.food)).is_some()
        && stack.consumable(info).is_some()
}



/// The fox as `Consumable`'s user: its own random, and its own ears.
struct Eater<'a> {    events: &'a mut Vec<MobEvent>,
}
impl EmissionWorld for Eater<'_> {
    type Error = Infallible;
    type Stack = crate::items::ItemStack;
    type Sound = String;
    type Random = ();
    type Source = LegacyRandom;
    fn random_source(&mut self, (): ()) -> &mut LegacyRandom {
        &mut self.m.random
    }
    /// `LivingEntity.spawnItemParticles` hands each of the 16 crumbs to
    /// `Level.addParticle`, which a server does nothing with: the four draws a
    /// crumb costs still come off the fox's random, as vanilla spends them.
    fn spawn_item_particles(
        &mut self,
        _: crate::items::ItemStack,
        count: i32,
    ) -> Result<(), Infallible> {
        let m = &mut *self.m;
        let (at, eye) = ([m.position.x, m.position.y, m.position.z], m.eye_y());
        let (pitch, yaw) = (m.head_pitch, m.yaw);
        crate::particles::item_use_particles(
            &mut m.random,
            pitch,
            yaw,
            at,
            eye,
            count.max(0) as u32,
            |_, _| {},
        );
        Ok(())
    }
    fn overrides_consume_sound(&self) -> bool {
        false
    }
    fn consume_sound(&mut self, _: crate::items::ItemStack) -> Result<String, Infallible> {
        unreachable!("the consumable plays its own sound")
    }
    /// `Entity.playSound`: at the fox's feet, and nothing from a silent fox.
    fn play_sound(&mut self, sound: String, volume: f32, pitch: f32) -> Result<(), Infallible> {
        if !self.m.silent {
            let at = [self.m.position.x, self.m.position.y, self.m.position.z];
            self.events
                .push(MobEvent::SoundId(sound, at, volume, pitch));
        }
        Ok(())
    }
}

/// Standalone AI uses the same candidate/effect algorithm and living landing
/// helper as Game's owning-host adapter. Native hooks belong to the Game pause;
/// this borrowed model never manufactures a narrower host for a callback.
struct Teleporter<'a, 'w> {
    world: &'a MobWorld<'w>,    events: &'a mut Vec<MobEvent>,
}
impl TeleportConsumeWorld for Teleporter<'_, '_> {
    type Error = Infallible;
    type Level = ();
    type Random = ();
    type Source = LegacyRandom;
    fn position_field(&self) -> Vec3 {
        self.mob.position
    }
    fn get_random(&mut self) -> Result<(), Infallible> {
        Ok(())
    }
    fn random_source(&mut self, (): ()) -> &mut LegacyRandom {
        &mut self.mob.random
    }
    fn min_y(&mut self, (): ()) -> Result<i32, Infallible> {
        Ok(self.world.world.bottom_y())
    }
    fn logical_height(&mut self, (): ()) -> Result<i32, Infallible> {
        Ok(self.world.world.shape.logical_height)
    }
    fn is_passenger(&mut self) -> Result<bool, Infallible> {
        Ok(self.mob.extra.passenger)
    }
    fn stop_riding(&mut self) -> Result<(), Infallible> {
        riders::stop_riding(self.mob, None);
        Ok(())
    }
    fn position(&mut self) -> Result<Vec3, Infallible> {
        Ok(self.mob.position)
    }
    fn random_teleport(&mut self, target: Vec3) -> Result<bool, Infallible> {
        Ok(super::living::random_teleport(
            self.mob,
            self.world,
            target,
            Some("minecraft:consumable_does_not_teleport_to"),
            self.events,
        ))
    }
    fn teleport_event(&mut self, (): (), old: Vec3) -> Result<(), Infallible> {
        self.events.push(MobEvent::GameEvent {
            event: "minecraft:teleport",
            at: [old.x, old.y, old.z],
            source: self.mob.id,
            state: BlockStateId(0),
        });
        Ok(())
    }
    fn is_fox(&self) -> bool {
        true
    }
    fn teleport_sound(&mut self, (): (), at: Vec3, _: bool) -> Result<(), Infallible> {
        self.events.push(MobEvent::Sound(
            "entity.fox.teleport",
            [at.x, at.y, at.z],
            1.0,
            1.0,
        ));
        Ok(())
    }
    fn block_position(&mut self) -> Result<BlockPos, Infallible> {
        Ok(BlockPos::containing(self.mob.position))
    }
    fn directional_particles(
        &mut self,
        (): (),
        origin: BlockPos,
        offset: i32,
    ) -> Result<(), Infallible> {
        self.events.push(MobEvent::LevelEvent {
            id: 2017,
            pos: [origin.x, origin.y, origin.z],
            data: offset,
        });
        Ok(())
    }
    fn reset_fall_distance(&mut self) -> Result<(), Infallible> {
        self.mob.fall_distance = 0.0;
        Ok(())
    }
    fn reset_impulse(&mut self) -> Result<(), Infallible> {
        // Mob has no separate CurrentImpulseContext state.
        Ok(())
    }
}


/// `Fox.SleepGoal`: nap in the shade by day when nothing is around.
#[derive(Clone, Debug)]
struct Sleep {
    countdown: i32,
}
impl Sleep {
}
impl Goal for Sleep {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK | JUMP
    }
}

/// `Fox.FoxStrollThroughVillageGoal` (`StrollThroughVillageGoal(fox, 200)`):
/// at night near a village, pick a spot toward its heart and walk 10 blocks
/// that way. The fox's `canFoxMove` comes after the base checks (and their
/// draws), and `start` clears its states.
#[derive(Clone, Debug, Default)]
struct StrollThroughVillage {
    /// `StrollThroughVillageGoal.wantedPos`.
    wanted: Option<[i32; 3]>,
}
fn block_pos(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}
fn section_of(p: [i32; 3]) -> [i32; 3] {
    [p[0] >> 4, p[1] >> 4, p[2] >> 4]
}
impl Goal for StrollThroughVillage {
    goal_clone!();
    fn name(&self) -> &'static str {
        "FoxStrollThroughVillage"
    }
    fn flags(&self) -> u8 {
        MOVE
    }
}

/// A shared goal under the fox's own conditions and start-up: the extra
/// `canUse` of `FoxMeleeAttackGoal` and `FoxLookAtPlayerGoal` and the
/// `clearStates` or `setIsInterested(false)` its subclasses do in `start`.
#[derive(Clone, Debug)]
struct Wrap<G> {
    goal: G,    /// The condition also applies to `canContinueToUse`.
    both: bool,}
impl<G: Goal + Clone + 'static> Wrap<G> {
    fn new(goal: G) -> Self {
        Self {
            goal,
            when: None,
            both: false,
            on_start: None,
        }
    }
}
impl<G: Goal + Clone + 'static> Goal for Wrap<G> {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.goal.flags()
    }
    fn interruptable(&self) -> bool {
        self.goal.interruptable()
    }
    fn every_tick(&self) -> bool {
        self.goal.every_tick()
    }
    fn name(&self) -> &'static str {
        self.goal.name()
    }
}

fn signum(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

/// `Fox.FaceplantGoal`: stay put for 40 ticks with the nose in the snow.
#[derive(Clone, Debug)]
struct Faceplant {
    countdown: i32,
}
impl Goal for Faceplant {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK | JUMP
    }
}

/// `Fox.StalkPreyGoal`: creep up on a chicken or rabbit more than 6 blocks
/// away, then crouch and keep still once it is within 6.
#[derive(Clone, Debug)]
struct StalkPrey;
impl StalkPrey {
}
impl Goal for StalkPrey {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
}

/// `Fox.FoxPounceGoal`: once fully crouched with a clear path, leap at the
/// target, tilt with the flight and bite when within 2 blocks, or end up with
/// the nose in snow.
#[derive(Clone, Debug)]
struct Pounce;
impl Goal for Pounce {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | JUMP
    }
    fn interruptable(&self) -> bool {
        false
    }
}

/// `Fox.SeekShelterGoal`: by day, or in a thunderstorm, leave open sky for
/// somewhere the sky does not see; a fox standing in a village stays put.
#[derive(Clone, Debug)]
struct SeekShelter {
    speed: f64,
    interval: i32,
    wanted: [f64; 3],
}
impl SeekShelter {
    fn new(speed: f64) -> Self {
        Self {
            speed,
            interval: reduced(100),
            wanted: [0.0; 3],
        }
    }
}
impl Goal for SeekShelter {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
}

/// `Fox.PerchAndSearchGoal`: now and then sit down and look around two to
/// four times when nothing is going on.
#[derive(Clone, Debug, Default)]
struct Perch {
    rel: [f64; 2],
    look_time: i32,
    looks_remaining: i32,
}
impl Perch {
}
impl Goal for Perch {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
}

/// `BlockState` with one property changed.
fn with_prop(w: &MobWorld, p: [i32; 3], key: &str, value: &str) -> Option<BlockStateId> {
    let mut s = w.registry.state(w.world.block(p[0], p[1], p[2]))?.clone();
    s.properties.insert(key.into(), value.into());
    w.registry.id(&s)
}
/// `FoxEatBerriesGoal.isValidTarget`: a ripe sweet berry bush or glow berries.
fn has_berries(w: &MobWorld, p: [i32; 3]) -> bool {
    match w.block_name(p[0], p[1], p[2]) {
        "minecraft:sweet_berry_bush" => w
            .prop(p[0], p[1], p[2], "age")
            .and_then(|a| a.parse::<i32>().ok())
            .is_some_and(|a| a >= 2),
        "minecraft:cave_vines" | "minecraft:cave_vines_plant" => {
            w.prop(p[0], p[1], p[2], "berries") == Some("true")
        }
        _ => false,
    }
}

/// `Fox.FoxEatBerriesGoal`: walk to a ripe berry bush and pick it after 40 ticks.
#[derive(Clone, Debug)]
struct EatBerries {
    block: MoveToBlock,
    waited: i32,
}
impl EatBerries {
    fn new() -> Self {
        Self {
            block: MoveToBlock::new(f64::from(1.2_f32), 12, 1),
            waited: 0,
        }
    }
}
impl Goal for EatBerries {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | JUMP
    }
    fn every_tick(&self) -> bool {
        true
    }
}


#[cfg(test)]
mod trust_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;


    /// `Fox.addTrustedEntity`: the first free slot, then the second, and a
    /// third player replaces the second (`DATA_TRUSTED_ID_0/1`).
    #[test]
    fn a_fox_trusts_two_players_and_the_third_replaces_the_second() {
        const A: [i32; 4] = [1, 2, 3, 4];
        const B: [i32; 4] = [5, 6, 7, 8];
        const C: [i32; 4] = [9, 10, 11, 12];
        let mut f = fox();
        assert!(!trusts(&f, A), "a fresh fox trusts nobody");
        add_trusted_entity(&mut f, A);
        add_trusted_entity(&mut f, B);
        assert!(trusts(&f, A) && trusts(&f, B));
        add_trusted_entity(&mut f, C);
        assert!(trusts(&f, A), "the first slot stays");
        assert!(!trusts(&f, B) && trusts(&f, C), "the second is replaced");
        assert_eq!(trusted_entities(&f).collect::<Vec<_>>(), vec![A, C]);
        clear_trusted(&mut f);
        assert_eq!(trusted_entities(&f).count(), 0);
        assert!(!trusts(&f, A));
    }
}

#[cfg(test)]
mod trust_breed_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;


    /// `Fox.FoxBreedGoal.breed`: the baby trusts both players who fed the
    /// parents into love, and one of them only once when it is the same.
    #[test]
    fn a_baby_fox_trusts_both_players_who_fed_its_parents() {
        const A: [i32; 4] = [1, 2, 3, 4];
        const B: [i32; 4] = [5, 6, 7, 8];
        let mut baby = fox();
        trust_breeders(&mut baby, Some(A), Some(B));
        assert_eq!(trusted_entities(&baby).collect::<Vec<_>>(), vec![A, B]);
        let mut same = fox();
        trust_breeders(&mut same, Some(A), Some(A));
        assert_eq!(trusted_entities(&same).collect::<Vec<_>>(), vec![A]);
        let mut one = fox();
        trust_breeders(&mut one, Some(A), None);
        assert_eq!(trusted_entities(&one).collect::<Vec<_>>(), vec![A]);
        let mut none = fox();
        trust_breeders(&mut none, None, None);
        assert_eq!(trusted_entities(&none).count(), 0);
    }
}

/// `Fox.FoxSearchForItemsGoal`: walk to a dropped item while the mouth is empty.
#[derive(Clone, Debug)]
struct SearchForItems;
impl Goal for SearchForItems {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
}

#[cfg(test)]
mod pickup_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;


    #[test]
    #[ignore = "daily"]
    fn carry_crouch_advances_only_after_living_step() {
        assert!(crate::mobs::world::with_test_world(&[], |w, _| {
            let mut world = w.clone();
            world.player = Some(crate::mobs::world::test_player(Vec3::new(1.0, 0.0, 0.0)));
            let w = &world;
            let mut f = fox();
            f.ai.target = Some(crate::mobs::Who::Player);
            set_flag(&mut f, CROUCHING, true);
            let mut events = Vec::new();
            assert!(ai_step_until_consumption(&mut f, w, &mut events).is_none());
            assert_eq!(f.anim.v[CROUCH_AMOUNT], 0.0);
            finish_ai_step(&mut f, w, &mut events);
            assert_eq!(f.anim.v[CROUCH_AMOUNT], 0.2);
        }));
    }

    #[test]
    #[ignore = "daily"]
    fn carry_no_ai_keeps_sleeping_in_water_and_animates() {
        assert!(crate::mobs::world::with_test_world(&[], |w, _| {
            let mut f = fox();
            f.no_ai = true;
            f.fluids.in_water = true;
            set_flag(&mut f, SLEEPING | SITTING | CROUCHING | FACEPLANTED, true);
            let mut events = Vec::new();
            finish_ai_step(&mut f, w, &mut events);
            assert!(flag(&f, SLEEPING));
            assert!(flag(&f, SITTING));
            assert!(events.is_empty());
            assert_eq!(f.anim.v[CROUCH_AMOUNT], 0.2);
        }));
    }

    /// `Fox.canHoldItem`: an empty mouth takes anything; a full one only swaps its non-food
    /// for food, and only once the fox has had ticks since it last ate.
    #[test]
    fn a_fox_holds_what_its_mouth_has_room_for() {
        let mut f = fox();
        assert!(
            can_hold_item(&f, false, false),
            "an empty mouth takes any item"
        );
        f.extra.set_main_hand(Some("minecraft:stick".into()));
        f.extra.ints[TICKS_SINCE_EATEN] = 0;
        assert!(
            !can_hold_item(&f, true, false),
            "no food swap at 0 ticks since eating"
        );
        f.extra.ints[TICKS_SINCE_EATEN] = 1;
        assert!(can_hold_item(&f, true, false), "food replaces a stick");
        assert!(
            !can_hold_item(&f, false, false),
            "a stick does not replace a stick"
        );
        assert!(!can_hold_item(&f, true, true), "food does not replace food");
        took_item(&mut f);
        assert_eq!(
            f.extra.ints[TICKS_SINCE_EATEN], 0,
            "taking an item restarts the clock"
        );
    }
}

#[cfg(test)]
mod consume_tests {
    use super::*;
    use crate::mobs::world::with_built_test_world;




    #[cfg(test)]
    mod alertable_tests {
        use super::*;
        use crate::mobs::sensor_scan_tests::kind;
        use crate::mobs::world::{test_player, with_test_world};
        use std::sync::Arc;

    }

    mod sleep_tests {
        use super::*;
        use crate::health::TestLevel;
        use crate::mobs::sensor_scan_tests::kind;
        use crate::mobs::world::with_test_world;
        use std::sync::Arc;

    }
}

#[cfg(test)]
mod seek_shelter_tests {
    use super::*;
    use crate::health::TestLevel;
    use crate::mobs::sensor_scan_tests::kind;
    use crate::mobs::world::with_built_test_world;
    use std::sync::Arc;

    /// A village bell at (5, 1, 5), and the section the fox stands in lit as
    /// shade everywhere but its own block: sky above the fox, a roof beside it.
    fn village_with_shade(
        world: &mut crate::world::World,
        registry: &horizon_world::registry::BlockRegistry,
    ) {
        for cx in -1..=1 {
            for cz in -1..=1 {
                world
                    .chunks
                    .insert((cx, cz), Arc::new(crate::chunk::Chunk::empty(cx, cz)));
            }
        }
        let bell = registry.default_state("minecraft:bell").unwrap();
        world.set(5, 1, 5, bell).unwrap();
        let chunk = Arc::make_mut(world.chunks.get_mut(&(0, 0)).unwrap());
        // Section 4 is y 0..16: the one the fox stands in.
        let section = Arc::make_mut(&mut chunk.sections[4]);
        let lit = crate::chunk::index(0, 1, 0);
        section.light = crate::chunk::Light::Full(Box::new(std::array::from_fn(|i| {
            if i == lit { 0xF0 } else { 0 }
        })));
    }



    /// `Fox.SeekShelterGoal.canUse`: under open sky by day a fox looks for
    /// shade, but not while it stands in a village (`ServerLevel.isVillage`):
    /// there it stays out in the sun.
    #[test]
    #[ignore = "daily"]
    fn a_village_fox_does_not_look_for_shade() {
        let (Some(open), Some(village)) =
            (seeks_shelter(&[]), seeks_shelter(&[villager_of_the_bell()]))
        else {
            // No prepared resources under .local/myriade-engine (CI).
            return;
        };
        assert!(open, "outside a village the fox looks for shade");
        assert!(!village, "inside a village it stays in the open");
    }
}

#[cfg(test)]
mod defend_tests {
    use super::*;
    use crate::health::TestLevel;
    use crate::mobs::sensor_scan_tests::kind;
    use crate::mobs::world::{test_player, with_built_test_world};
    use std::sync::Arc;

    /// The player the fox trusts, and the zombie that just hit it.
    const TRUSTED: [i32; 4] = [3, 1, 4, 1];
    const ZOMBIE: u32 = 9;


}
