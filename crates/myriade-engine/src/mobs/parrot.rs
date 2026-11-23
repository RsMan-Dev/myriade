use horizon_core::{
    geometry::Vec3,
    random::{LevelRandom, RandomSource},
};



/// `Parrot.getPitch`.
fn pitch(random: &mut impl RandomSource) -> f32 {
    (random.next_float() - random.next_float()) * 0.2 + 1.0
}












#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use horizon_core::random::LegacyRandom;
    use std::sync::Arc;


    /// `Parrot()`: `FlyingMoveControl(this, 10, false)`, the path maluses,
    /// and `createNavigation`'s `FlyingPathNavigation` (no doors, floats).
    #[test]
    fn a_parrot_flies_by_its_navigation_and_move_control() {
        let m = parrot();
        assert_eq!(
            m.ai.mov.kind,
            MoveKind::Flying {
                max_turn: 10.0,
                hovers: false
            }
        );
        assert_eq!(m.ai.nav.kind, NavKind::Flying);
        assert!(!m.ai.nav.can_open_doors);
        assert!(m.ai.nav.can_float);
        for t in [PathType::FireInNeighbor, PathType::Fire, PathType::Cocoa] {
            assert_eq!(m.malus(t), -1.0);
        }
        assert!(m.ai.goals.has("ParrotWander"));
    }


    /// `Parrot.calculateFlapping`: in the air the wings beat and a fall is
    /// slowed to 0.6; on the ground or a shoulder they fold.
    #[test]
    fn a_falling_parrot_flaps_and_glides() {
        let mut m = parrot();
        m.on_ground = false;
        m.velocity = Vec3::new(0.1, -0.5, 0.0);
        calculate_flapping(&mut m);
        assert!((m.velocity.y + 0.3).abs() < 1.0e-12);
        assert_eq!(m.velocity.x, 0.1);
        assert_eq!(m.anim.flap_speed, 1.0);
        assert_eq!(m.anim.flapping, 0.9);
        assert_eq!(m.anim.flap, 1.8);
        // Rising is not slowed.
        m.velocity = Vec3::new(0.0, 0.2, 0.0);
        calculate_flapping(&mut m);
        assert_eq!(m.velocity.y, 0.2);
        assert_eq!(m.anim.flap_o, 1.8);
        // On a shoulder (a passenger), flapSpeed falls by 0.3 a tick.
        m.extra.mount = Some(7);
        calculate_flapping(&mut m);
        assert!((m.anim.flap_speed - 0.7).abs() < 1.0e-6);
        m.extra.mount = None;
        m.on_ground = true;
        m.velocity = Vec3::new(0.0, -0.0784, 0.0);
        calculate_flapping(&mut m);
        assert!((m.anim.flap_speed - 0.4).abs() < 1.0e-6);
        assert_eq!(m.velocity.y, -0.0784, "no glide on the ground");
    }

    /// `Parrot.onFlap`: a wing beat (`entity.parrot.fly`, 0.15) each half
    /// `flapSpeed` flown.
    #[test]
    fn it_beats_its_wings_as_it_flies() {
        let mut m = parrot();
        m.anim.flap_speed = 1.0;
        let mut events = Vec::new();
        m.ai.steps.fly_dist = 0.5;
        assert!(!process_flapping(&mut m, &mut events));
        assert!(events.is_empty(), "nextFlap starts at 1");
        m.ai.steps.fly_dist = 1.0;
        assert!(!process_flapping(&mut m, &mut events), "strict threshold");
        m.ai.steps.fly_dist = 1.2;
        assert!(process_flapping(&mut m, &mut events));
        assert!(matches!(
            events.as_slice(),
            [MobEvent::Sound("entity.parrot.fly", _, v, _)] if *v == 0.15
        ));
        assert!((m.ai.steps.next_flap - 1.7).abs() < 1.0e-6);
        events.clear();
        assert!(
            !process_flapping(&mut m, &mut events),
            "no second flap without moving"
        );
        m.silent = true;
        m.ai.steps.fly_dist = 1.8;
        assert!(process_flapping(&mut m, &mut events), "silent still flaps");
        assert!(events.is_empty());
        assert!((m.ai.steps.next_flap - 2.3).abs() < 1.0e-6);
    }

    #[test]
    fn vanilla_shoulder_rng_checkpoint_without_data_files() {
        // Java Random's specified 48-bit LCG, seed 4096: coin, candidate,
        // two nextFloat calls for Parrot.getPitch, then nextInt(1_000_000).
        // Literal checkpoint, independent of this helper's implementation.
        let mut random = LegacyRandom::new(4096);
        let (sound, volume, pitch) = shoulder_sound(
            crate::neoforge_mob_maps::NeoForgeMobMaps::vanilla(),
            &mut random,
            true,
            false,
            true,
            &[("minecraft:zombie", false)],
        )
        .unwrap();
        assert_eq!((sound, volume), ("entity.parrot.imitate.zombie", 0.7));
        assert_eq!(pitch.to_bits(), 0x3f8b_df3a);
        assert_eq!(random.next_int_bound(1_000_000), Ok(475_581));
    }

    /// `ServerPlayer.playShoulderEntityAmbientSound`: with a mob to imitate
    /// and a coin flip won, the mob's voice at 0.7; otherwise the parrot's own.
    #[test]
    fn a_shoulder_parrot_imitates_or_chirps() {
        let maps = crate::neoforge_mob_maps::NeoForgeMobMaps::vanilla();
        let nearby = [("minecraft:zombie", false)];
        let mut imitated = 0;
        let mut chirped = 0;
        for n in 0..64_i64 {
            let seed = n.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64);
            let mut probe = LegacyRandom::new(seed);
            let flip = probe.next_int_bound(2).unwrap();
            let mut random = LegacyRandom::new(seed);
            let (sound, volume, _) =
                shoulder_sound(maps, &mut random, true, false, true, &nearby).unwrap();
            if flip == 0 {
                assert_eq!((sound, volume), ("entity.parrot.imitate.zombie", 0.7));
                imitated += 1;
            } else {
                assert_eq!((sound, volume), ("entity.parrot.ambient", 1.0));
                chirped += 1;
            }
        }
        assert!(imitated > 0 && chirped > 0);
        // A silent player (or a dead one) imitates nothing and only chirps.
        let mut random = LegacyRandom::new(1);
        let (sound, volume, _) =
            shoulder_sound(maps, &mut random, true, true, true, &nearby).unwrap();
        assert_eq!((sound, volume), ("entity.parrot.ambient", 1.0));
        // The happy ghast's imitation is silent.
        let ghast = [("minecraft:happy_ghast", false)];
        let silent = (0..64_i64).filter(|&n| {
            let seed = n.wrapping_mul(0x9E37_79B9_7F4A_7C15_u64 as i64);
            let mut r = LegacyRandom::new(seed);
            shoulder_sound(maps, &mut r, true, false, true, &ghast).is_none()
        });
        assert!(silent.count() > 0);
    }
}
