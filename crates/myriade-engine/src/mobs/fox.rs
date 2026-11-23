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












/// `Fox.SleepGoal`: nap in the shade by day when nothing is around.
#[derive(Clone, Debug)]
struct Sleep {
    countdown: i32,
}
impl Sleep {
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

/// A shared goal under the fox's own conditions and start-up: the extra
/// `canUse` of `FoxMeleeAttackGoal` and `FoxLookAtPlayerGoal` and the
/// `clearStates` or `setIsInterested(false)` its subclasses do in `start`.
#[derive(Clone, Debug)]
struct Wrap<G> {
    goal: G,    /// The condition also applies to `canContinueToUse`.
    both: bool,}

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

/// `Fox.StalkPreyGoal`: creep up on a chicken or rabbit more than 6 blocks
/// away, then crouch and keep still once it is within 6.
#[derive(Clone, Debug)]
struct StalkPrey;
impl StalkPrey {
}

/// `Fox.FoxPounceGoal`: once fully crouched with a clear path, leap at the
/// target, tilt with the flight and bite when within 2 blocks, or end up with
/// the nose in snow.
#[derive(Clone, Debug)]
struct Pounce;

/// `Fox.SeekShelterGoal`: by day, or in a thunderstorm, leave open sky for
/// somewhere the sky does not see; a fox standing in a village stays put.
#[derive(Clone, Debug)]
struct SeekShelter {
    speed: f64,
    interval: i32,
    wanted: [f64; 3],
}
impl SeekShelter {
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
