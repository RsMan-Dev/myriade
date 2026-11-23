//! Dolphin goals: `DolphinSwimToTreasureGoal`, `DolphinSwimWithPlayerGoal`
//! and `DolphinJumpGoal`. `extra.flag` is `gotFish` and `extra.home` the
//! `treasurePos` the game found for it (`MobEvent::DolphinTreasure`). `extra.point` is where the
//! game saw the first item floating within 8 blocks (`Dolphin.ALLOWED_ITEMS`).
use super::{
    MobEvent, Who,
    goals::{Cx, Goal, JUMP, LOOK, MOVE, goal_clone, reduced},
};
use crate::blocks::Fluid;
use horizon_core::geometry::trig::Mth;
use horizon_core::random::RandomSource;
use horizon_world::move_gate::MoveGate;

/// `Entity.getMotionDirection`: the horizontal step of the yaw
/// (`Direction.fromYRot`: south, west, north, east).
fn motion_step(yaw: f32) -> [i32; 2] {
    match (yaw / 90.0 + 0.5).floor() as i32 & 3 {
        0 => [0, 1],
        1 => [-1, 0],
        2 => [0, -1],
        _ => [1, 0],
    }
}

/// `DolphinSwimToTreasureGoal`: a dolphin that was fed a fish leads the way to
/// a shipwreck or an ocean ruin within 50 sectors.
#[derive(Clone, Debug, Default)]
pub struct SwimToTreasure {
    stuck: bool,
}
impl SwimToTreasure {
}
impl Goal for SwimToTreasure {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn interruptable(&self) -> bool {
        false
    }
}

/// `DolphinSwimWithPlayerGoal`: swim up to a swimming player and give them
/// Dolphin's Grace.
#[derive(Clone, Debug)]
pub struct SwimWithPlayer {
    speed: f64,
}
impl SwimWithPlayer {
    pub fn new(speed: f64) -> Self {
        Self { speed }
    }
}
impl Goal for SwimWithPlayer {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
}
/// `MobEffectInstance(DOLPHINS_GRACE, 100)`.
fn grace() -> MobEvent {
    MobEvent::EffectPlayer {
        effect: "minecraft:dolphins_grace",
        ticks: 100,
        amplifier: 0,
        ambient: false,
    }
}

/// `DolphinJumpGoal`: leap out of clear water and arc back in.
#[derive(Clone, Debug)]
pub struct Jump {
    interval: i32,
    breached: bool,
}
impl Jump {
    pub fn new(interval: i32) -> Self {
        Self {
            interval: reduced(interval),
            breached: false,
        }
    }
}
impl Goal for Jump {
    goal_clone!();
    fn flags(&self) -> u8 {
        JUMP | MOVE
    }
    fn interruptable(&self) -> bool {
        false
    }
}

/// `FollowPlayerRiddenEntityGoal(mob, AbstractBoat / AbstractNautilus)`: a
/// dolphin swims to the back of a boat or nautilus a player steers within 5
/// blocks of it, then ahead of it in the direction it moves, and goes back
/// when it is more than 12 blocks off.
///
/// Only the player the mob sees (the nearest) is considered.
#[derive(Clone, Debug)]
pub struct FollowPlayerRidden {
    nautilus: bool,
    following: bool,
    recalc: i32,
    leading: bool,
}
impl FollowPlayerRidden {
    pub fn new(nautilus: bool) -> Self {
        Self {
            nautilus,
            following: false,
            recalc: 0,
            leading: false,
        }
    }
    /// `Entity.hasMovedHorizontallyRecently` of the rider, and the player's
    /// own position (its yaw gives the directions).
    fn rider<'a>(&self, cx: &'a Cx) -> Option<&'a super::PlayerView> {
        cx.w.selected_player().filter(|p| {
            p.vehicle
                .as_ref()
                .is_some_and(|v| v.nautilus == self.nautilus)
        })
    }
    fn moved(p: &super::PlayerView) -> bool {
        p.vehicle.as_ref().is_some_and(|v| {
            (v.velocity.x * v.velocity.x + v.velocity.z * v.velocity.z).sqrt() > 1.0e-5
        })
    }
}
impl Goal for FollowPlayerRidden {
    goal_clone!();
    fn name(&self) -> &'static str {
        "FollowPlayerRiddenEntityGoal"
    }
    fn flags(&self) -> u8 {
        0
    }
}

/// `Dolphin.MoveToItemGoal`: swim to a floating item, playing a sound; drops what it holds
/// when it stops and rests for up to 100 ticks.
#[derive(Clone, Debug, Default)]
pub struct MoveToItem {
    cooldown: i64,
}
impl Goal for MoveToItem {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
}
/// `Dolphin.PlayWithItemsGoal`: a dolphin that holds something throws it away.
#[derive(Clone, Debug, Default)]
pub struct PlayWithItems;
impl Goal for PlayWithItems {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
}

#[cfg(test)]
mod item_tests {
    use super::*;
    use crate::items::ItemStack;
    use crate::mobs::{Species, sensor_scan_tests::kind};
    use horizon_core::geometry::Vec3;


    /// `Dolphin.ItemGoal.dropItem`: a dolphin facing south (yaw 0, level) throws the item
    /// forward (+Z) at 0.3, off by at most 0.02 sideways, with a pickup delay of 40, and
    /// its mouth is empty after. An empty mouth throws nothing.
    #[test]
    fn a_dolphin_throws_its_item_ahead_and_empties_its_mouth() {
        let mut m = dolphin();
        let mut events = Vec::new();
        assert!(!drop_item(&mut m, &mut events), "nothing to throw");
        assert!(events.is_empty());
        let stack = ItemStack {
            item: "minecraft:cod".into(),
            count: 3,
            damage: 0,
            extra: None,
        };
        crate::mobs::finalize::set_item_stack(
            &mut m,
            crate::mobs::equip::Slot::MainHand.index(),
            stack,
        );
        assert!(drop_item(&mut m, &mut events));
        let [
            MobEvent::ThrowItem {
                stack,
                motion,
                pickup_delay,
                ..
            },
        ] = events.as_slice()
        else {
            panic!("one thrown item, got {events:?}");
        };
        assert_eq!(
            (&*stack.item, stack.count, *pickup_delay),
            ("minecraft:cod", 3, 40)
        );
        assert!(
            motion[0].abs() <= 0.02 + 1e-6,
            "sideways at most 0.02: {motion:?}"
        );
        assert!(
            (motion[2] - 0.3).abs() <= 0.02 + 1e-6,
            "ahead at 0.3: {motion:?}"
        );
        assert_eq!(motion[1], 0.0, "level, so no climb");
        assert!(m.extra.main_hand_id().is_none(), "the mouth is empty");
    }
}

#[cfg(test)]
mod target_tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use crate::mobs::{Species, sensor_scan_tests::kind};
    use horizon_core::geometry::Vec3;


    /// `Dolphin.canAttack`: a baby takes nobody as its target, so a hit on it
    /// is not answered; an adult does take the one that hurt it.
    #[test]
    #[ignore = "daily"]
    fn a_baby_dolphin_spares_whatever_hurt_it() {
        let ran = with_test_world(&[], |w, _| {
            assert!(
                dolphin(true).spares_target(w, Who::Mob(7)),
                "a baby dolphin fights nobody"
            );
            assert!(
                !dolphin(false).spares_target(w, Who::Mob(7)),
                "an adult dolphin takes its attacker"
            );
        });
        assert!(ran, "vanilla data is missing");
    }
}

#[cfg(test)]
mod treasure_play_tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use crate::mobs::{Species, sensor_scan_tests::kind};
    use horizon_core::geometry::Vec3;


    /// `DolphinSwimToTreasureGoal.tick`: `level.getRandom().nextInt(this.adjustedTickDelay(80))`,
    /// the level's random and 40 ticks for a goal that does not tick every tick.
    #[test]
    #[ignore = "daily"]
    fn a_dolphin_hearts_its_treasure_on_the_levels_random_every_40_ticks() {
        let mut out = None;
        if !with_test_world(&[], |w, _| {
            let mut runs = Vec::new();
            // 119: `new Random(119).nextInt(40) == 0`, but `nextInt(80) != 0`;
            // 1: neither is 0. Vanilla draws 40 from the level's random.
            for seed in [119, 1] {
                let mut m = dolphin();
                // A treasure far enough that `canContinueToUse` holds.
                m.extra.home = Some([40, 64, 40]);
                let mut lr = crate::health::TestLevel::new(seed);
                let mut events = Vec::new();
                let mut cx = Cx {
                    w,
                    level: &mut lr.level(),
                    events: &mut events,
                };
                let mut goal = SwimToTreasure::default();
                goal.tick(&mut m, &mut cx);
                let heart = events
                    .iter()
                    .any(|e| matches!(e, MobEvent::AroundSelf { event: 38, .. }));
                runs.push((seed, goal.stuck, heart));
            }
            out = Some(runs);
        }) {
            return;
        }
        for (seed, stuck, _) in out.as_ref().unwrap() {
            assert!(!stuck, "seed {seed}: the dolphin found no spot to swim to");
        }
        let runs = out.unwrap();
        assert!(
            runs[0].2,
            "seed 119: the level's random says heart (nextInt(40) == 0)"
        );
        assert!(
            !runs[1].2,
            "seed 1: the level's random says no heart (nextInt(40) != 0)"
        );
    }

    /// `Dolphin.MoveToItemGoal.start`: `playSound(DOLPHIN_PLAY, 1.0, 1.0)`,
    /// which `Entity.playSound` drops for a silent dolphin.
    #[test]
    #[ignore = "daily"]
    fn a_silent_dolphin_makes_no_sound_swimming_to_an_item() {
        let ran = with_test_world(&[], |w, _| {
            for silent in [false, true] {
                let mut m = dolphin();
                m.extra.point = Some([8.5, 64.0, 0.5]);
                m.silent = silent;
                let mut lr = crate::health::TestLevel::new(0);
                let mut events = Vec::new();
                let mut cx = Cx {
                    w,
                    level: &mut lr.level(),
                    events: &mut events,
                };
                MoveToItem::default().start(&mut m, &mut cx);
                let sounds: Vec<_> = events
                    .iter()
                    .filter_map(|e| match e {
                        MobEvent::Sound(name, at, volume, pitch) => {
                            Some((*name, *at, *volume, *pitch))
                        }
                        _ => None,
                    })
                    .collect();
                if silent {
                    assert!(sounds.is_empty(), "a silent dolphin squeaked: {sounds:?}");
                } else {
                    let feet = [m.position.x, m.position.y, m.position.z];
                    assert_eq!(sounds, vec![("entity.dolphin.play", feet, 1.0, 1.0)]);
                }
            }
        });
        assert!(ran, "vanilla data is missing");
    }
}
