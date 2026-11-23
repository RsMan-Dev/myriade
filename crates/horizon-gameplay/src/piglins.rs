//! The game side of piglins (`PiglinAi`): spotting and picking up gold,
//! bartering when the admiring is over, and the hand-over of currency.
use crate::piglin_level::PiglinLevel;
use horizon_core::{geometry::Vec3, random::RandomSource};

pub fn piglin_loved<L: PiglinLevel + ?Sized>(l: &L, item: &str) -> bool {
    l.item_in_tag("minecraft:piglin_loved", item)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn armor(points: f64, binding: bool) -> equip::Facts {
        equip::Facts {
            armor: points,
            armor_toughness: 0.0,
            attack_damage: 0.0,
            enchantments: 0,
            damage: 0,
            custom_name: false,
            prevents_armor_change: binding,
        }
    }

    #[test]
    fn better_helmet_replaces_worse_one() {
        // Diamond (3) over iron (2), neither loved.
        let (diamond, iron) = (armor(3.0, false), armor(2.0, false));
        assert!(piglin_replace(
            false,
            false,
            &diamond,
            &iron,
            Slot::Head,
            None
        ));
        assert!(!piglin_replace(
            false,
            false,
            &iron,
            &diamond,
            Slot::Head,
            None
        ));
    }

    #[test]
    fn binding_curse_keeps_the_worn_piece() {
        let (diamond, cursed) = (armor(3.0, false), armor(1.0, true));
        assert!(!piglin_replace(
            false,
            false,
            &diamond,
            &cursed,
            Slot::Head,
            None
        ));
        assert!(!piglin_replace(
            true,
            false,
            &diamond,
            &cursed,
            Slot::Head,
            None
        ));
    }

    #[test]
    fn wanted_beats_comparison() {
        let (gold, diamond) = (armor(2.0, false), armor(3.0, false));
        assert!(piglin_replace(
            true,
            false,
            &gold,
            &diamond,
            Slot::Head,
            None
        ));
        assert!(!piglin_replace(
            false,
            true,
            &diamond,
            &gold,
            Slot::Head,
            None
        ));
    }

    #[test]
    fn preferred_weapon_comparison_by_damage() {
        let mut strong = armor(0.0, false);
        strong.attack_damage = 7.0;
        let mut weak = armor(0.0, false);
        weak.attack_damage = 4.0;
        assert!(piglin_replace(
            false,
            false,
            &strong,
            &weak,
            Slot::MainHand,
            None
        ));
        assert!(!piglin_replace(
            false,
            false,
            &weak,
            &strong,
            Slot::MainHand,
            None
        ));
    }
}
