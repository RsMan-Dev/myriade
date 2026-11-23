use crate::containers::BlockEntity;
use horizon_core::{geometry::Vec3, random::RandomSource};
use std::collections::HashMap;

/// `Extra.ints` slots.
const INVULNERABLE: usize = 0;
const ATTACKING: usize = 1;
const STUCK: usize = 2;
const TEARING_DOWN: usize = 3;
const EYES: usize = 4;
const NEXT_FLICKER: usize = 5;
/// `Extra.counter` bits: hurt while protected, and by a player.
const HURT: i32 = 1;
const HURT_BY_PLAYER: i32 = 2;

/// `Creaking.ACTIVATION_RANGE_SQ`.
const ACTIVATION_RANGE_SQ: f64 = 144.0;
/// `Creaking.INVULNERABILITY_ANIMATION_DURATION` and the attack animation.
const INVULNERABILITY_TICKS: i32 = 8;
const ATTACK_TICKS: i32 = 15;
/// `Creaking.TWITCH_DEATH_DURATION`.
const TWITCH_TICKS: i32 = 45;

/// `Vec3.normalize`.
fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < f64::from(1.0e-5_f32) {
        [0.0; 3]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}
#[cfg(test)]
mod portal_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use std::sync::Arc;


    /// `Creaking.canUsePortal`: the creaking's own `!isHeartBound()`; a home
    /// point another species keeps (a wolf's bed) is not a heart, so it does
    /// not stop that mob from travelling.
    #[test]
    fn only_a_creaking_bound_to_a_heart_is_refused_the_portal() {
        let mut m = mob(Species::Creaking);
        assert!(!heart_bound(&m), "born free");
        assert!(
            crate::mobs::mob_can_use_portal(&m),
            "a free creaking travels"
        );
        m.extra.home = Some([4, 64, -7]);
        assert!(heart_bound(&m));
        assert!(
            !crate::mobs::mob_can_use_portal(&m),
            "a creaking bound to its heart must not leave it behind"
        );
        let mut wolf = mob(Species::Wolf);
        wolf.extra.home = Some([4, 64, -7]);
        assert!(!heart_bound(&wolf));
        assert!(crate::mobs::mob_can_use_portal(&wolf));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use std::sync::Arc;

    fn with_world(
        others: &[super::super::Seen],
        f: impl FnOnce(&MobWorld<'_>, &super::super::MobTypes),
    ) -> bool {
        if let Some(root) = std::env::var_os("HORIZON_TEST_RESOURCES") {
            crate::mobs::world::with_resource_test_world(
                std::path::Path::new(&root),
                others,
                true,
                true,
                |_, _| {},
                f,
            )
        } else {
            crate::mobs::world::with_test_world(others, f)
        }
    }

    /// Creaking.activate and deactivate post ENTITY_ACTION, for the sculk sensors and
    /// wardens nearby, independently of whether their target change is accepted.
    #[test]
    fn activating_and_deactivating_post_entity_action() {
        let mut m = creaking();
        let id = m.id;
        let action = |events: &[MobEvent]| {
            events.iter().any(|e| {
                matches!(e, MobEvent::GameEvent { event, source, .. }
                    if *event == "minecraft:entity_action" && *source == id)
            })
        };
        let mut events = Vec::new();
        activation_effects(&mut m, true, &mut events);
        assert!(action(&events), "activating posts entity_action");
        let mut events = Vec::new();
        activation_effects(&mut m, false, &mut events);
        assert!(action(&events), "deactivating posts entity_action");
    }

    struct TargetHook {
        outcome: super::super::TargetChange,
        calls: std::sync::Mutex<Vec<(bool, Option<Who>)>>,
    }
    impl super::super::MobHooks for TargetHook {
        fn enderman_anger_suppressed(&self, _: u32, _: &super::super::PlayerView) -> bool {
            false
        }
        fn can_grief(&self, _: u32, rule: bool) -> bool {
            rule
        }
        fn grief_hooked(&self) -> bool {
            false
        }
        fn living_visibility(&self, _: &super::super::PlayerView, _: u32, visibility: f64) -> f64 {
            visibility
        }
    }

    #[test]
    #[ignore = "daily"]
    fn activation_target_hook_cancels_and_redirects_without_suppressing_side_effects() {
        use super::super::TargetChange;
        let others = [creaking().seen(None, &crate::items::Items::empty())];
        assert!(with_world(&others, |world, _| {
            for outcome in [
                TargetChange::Keep,
                TargetChange::Set(Some(Who::Mob(1))),
                TargetChange::Set(Some(Who::Mob(999))),
                TargetChange::Set(None),
            ] {
                for activating in [true, false] {
                    let hook = TargetHook {
                        outcome,
                        calls: Default::default(),
                    };
                    let mut world = world.clone();
                    world.mob_hooks = Some(&hook);
                    let mut m = creaking();
                    m.id = 2;
                    let previous = (!activating).then_some(Who::Player);
                    monster_brain::set_attack_target(&mut m, previous);
                    let expected = match outcome {
                        TargetChange::Keep | TargetChange::Set(Some(Who::Mob(999))) => previous,
                        TargetChange::Set(target) => target,
                    };
                    let mut events = Vec::new();
                    if activating {
                        activate(&mut m, &world, &mut events);
                    } else {
                        deactivate(&mut m, &world, &mut events);
                    }
                    assert_eq!(monster_brain::attack_target(&m), expected);
                    assert_eq!(m.ai.target, expected);
                    assert_eq!(is_active(&m), activating);
                    assert!(events.iter().any(|event| matches!(
                        event,
                        MobEvent::GameEvent {
                            event: "minecraft:entity_action",
                            source: 2,
                            ..
                        }
                    )));
                    let sound = if activating {
                        "entity.creaking.activate"
                    } else {
                        "entity.creaking.deactivate"
                    };
                    assert!(
                        events.iter().any(
                            |event| matches!(event, MobEvent::Sound(name, ..) if *name == sound)
                        )
                    );
                    assert_eq!(
                        *hook.calls.lock().unwrap(),
                        [(true, activating.then_some(Who::Player))]
                    );
                    hook.calls.lock().unwrap().clear();
                    m.extra.flag = false;
                    assert!(check_can_move(&mut m, &world, &mut events));
                    assert!(
                        hook.calls.lock().unwrap().is_empty(),
                        "an inactive creaking with no players changes no target"
                    );
                }
            }
        }));
    }



    #[test]
    fn a_creaking_with_no_heart_hurts_like_a_monster() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // Creaking.hurtServer: without a home the blow goes to
        // Monster.hurtServer, whose getHurtSound is the hostile hurt — not
        // the generic one.
        let mut m = creaking();
        assert!(m.damage(level, 1.0, Damage::player([0.0, 0.0, 1.0])));
        assert_eq!(
            hurt_sounds(&m),
            ["entity.hostile.hurt"],
            "an unbound creaking is hurt like a monster"
        );
    }

    #[test]
    fn a_heart_bound_creaking_sways_when_a_blow_gets_through() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // Only #bypasses_invulnerability reaches a bound creaking
        // (Creaking.hurtServer hands those straight to Monster), and that is
        // the blow whose hurt sound Creaking.getHurtSound answers with the sway.
        let mut m = creaking();
        m.extra.home = Some([0, 64, 0]);
        assert!(m.damage(level, 1.0, Damage::of(DamageKind::OutOfWorld)));
        assert_eq!(
            hurt_sounds(&m),
            ["entity.creaking.sway"],
            "a heart-bound creaking sways"
        );
    }

    #[test]
    fn a_giant_hurts_and_dies_like_a_monster() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // Giant overrides none of Monster's sounds, so its hurt and death are
        // the hostile pair.
        let mut m = creaking();
        let k = Arc::make_mut(&mut m.kind);
        k.id = "minecraft:giant".into();
        k.species = Species::Giant;
        k.sounds = ["", "entity.generic.hurt", "entity.generic.death"];
        m.health = 100.0;
        assert!(m.damage(level, 1.0, Damage::player([0.0, 0.0, 1.0])));
        assert_eq!(hurt_sounds(&m), ["entity.hostile.hurt"], "a giant's hurt");
        m.outbox.clear();
        // The invulnerability frames are over: the next blow is a full one.
        m.invulnerable = 0;
        assert!(m.damage(level, 200.0, Damage::player([0.0, 0.0, 1.0])));
        assert!(!m.alive());
        assert_eq!(hurt_sounds(&m), ["entity.hostile.death"], "a giant's death");
    }
}

