//! Parrots: flying (`FlyingMoveControl`, `FlyingPathNavigation`, the wander
//! that heads for trees), flapping and gliding down (`calculateFlapping`),
//! and what `Parrot.aiStep` adds: imitating the mobs around it
//! (`imitateNearbyMobs`, the ambient sound of `getAmbient`) and dancing near
//! a playing jukebox.
use super::{MobEvent, MobWorld, Species, control::MoveKind, nav::NavKind, path::PathType};
use horizon_core::{
    geometry::Vec3,
    random::{LevelRandom, RandomSource},
};

/// `Parrot.MOB_SOUND_MAP` (the happy ghast's is `SoundEvents.EMPTY`: "").
const IMITATIONS: [(Species, &str); 41] = [
    (Species::Blaze, "entity.parrot.imitate.blaze"),
    (Species::Bogged, "entity.parrot.imitate.bogged"),
    (Species::Breeze, "entity.parrot.imitate.breeze"),
    (Species::CamelHusk, "entity.parrot.imitate.camel_husk"),
    (Species::CaveSpider, "entity.parrot.imitate.spider"),
    (Species::Creaking, "entity.parrot.imitate.creaking"),
    (Species::Creeper, "entity.parrot.imitate.creeper"),
    (Species::Drowned, "entity.parrot.imitate.drowned"),
    (
        Species::ElderGuardian,
        "entity.parrot.imitate.elder_guardian",
    ),
    (Species::Endermite, "entity.parrot.imitate.endermite"),
    (Species::Evoker, "entity.parrot.imitate.evoker"),
    (Species::Ghast, "entity.parrot.imitate.ghast"),
    (Species::HappyGhast, ""),
    (Species::Guardian, "entity.parrot.imitate.guardian"),
    (Species::Hoglin, "entity.parrot.imitate.hoglin"),
    (Species::Husk, "entity.parrot.imitate.husk"),
    (Species::Illusioner, "entity.parrot.imitate.illusioner"),
    (Species::MagmaCube, "entity.parrot.imitate.magma_cube"),
    (Species::Parched, "entity.parrot.imitate.parched"),
    (Species::Phantom, "entity.parrot.imitate.phantom"),
    (Species::Piglin, "entity.parrot.imitate.piglin"),
    (Species::PiglinBrute, "entity.parrot.imitate.piglin_brute"),
    (Species::Pillager, "entity.parrot.imitate.pillager"),
    (Species::Ravager, "entity.parrot.imitate.ravager"),
    (Species::Shulker, "entity.parrot.imitate.shulker"),
    (Species::Silverfish, "entity.parrot.imitate.silverfish"),
    (Species::Skeleton, "entity.parrot.imitate.skeleton"),
    (Species::Slime, "entity.parrot.imitate.slime"),
    (Species::Spider, "entity.parrot.imitate.spider"),
    (Species::Stray, "entity.parrot.imitate.stray"),
    (Species::Vex, "entity.parrot.imitate.vex"),
    (Species::Vindicator, "entity.parrot.imitate.vindicator"),
    (Species::Warden, "entity.parrot.imitate.warden"),
    (Species::Witch, "entity.parrot.imitate.witch"),
    (Species::Wither, "entity.parrot.imitate.wither"),
    (
        Species::WitherSkeleton,
        "entity.parrot.imitate.wither_skeleton",
    ),
    (Species::Zoglin, "entity.parrot.imitate.zoglin"),
    (Species::Zombie, "entity.parrot.imitate.zombie"),
    (Species::ZombieHorse, "entity.parrot.imitate.zombie_horse"),
    (
        Species::ZombieNautilus,
        "entity.parrot.imitate.zombie_nautilus",
    ),
    (
        Species::ZombieVillager,
        "entity.parrot.imitate.zombie_villager",
    ),
];

/// `Parrot.MOB_SOUND_MAP`'s sound of a species (vanilla's table: packs go
/// through `neoforge:parrot_imitations`, [`crate::neoforge_mob_maps`]).
pub(crate) fn imitation(species: Species) -> Option<&'static str> {
    IMITATIONS.iter().find(|i| i.0 == species).map(|i| i.1)
}

/// `Parrot.getPitch`.
fn pitch(random: &mut impl RandomSource) -> f32 {
    (random.next_float() - random.next_float()) * 0.2 + 1.0
}



/// The sound a parrot on a player's shoulder makes (`ServerPlayer.
/// playShoulderEntityAmbientSound` after its one-in-200 draw): the voice of a
/// random imitable mob in `nearby` (`Parrot.imitateNearbyMobs`, half the time,
/// those with a `silent` flag excluded after being picked), else its own
/// ambient sound or a rare imitation (`Parrot.getAmbient`). `random` is drawn
/// from in vanilla's order. Returns the sound, its volume and its pitch; the
/// happy ghast's imitation is silent (`""`).
pub fn shoulder_sound(
    maps: &crate::neoforge_mob_maps::NeoForgeMobMaps,
    random: &mut impl RandomSource,
    player_alive: bool,
    player_silent: bool,
    peaceful: bool,
    nearby: &[(&str, bool)],
) -> Option<(&'static str, f32, f32)> {
    if player_alive && !player_silent && random.next_int_bound(2).unwrap_or(1) == 0 {
        let candidates: Vec<_> = nearby
            .iter()
            .filter(|(s, _)| maps.parrot_imitation(s).is_some())
            .collect();
        if !candidates.is_empty() {
            let (species, silent) =
                *candidates[random.next_int_bound(candidates.len() as i32).unwrap_or(0) as usize];
            if !silent {
                let sound = maps.parrot_imitation(species).filter(|s| !s.is_empty())?;
                return Some((sound, 0.7, pitch(random)));
            }
        }
    }
    let sound = if !peaceful && random.next_int_bound(1000).unwrap_or(1) == 0 {
        random_imitation(maps, random)
    } else {
        "entity.parrot.ambient"
    };
    Some((sound, 1.0, pitch(random))).filter(|s| !s.0.is_empty())
}


/// `getAmbient`'s rare draw: a random imitable mob's sound, from vanilla's
/// table or the pack's map (the parrot's own sound when the map is empty).
fn random_imitation(
    maps: &crate::neoforge_mob_maps::NeoForgeMobMaps,
    random: &mut impl RandomSource,
) -> &'static str {
    let sounds = maps.parrot_sounds();
    let len = sounds.map_or(IMITATIONS.len(), <[_]>::len);
    if len == 0 {
        return "entity.parrot.ambient";
    }
    let i = random.next_int_bound(len as i32).unwrap_or(0) as usize;
    match sounds {
        Some(sounds) => sounds[i],
        None => IMITATIONS[i].1,
    }
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
