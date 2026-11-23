use horizon_core::random::RandomSource;
use std::sync::Arc;

/// Farm animal climate from the biome tags (`spawns_warm/cold_variant_farm_animals`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Climate {
    #[default]
    Temperate,
    Warm,
    Cold,
}
/// SheepColorSpawnRules.getSheepColor: a weighted list of 100 where the
/// climate's common colour (82) itself rolls pink 1 in 500.
pub fn sheep_color(climate: Climate, random: &mut impl RandomSource) -> u8 {
    // DyeColor ids: white 0, pink 6, gray 7, light gray 8, brown 12, black 15.
    let (list, common): ([(u8, i32); 4], u8) = match climate {
        Climate::Temperate => ([(15, 5), (7, 5), (8, 5), (12, 3)], 0),
        Climate::Warm => ([(7, 5), (8, 5), (0, 5), (15, 3)], 12),
        Climate::Cold => ([(8, 5), (7, 5), (0, 5), (12, 3)], 15),
    };
    let mut r = random.next_int_bound(100).unwrap_or(0);
    for (color, weight) in list {
        if r < weight {
            return color;
        }
        r -= weight;
    }
    if random.next_int_bound(500).unwrap_or(1) < 499 {
        common
    } else {
        6
    }
}

/// `Sheep.readAdditionalSaveData`'s `Color`: `DyeColor.LEGACY_ID_CODEC` reads
/// `Codec.BYTE` (any number tag, `Number.byteValue`), then `ByIdMap.continuous`
/// with `OutOfBoundsStrategy.ZERO` gives white for a byte outside 0..=15; the
/// old `& 15` wrapped the id instead.
pub fn saved_color(value: Option<&horizon_core::nbt::Tag>) -> u8 {
    value
        .and_then(horizon_world::datafix::dynamic::as_number)
        .map(|n| n.byte_value())
        .and_then(|id| u8::try_from(id).ok())
        .filter(|id| *id < 16)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::geometry::Vec3;


    #[test]
    fn chicken_flap_threshold_is_strict_and_advances_once() {
        let mut m = chicken();
        m.anim.flap_speed = 1.0;
        m.ai.steps.fly_dist = 1.0;
        assert!(!chicken_process_flapping(&mut m));
        m.ai.steps.fly_dist = 1.2;
        assert!(chicken_process_flapping(&mut m));
        assert!((m.ai.steps.next_flap - 1.7).abs() < 1.0e-6);
        assert!(!chicken_process_flapping(&mut m));
        m.ai.steps.fly_dist = m.ai.steps.next_flap;
        assert!(!chicken_process_flapping(&mut m));
        m.ai.steps.fly_dist = 1.8;
        assert!(chicken_process_flapping(&mut m));
        assert!((m.ai.steps.next_flap - 2.3).abs() < 1.0e-6);
    }

    #[test]
    fn a_laid_egg_leaves_the_sound_to_the_drop() {
        // Chicken.aiStep (expected): the pitch's two floats, then the timer's
        // nextInt(6000); the sound and `entity_place` wait for the loot roll.
        let mut m = chicken();
        m.extra.egg_time = 1;
        let mut oracle = m.random.clone();
        let mut events = Vec::new();
        chicken_egg_step(&mut m, &mut events);
        let pitch = (oracle.next_float() - oracle.next_float()) * 0.2 + 1.0;
        let next = oracle.next_int_bound(6000).unwrap() + 6000;
        assert_eq!(m.extra.egg_time, next);
        assert_eq!(events.len(), 1, "no sound before the drop");
        assert!(matches!(
            events[0],
            MobEvent::LayEgg { mob: 1, pitch: p, .. } if p == pitch
        ));
    }

    #[test]
    fn a_falling_chicken_flaps_and_a_landed_one_folds_its_wings() {
        let mut m = chicken();
        m.on_ground = true;
        chicken_flap(&mut m);
        assert_eq!(m.anim.flap_speed, 0.0);
        assert_eq!(m.anim.flap, 0.0);
        // In the air flapSpeed rises by 1.2 (clamped to 1), flapping restarts
        // at 1 and decays by 0.9, and flap turns by twice flapping.
        m.on_ground = false;
        chicken_flap(&mut m);
        assert_eq!(m.anim.flap_speed, 1.0);
        assert_eq!(m.anim.flapping, 0.9);
        assert_eq!(m.anim.flap, 1.8);
        chicken_flap(&mut m);
        assert_eq!(m.anim.flap_o, 1.8);
        assert_eq!(m.anim.flap_speed_o, 1.0);
        assert_eq!(m.anim.flap, 3.6);
        // Landed, flapSpeed falls by 0.3 a tick and flapping decays.
        m.on_ground = true;
        chicken_flap(&mut m);
        assert!((m.anim.flap_speed - 0.7).abs() < 1.0e-6);
        assert!((m.anim.flapping - 0.81).abs() < 1.0e-6);
    }
}

#[cfg(test)]
mod sheep_colour_save_tests {
    use super::*;
    use horizon_core::nbt::Tag;

    /// `Sheep.readAdditionalSaveData` reads `Color` with
    /// `DyeColor.LEGACY_ID_CODEC`, whose out-of-bounds strategy is white;
    /// the `& 15` wrapped 99 to light blue (3).
    #[test]
    fn an_unknown_sheep_colour_reads_white() {
        let mut kind = crate::mobs::sensor_scan_tests::kind();
        let k = std::sync::Arc::make_mut(&mut kind);
        k.id = "minecraft:sheep".into();
        k.species = Species::Sheep;
        let types = [("minecraft:sheep".into(), kind)].into();
        let tag = Tag::compound([
            ("id", Tag::String("minecraft:sheep".into())),
            (
                "Pos",
                Tag::List(vec![
                    Tag::Double(0.0f64.to_bits()),
                    Tag::Double(64.0f64.to_bits()),
                    Tag::Double(0.0f64.to_bits()),
                ]),
            ),
            ("Color", Tag::Byte(99)),
        ]);
        let m = crate::entity_nbt::mob_from_nbt(&tag, &types, 1, None).unwrap();
        assert_eq!(m.extra.color, 0, "an unknown dye id is white");
        // Codec.BYTE takes an int tag's low byte: 259 is 3, light blue.
        assert_eq!(saved_color(Some(&Tag::Int(259))), 3);
        assert_eq!(saved_color(Some(&Tag::Byte(-1))), 0);
    }
}
