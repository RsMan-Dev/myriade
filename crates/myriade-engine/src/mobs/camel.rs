use horizon_core::geometry::trig::Mth;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::RandomSource;
use horizon_world::native_load_context::LoopContext;
use std::collections::HashMap;

// Camel state: `egg_time` the mob age of the last pose change (it may be
// negative: a camel finalized at age 0 stood up 53 ticks "ago"),
// `camel_sitting` whether it sits, `anim_ticks` the dash cooldown, `flag2` dashing, `ints[0]` the
// idle animation timeout. AnimState states: sit, sit pose, stand up, idle,
// dash.
const SIT_DOWN: i64 = 40;
const STAND_UP: i64 = 52;
pub const DASH_COOLDOWN: i32 = 55;

/// `Camel.getDefaultDimensions` for `Pose.SITTING`: the adult camel's box
/// 1.43 lower with its eyes at 0.845, or a baby's 0.425 tall box with its
/// eyes at 0.41. Standing, an adult has the type's box and a baby
/// `BABY_STANDING_DIMENSIONS` (the species' baby size).
pub fn sitting_dims(baby: bool) -> (f32, f32, f32) {
    if baby {
        (0.95, 0.425, 0.41)
    } else {
        (1.7, 2.375 - 1.43, 0.845)
    }
}
/// `is_camel`: camels and camel husks (`CamelHusk extends Camel`).
pub fn is_camel(s: super::Species) -> bool {
    matches!(s, super::Species::Camel | super::Species::CamelHusk)
}
/// `CamelAi.RandomSitting`: after 20 seconds in a pose, sit or stand.
#[derive(Clone, Debug)]
struct RandomSitting;



#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::{Anim, Brain, Flags, MobType, Part, Species};
    use crate::mobs::{leash::Holder, world::with_test_world};
    use horizon_core::geometry::Vec3;
    use serde_json::json;
    use std::sync::Arc;




    #[test]
    fn a_fresh_camel_can_move_at_once() {
        // Camel.finalizeSpawn: standUpInstantly, also at age 0.
        let mut m = camel();
        register(&mut m);
        assert_eq!(m.age, 0);
        assert!(!sitting(&m));
        assert!(!refuse_to_move(&m));
        stand_up_instantly(&mut m);
        assert!(!refuse_to_move(&m));
    }

    #[test]
    fn sitting_and_standing_up_take_their_transition_time() {
        let mut m = camel();
        register(&mut m);
        m.age = 100;
        let mut events = Vec::new();
        sit_down(&mut m, &mut events);
        assert!(sitting(&m) && refuse_to_move(&m));
        assert_eq!(pose_time(&m), 0);
        m.age += 40;
        assert!(sitting(&m) && refuse_to_move(&m));
        stand_up(&mut m, &mut events);
        assert!(!sitting(&m) && refuse_to_move(&m));
        m.age += 52;
        assert!(!refuse_to_move(&m));
    }

    #[test]
    fn last_pose_tick_is_a_time_not_a_sign() {
        let mut m = camel();
        register(&mut m);
        m.age = 30;
        // Stood up at game time 1000; now 1030.
        m.extra.egg_time = 0;
        assert_eq!(synced_pose_tick(&m, 1030), 1000);
        m.extra.camel_sitting = true;
        assert_eq!(synced_pose_tick(&m, 1030), -1000);
    }

    #[test]
    fn a_loaded_camel_keeps_its_pose_time() {
        let mut m = camel();
        register(&mut m);
        let types = types(&m);
        let load = |tick: i64, now: i64| {
            let (tag, _) = horizon_core::nbt::parse_snbt(&format!(
                "{{id:\"minecraft:camel\",Pos:[0.0d,0.0d,0.0d],LastPoseTick:{tick}L}}"
            ))
            .unwrap();
            let mut m = crate::entity_nbt::mob_from_nbt(&tag, &types, 2, None).unwrap();
            finish_load(&mut m, now);
            m
        };
        // Standing for a long time: free to move at once.
        let m = load(1000, 5000);
        assert!(!sitting(&m) && !refuse_to_move(&m));
        // Stood up 5 ticks ago: still getting up.
        let m = load(1000, 1005);
        assert!(!sitting(&m) && refuse_to_move(&m));
        assert_eq!(pose_time(&m), 5);
        // Sat 5 ticks ago: pose time 5, not 401.
        let m = load(-1000, 1005);
        assert!(sitting(&m));
        assert_eq!(pose_time(&m), 5);
        // No tag: standing.
        let (tag, _) =
            horizon_core::nbt::parse_snbt("{id:\"minecraft:camel\",Pos:[0.0d,0.0d,0.0d]}").unwrap();
        let mut m = crate::entity_nbt::mob_from_nbt(&tag, &types, 2, None).unwrap();
        finish_load(&mut m, 5000);
        assert!(!refuse_to_move(&m));
    }

    /// A loaded camel that has not ticked yet saves the game time it was
    /// loaded with, however much game time passes (`Camel.addAdditionalSaveData`
    /// writes the synced value unchanged).
    #[test]
    fn a_camel_that_does_not_tick_saves_its_loaded_pose_tick() {
        let mut m = camel();
        register(&mut m);
        let types = types(&m);
        for tick in [200, -200, 0] {
            let (tag, _) = horizon_core::nbt::parse_snbt(&format!(
                "{{id:\"minecraft:camel\",Pos:[0.0d,0.0d,0.0d],LastPoseTick:{tick}L}}"
            ))
            .unwrap();
            let mut m = crate::entity_nbt::mob_from_nbt(&tag, &types, 2, None).unwrap();
            finish_load(&mut m, 5000);
            for now in [5000, 5007, 6000] {
                assert_eq!(synced_pose_tick(&m, now), tick, "{tick} at {now}");
            }
            // Ticking keeps it: age and game time advance together.
            tick_age(&mut m, 5001);
            assert_eq!(synced_pose_tick(&m, 5001), tick);
        }
    }


}
