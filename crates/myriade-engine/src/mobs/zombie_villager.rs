use horizon_core::nbt::{Compound, Tag};
use horizon_core::random::RandomSource;

/// `VILLAGER_CONVERSION_WAIT_MIN` and the width of the random range.
pub const WAIT_MIN: i32 = 3600;
pub const WAIT_SPREAD: i32 = 2401;
/// `MAX_SPECIAL_BLOCKS_COUNT` and `SPECIAL_BLOCK_RADIUS`.
const MAX_SPECIAL_BLOCKS: i32 = 14;
const RADIUS: i32 = 4;
const SPEEDS_UP_CURING: &str = "minecraft:speeds_up_zombie_villager_curing";

/// A cure under way: ticks left and the player who started it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cure {
    pub time: i32,
    pub starter: Option<[i32; 4]>,
}





/// `ConversionTime` (-1: not converting) and `ConversionPlayer`.
fn put_cure(map: &mut Compound, cure: Option<Cure>) {
    map.insert(
        "ConversionTime".into(),
        Tag::Int(cure.map_or(-1, |c| c.time)),
    );
    if let Some(starter) = cure.and_then(|c| c.starter) {
        map.insert("ConversionPlayer".into(), Tag::IntArray(starter.to_vec()));
    }
}
fn parse_cure(tag: &Tag) -> Option<Cure> {
    let time = match tag.get("ConversionTime") {
        Some(Tag::Int(v)) => *v,
        _ => -1,
    };
    (time != -1).then(|| Cure {
        time,
        starter: crate::save::uuid_from_tag(tag.get("ConversionPlayer")),
    })
}


#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::{geometry::Vec3, random::LegacyRandom};

    fn kind() -> std::sync::Arc<super::super::MobType> {
        let mut kind = super::super::sensor_scan_tests::kind();
        let data = std::sync::Arc::make_mut(&mut kind);
        data.id = "minecraft:zombie_villager".into();
        data.species = Species::ZombieVillager;
        data.eye_height = 1.5;
        kind
    }




    #[test]
    fn loading_a_cure_starts_converting() {
        let mut m = zombie_villager();
        m.add_effect("minecraft:weakness", 400, 0);
        let mut map = Compound::new();
        map.insert("ConversionTime".into(), Tag::Int(7));
        read_nbt(&mut m, &Tag::Compound(map));
        assert!(is_converting(&m));
        assert!(m.extra.effects.iter().all(|e| e.id != "minecraft:weakness"));
        assert!(
            m.extra
                .effects
                .iter()
                .any(|e| e.id == "minecraft:strength" && e.duration == 7)
        );
    }
    #[test]
    fn a_cure_round_trips_through_nbt() {
        let cure = Cure {
            time: 4200,
            starter: Some([1, 2, 3, 4]),
        };
        let mut map = Compound::new();
        put_cure(&mut map, Some(cure));
        assert_eq!(parse_cure(&Tag::Compound(map)), Some(cure));
    }
    #[test]
    fn no_cure_saves_minus_one() {
        let mut map = Compound::new();
        put_cure(&mut map, None);
        assert_eq!(map.get(&"ConversionTime".into()), Some(&Tag::Int(-1)));
        assert_eq!(parse_cure(&Tag::Compound(map)), None);
    }

    #[test]
    fn starting_a_cure_broadcasts_even_when_silent_without_drawing_sound_randomness() {
        for silent in [false, true] {
            let mut m = zombie_villager();
            m.silent = silent;
            m.add_effect("minecraft:weakness", 100, 0);
            let mut random = m.random.clone();
            let starter = Some([1, 2, 3, 4]);
            let mut events = Vec::new();

            start_converting(&mut m, starter, 4000, &mut events);

            assert_eq!(
                cure(&m),
                Some(&Cure {
                    time: 4000,
                    starter
                })
            );
            assert!(!m.has_effect("minecraft:weakness"));
            assert!(m.extra.effects.contains(&crate::mobs::StoredEffect::new(
                "minecraft:strength",
                4000,
                0
            )));
            assert_eq!(events.len(), 1);
            let MobEvent::EntityEvent { mob, event, at } = events[0] else {
                panic!("Starting a cure must broadcast an entity event");
            };
            assert_eq!((mob, event, at), (42, 16, [1.25, 2.0, 3.75]));
            assert_eq!(m.random.next_long(), random.next_long());
        }
    }

    #[test]
    fn cure_sound_draws_volume_before_pitch_at_eye_height() {
        let mut m = zombie_villager();
        let Some(MobEvent::Sound(name, at, volume, pitch)) = cure_sound(&mut m) else {
            panic!("A non-silent zombie villager must play the cure sound");
        };
        assert_eq!(name, "entity.zombie_villager.cure");
        assert_eq!(at, [1.25, 3.5, 3.75]);
        // The first two java.util.Random(0) floats, volume before pitch.
        assert_eq!(volume.to_bits(), 0x3fdd_905a);
        assert_eq!(pitch.to_bits(), 0x3f61_cb52);
        assert_eq!(m.random.next_float().to_bits(), 0x3e76_4f2c);
    }

}
