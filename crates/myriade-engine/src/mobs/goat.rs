use horizon_core::{geometry::Vec3, random::RandomSource};
use myriade_types::SoundSource;

/// `GoatAi.TIME_BETWEEN_RAMS` and `TIME_BETWEEN_RAMS_SCREAMER`.
const RAMS: (i32, i32) = (600, 6000);
const RAMS_SCREAMER: (i32, i32) = (100, 300);
const RAM_PREPARE_TIME: i64 = 20;
const MIN_RAM_DISTANCE: i32 = 4;
const MAX_RAM_DISTANCE: i32 = 7;
/// `PrepareRamNearestTarget.TIME_OUT_DURATION` and `RamTarget`'s.
const PREPARE_TIME_OUT: i32 = 160;
const RAM_TIME_OUT: i32 = 200;
/// `NEAREST_VISIBLE_LIVING_ENTITIES` reaches 16 blocks.
const SENSE_RANGE: f64 = 16.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Phase {
    #[default]
    Done,
    Prepare,
    /// `RAM_TARGET` is set; `RamTarget` starts on the next tick.
    RamStart,
    Ram,
}



const RAM_COOLDOWN: &str = "ram_cooldown_ticks";
fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}


/// `PrepareRamNearestTarget` and `RamTarget` of the ram activity.
#[derive(Clone, Debug)]
struct Ram;
fn sign(v: i32) -> i32 {
    v.signum()
}
/// `Goat.createHorn`: the instrument is a random pick (by a random seeded
/// with the goat's UUID hash) from `#regular_goat_horns` or, for a screaming
/// goat, `#screaming_goat_horns`.
pub fn create_horn(uuid: [i32; 4], screaming: bool) -> &'static str {
    const REGULAR: [&str; 4] = [
        "minecraft:ponder_goat_horn",
        "minecraft:sing_goat_horn",
        "minecraft:seek_goat_horn",
        "minecraft:feel_goat_horn",
    ];
    const SCREAMING: [&str; 4] = [
        "minecraft:admire_goat_horn",
        "minecraft:call_goat_horn",
        "minecraft:yearn_goat_horn",
        "minecraft:dream_goat_horn",
    ];
    // The UUID's Java hash code: the two halves xored, then folded to an int.
    let [a, b, c, d] = uuid;
    let mut random = horizon_core::random::LegacyRandom::new(i64::from(a ^ b ^ c ^ d));
    let list = if screaming { SCREAMING } else { REGULAR };
    list[random.next_int_bound(list.len() as i32).unwrap_or(0) as usize]
}



#[cfg(test)]
mod long_jump_save_tests {
    use super::*;
    use crate::entity_nbt::{mob_from_nbt, mob_to_nbt};
    use horizon_core::nbt::Tag;
    use std::sync::Arc;


    /// `GoatAi`'s `LONG_JUMP_COOLDOWN_TICKS` is a brain memory, and
    /// `Brain.Packed` saves every memory: `mob_to_nbt`/`mob_from_nbt` are the
    /// pair `Game::mob_tree_nbt` and `Game::load_mob` go through. Horizon
    /// counted the cooldown in the goat's memories and wrote no `Brain`, so a
    /// goat that had just long jumped came back ready to jump again.
    #[test]
    fn a_goat_keeps_its_long_jump_cooldown_through_a_save() {
        let mut m = goat();
        assert_eq!(crate::mobs::frog::jump_cooldown(&m), None);
        crate::mobs::frog::set_jump_cooldown(&mut m, Some(57));
        let saved = mob_to_nbt(&m, [1, 2, 3, 4], None, None, |_| None);
        assert_eq!(
            saved
                .get("Brain")
                .and_then(|b| b.get("memories"))
                .and_then(|b| b.get("minecraft:long_jump_cooling_down")),
            Some(&Tag::compound([("value", Tag::Int(57))])),
            "the save names the ticks left"
        );
        let types = std::collections::HashMap::from([(m.kind.id.clone(), m.kind.clone())]);
        let back = mob_from_nbt(&saved, &types, 2, None).unwrap();
        assert_eq!(
            crate::mobs::frog::jump_cooldown(&back),
            Some(57),
            "the reloaded cooldown"
        );
        // No cooldown writes no memory, even over a compound that names one.
        let saved = mob_to_nbt(&goat(), [1, 2, 3, 4], Some(&saved), None, |_| None);
        assert_eq!(
            saved
                .get("Brain")
                .and_then(|b| b.get("memories"))
                .and_then(|b| b.get("minecraft:long_jump_cooling_down")),
            None,
            "no cooldown, no key"
        );
    }
}

#[cfg(test)]
mod sound_tests {
    use super::*;

}

#[cfg(test)]
mod ram_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};

    fn fake_goat() -> std::sync::Arc<super::super::MobType> {
        let mut kind = crate::mobs::sensor_scan_tests::kind();
        let k = std::sync::Arc::make_mut(&mut kind);
        k.species = super::super::Species::Goat;
        k.food = std::sync::Arc::from(["minecraft:wheat".to_string()]);
        kind
    }


}

