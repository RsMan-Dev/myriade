use horizon_core::{
    geometry::{Vec3, trig},
    random::RandomSource,
};

// Rabbit's transient state in Extra (none of these clocks are saved).
const JUMP_TICKS: usize = 0;
const JUMP_DURATION: usize = 1;
const MOVE_DELAY: usize = 2;
const IDLE_TIMEOUT: usize = 3;
// Client counters start only when the actual ground jump emits entity event 1.
const CLIENT_JUMP_TICKS: usize = 4;
const CLIENT_JUMP_DURATION: usize = 5;

/// The killer bunny's variant id (`Rabbit.Variant.EVIL`).
pub const EVIL: u8 = 99;

/// `Rabbit.getRandomRabbitVariant(level, pos)`: one `nextInt(100)` from the
/// level's random, then the biome's tags pick the variant. The roll is drawn
/// even where a tag decides alone (the gold rabbit).
pub fn random_variant(white: bool, gold: bool, random: &mut impl RandomSource) -> u8 {
    let roll = random.next_int_bound(100).unwrap_or(0);
    if white {
        if roll < 80 { 1 } else { 3 }
    } else if gold {
        4
    } else if roll < 50 {
        0
    } else if roll < 90 {
        5
    } else {
        2
    }
}


/// `CarrotBlock.MAX_AGE`.
const CARROT_MAX_AGE: i32 = 7;

















#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::{
        Anim, Brain, Flags, MobType, Part, Species,
        anim::{self, Poses},
        body_anim,
        keyframes::Animation,
    };
    use serde_json::json;
    use std::{collections::HashMap, sync::Arc};

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1.0e-6, "{a} != {b}");
    }

    #[test]
    fn summoned_age_selects_the_baby_model_and_texture() {
        let mut template = rabbit();
        let kind = Arc::make_mut(&mut template.kind);
        kind.texture = "entity/rabbit/rabbit_brown".into();
        kind.layers
            .insert("rabbit_baby#main".into(), kind.model.clone());
        let types = HashMap::from([(kind.id.clone(), template.kind.clone())]);
        for (age, baby) in [(-24000, true), (-1, true), (0, false), (6000, false)] {
            let (tag, _) = horizon_core::nbt::parse_snbt(&format!(
                "{{id:\"minecraft:rabbit\",Pos:[0.0d,0.0d,0.0d],Age:{age}}}"
            ))
            .unwrap();
            let m = crate::entity_nbt::mob_from_nbt(&tag, &types, 2, None).unwrap();
            assert_eq!(m.extra.age, age);
            assert_eq!(m.is_baby(), baby);
            assert_eq!(
                crate::mobs::render::body(&m),
                if baby {
                    (
                        "rabbit_baby#main".into(),
                        "entity/rabbit/rabbit_brown_baby".into(),
                    )
                } else {
                    ("rabbit#main".into(), "entity/rabbit/rabbit_brown".into())
                }
            );
        }
    }

    #[test]
    fn registers_vanillas_goals_with_the_three_avoidances() {
        // Rabbit.registerGoals: players 8, wolves 10, monsters 4 (#2780).
        let mut m = rabbit();
        register(&mut m);
        for name in [
            "Float",
            "Panic",
            "Breed",
            "Tempt",
            "RabbitAvoidEntityGoal",
            "Stroll",
            "LookAtPlayer",
        ] {
            assert!(m.ai.goals.has(name), "{name}");
        }
        assert!(!m.ai.goals.has("RandomLookAround"));
    }

    #[test]
    fn love_ticks_survive_a_save() {
        // Animal.addAdditionalSaveData / readAdditionalSaveData: "InLove" (#1396).
        let mut m = rabbit();
        m.extra.in_love = 570;
        let tag = crate::entity_nbt::mob_to_nbt(&m, [0, 0, 0, 1], None, None, |_| None);
        assert_eq!(tag.get("InLove"), Some(&horizon_core::nbt::Tag::Int(570)));
        let types = HashMap::from([(m.kind.id.clone(), m.kind.clone())]);
        let back = crate::entity_nbt::mob_from_nbt(&tag, &types, 2, None).unwrap();
        assert_eq!(back.extra.in_love, 570);
    }

    #[test]
    fn landing_waits_then_real_hop_clock_moves_the_model() {
        let mut m = rabbit();
        m.on_ground = true;
        m.ai.mov.set_wanted(0.0, 0.0, 5.0, 0.6);
        server_ai_step(&mut m, None);
        assert_eq!(m.extra.ints[MOVE_DELAY], 10);
        assert!(!m.ai.jumping);
        for _ in 0..9 {
            server_ai_step(&mut m, None);
        }
        assert!(!m.ai.jumping);
        server_ai_step(&mut m, None);
        assert!(m.ai.jumping);
        assert_eq!(m.extra.ints[JUMP_DURATION], 15);
        assert_eq!(m.anim.states[0].started, None);
        apply_jump(&mut m, 0.2);
        post_tick(&mut m);
        m.age = 1;
        base_tick(&mut m);
        assert_eq!(m.anim.states[0].started, Some(1.0));
        let curve = Animation::parse(&json!({"length": 0.75, "looping": true,
            "bones": {"left_hind_foot": [{"target": "rotation", "keyframes": [[0,[0,0,0],[0,0,0],"linear"],[0.75,[1,0,0],[1,0,0],"linear"]]}]}}));
        let animations = HashMap::from([("RabbitAnimation.HOP".into(), curve)]);
        let mut poses = Poses::new(&m.kind.model);
        let state = anim::state(&m, 0.5);
        body_anim::rabbit(
            &mut poses,
            &state,
            &m.anim.states[0],
            &m.anim.states[1],
            &animations,
        );
        assert!(poses.get("left_hind_foot")[3] > 0.0);
        for _ in 1..15 {
            post_tick(&mut m);
        }
        assert_eq!(m.extra.ints[JUMP_TICKS], 15);
        assert!(m.ai.jumping);
        post_tick(&mut m);
        base_tick(&mut m);
        assert!(!m.ai.jumping);
        assert_eq!(m.anim.states[0].started, None);
    }

    #[test]
    fn panic_and_explicit_requests_use_their_own_hop_timing() {
        let mut m = rabbit();
        m.on_ground = true;
        m.ai.mov.set_wanted(0.0, 0.0, 5.0, 2.2);
        server_ai_step(&mut m, None);
        assert_eq!(m.extra.ints[MOVE_DELAY], 3);
        m.ai.jump = true;
        m.jump_control_tick();
        assert!(m.ai.jumping);
        assert!(!m.ai.jump);
        assert_eq!(m.extra.ints[MOVE_DELAY], 3);
        m.jump_control_tick();
        assert!(m.ai.jumping);
    }

    #[test]
    fn a_new_hop_preserves_a_running_clock_until_the_counter_finishes() {
        let mut m = rabbit();
        m.anim.states[0].start(4.0);
        m.extra.ints[CLIENT_JUMP_TICKS] = 8;
        hop_event(&mut m);
        assert_eq!(m.anim.states[0].started, Some(4.0));
        assert_eq!(m.extra.ints[CLIENT_JUMP_TICKS], 0);
        assert_eq!(m.extra.ints[CLIENT_JUMP_DURATION], 15);
        post_tick(&mut m);
        m.age = 12;
        base_tick(&mut m);
        assert_eq!(m.anim.states[0].started, Some(4.0));
        assert!(m.outbox.is_empty());
    }

    #[test]
    fn deep_water_jump_requests_do_not_start_a_client_hop() {
        let mut m = rabbit();
        m.fluids.in_water = true;
        m.fluids.water = 1.0;
        m.ai.jump = true;
        jump_control(&mut m);
        assert!(m.ai.jumping);
        assert_eq!(m.extra.ints[JUMP_DURATION], 15);
        post_tick(&mut m);
        base_tick(&mut m);
        assert_eq!(m.extra.ints[JUMP_TICKS], 1);
        assert_eq!(m.extra.ints[CLIENT_JUMP_DURATION], 0);
        assert!(m.anim.states[0].started.is_none());
        assert!(
            !m.outbox
                .iter()
                .any(|e| matches!(e, MobEvent::RabbitHop { .. }))
        );
        hop_event(&mut m);
        post_tick(&mut m);
        base_tick(&mut m);
        assert!(m.anim.states[0].started.is_some());
    }

    #[test]
    fn grounded_movement_waits_and_water_uses_its_hop_speed() {
        let mut m = rabbit();
        m.on_ground = true;
        m.ai.mov.set_wanted(0.0, 0.0, 5.0, 0.6);
        prepare_move(&mut m);
        assert_eq!(m.ai.mov.speed_modifier, 0.0);
        assert_eq!(m.ai.zza, 0.0);
        m.ai.jumping = true;
        prepare_move(&mut m);
        assert_eq!(m.ai.mov.speed_modifier, 0.6);
        m.fluids.in_water = true;
        prepare_move(&mut m);
        assert_eq!(m.ai.mov.speed_modifier, 1.5);
        assert_eq!(m.extra.vector[0], 1.5);
        assert!(m.ai.nav.can_float);
        m.extra.flag = false;
        m.ai.mov.set_wanted(0.0, 0.0, 5.0, 2.2);
        server_ai_step(&mut m, None);
        assert_eq!(m.ai.mov.speed_modifier, 1.5);
        assert_eq!(m.extra.ints[MOVE_DELAY], 10);
    }

    #[test]
    fn jump_impulse_scales_strength_terrain_boost_and_baby_kick() {
        let mut m = rabbit();
        m.ai.mov.speed_modifier = 0.6;
        near(f64::from(jump_power(&m, 1.0)), 0.2);
        m.ai.mov.speed_modifier = 2.2;
        near(f64::from(jump_power(&m, 1.0)), 0.3);
        m.horizontal_collision = true;
        near(f64::from(jump_power(&m, 1.0)), 0.5);
        m.attrs.jump_strength = 0.84;
        m.extra.effects.push(crate::mobs::StoredEffect::new(
            "minecraft:jump_boost",
            100,
            1,
        ));
        near(f64::from(jump_power(&m, 0.5)), 0.7);
        apply_jump(&mut m, 0.2);
        near(m.velocity.z, 0.1 / 3.25_f64.sqrt());
        near(m.velocity.y, 0.2 + 0.15 / 3.25_f64.sqrt());
        m.extra.age = -100;
        m.velocity = Vec3::ZERO;
        apply_jump(&mut m, 0.2);
        near(m.velocity.z, 0.1 / 1.25_f64.sqrt());
        near(m.velocity.y, 0.2 + 0.05 / 1.25_f64.sqrt());
        assert_eq!(m.extra.ints[CLIENT_JUMP_DURATION], 15);
    }

    #[test]
    fn idle_timeout_hop_priority_no_ai_and_leash_follow_tick_order() {
        let mut m = rabbit();
        m.age = 7;
        m.extra.ints[IDLE_TIMEOUT] = 0;
        m.extra.ints[CLIENT_JUMP_TICKS] = 2;
        base_tick(&mut m);
        assert_eq!(m.anim.states[1].started, Some(7.0));
        assert!(m.anim.states[0].started.is_none());
        base_tick(&mut m);
        assert_eq!(m.anim.states[0].started, Some(7.0));
        assert!(m.anim.states[1].started.is_none());
        m.extra.ints[CLIENT_JUMP_TICKS] = 0;
        m.extra.ints[IDLE_TIMEOUT] = 0;
        m.no_ai = true;
        base_tick(&mut m);
        assert_eq!(m.extra.ints[IDLE_TIMEOUT], -1);
        assert!(m.anim.states[1].started.is_none());
        m.no_ai = false;
        base_tick(&mut m);
        assert!(m.anim.states[1].started.is_some());
        m.extra.leash = Some(super::super::leash::Holder::Knot([0, 0, 0]));
        base_tick(&mut m);
        assert!(m.anim.states[1].started.is_none());
    }
}
#[cfg(test)]
mod attack_sound_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;


    /// `Rabbit.playAttackSound`: the killer bunny's bite at volume 1 and
    /// `(nextFloat - nextFloat) * 0.2 + 1`, and nothing from another rabbit.
    #[test]
    fn the_killer_bunny_bites_with_its_own_sound() {
        let mut m = rabbit(EVIL);
        let mut probe = m.random.clone();
        let mut events = Vec::new();
        attack_sound(&mut m, &mut events);
        let want = (probe.next_float() - probe.next_float()) * 0.2 + 1.0;
        assert!(
            matches!(
                events.as_slice(),
                [MobEvent::Sound("entity.rabbit.attack", [2.5, 64.0, -1.5], 1.0, p)] if *p == want
            ),
            "{events:?}, pitch {want}"
        );
        // A silent killer bunny still pays the pitch's two draws, then says nothing.
        let mut quiet = rabbit(EVIL);
        quiet.silent = true;
        let mut probe = quiet.random.clone();
        let _ = (probe.next_float(), probe.next_float());
        let mut events = Vec::new();
        attack_sound(&mut quiet, &mut events);
        assert!(events.is_empty(), "{events:?}");
        assert_eq!(
            quiet.random.next_int_bound(1000).unwrap_or(0),
            probe.next_int_bound(1000).unwrap_or(0),
            "two draws, silent or not"
        );
        // A baby killer bunny keeps the plain 1.0 base: the pitch is its own,
        // not the 1.5 a baby's `getVoicePitch` would give.
        let mut baby = rabbit(EVIL);
        baby.extra.age = -24_000;
        let mut probe = baby.random.clone();
        let mut events = Vec::new();
        attack_sound(&mut baby, &mut events);
        let want = (probe.next_float() - probe.next_float()) * 0.2 + 1.0;
        assert!(
            matches!(events.as_slice(), [MobEvent::Sound(_, _, 1.0, p)] if *p == want),
            "{events:?}, pitch {want}"
        );
        // Any other rabbit: no sound and no draw.
        let mut plain = rabbit(0);
        let mut probe = plain.random.clone();
        let mut events = Vec::new();
        attack_sound(&mut plain, &mut events);
        assert!(events.is_empty(), "{events:?}");
        assert_eq!(
            plain.random.next_int_bound(1000).unwrap_or(0),
            probe.next_int_bound(1000).unwrap_or(0),
            "a brown rabbit draws nothing"
        );
    }
}
