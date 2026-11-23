//! How visible a mob is to the mob that looks for it
//! (`LivingEntity.getVisibilityPercent`).

use super::{MobWorld, Seen, Species};

/// What decides how visible a mob is to another: its armour cover and the worn
/// head that lowers its `mob_visibility`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cover {
    /// The share of armour slots filled (`getArmorCoverPercentage`), which
    /// scales how visible an invisible mob is.
    pub armor: f32,
    /// The item on its head, when it is one of the mob heads whose
    /// `mob_visibility` lowers it.
    pub head: Option<&'static str>,
}

/// The mob heads that have a `mob_visibility` component.
const MOB_HEADS: [&str; 4] = [
    "minecraft:skeleton_skull",
    "minecraft:zombie_head",
    "minecraft:creeper_head",
    "minecraft:piglin_head",
];


/// The `mob_visibility` component vanilla gives an item by default
/// (`Item.Properties.loweredMobVisibility`): how visible its wearer is to a
/// mob of type `looking`, 1 for every other item and mob. Only the mob
/// heads have one, each for exactly the types named (a skeleton skull fools
/// a skeleton, not a stray or a bogged), and all four are head equipment,
/// the slot `PlayerView::head` holds.
pub fn mob_visibility(item: &str, looking: Species) -> f32 {
    let lowered = match item {
        "minecraft:skeleton_skull" => looking == Species::Skeleton,
        "minecraft:zombie_head" => looking == Species::Zombie,
        "minecraft:creeper_head" => looking == Species::Creeper,
        "minecraft:piglin_head" => matches!(looking, Species::Piglin | Species::PiglinBrute),
        _ => false,
    };
    if lowered { 0.5 } else { 1.0 }
}

impl Seen {
    /// `LivingEntity.getVisibilityPercent(level, looking)` of a mob before the
    /// visibility event and the clamp: invisibility by armour cover, and the
    /// worn head's `mob_visibility` for the `looking` mob's type. Sneaking is
    /// left out: vanilla reads it through `isDiscrete`, which a mob lacks.
    pub fn visibility_percent(&self, looking: Species) -> f64 {
        let mut visibility = 1.0;
        if self.invisible {
            visibility *= 0.7 * f64::from(self.cover.armor.max(0.1));
        }
        if let Some(head) = self.cover.head {
            visibility *= f64::from(mob_visibility(head, looking));
        }
        visibility
    }
}

impl MobWorld<'_> {
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::goals::{Cx, Goal, NearestTarget};
    use crate::mobs::world::with_test_world;
    use crate::mobs::{MobHooks, PlayerView};
    use horizon_core::geometry::Vec3;

    /// A mod's `living_visibility` that hides every zombie from everyone.
    struct HideZombies;
    impl MobHooks for HideZombies {
        fn enderman_anger_suppressed(&self, _: u32, _: &PlayerView) -> bool {
            false
        }
        fn can_grief(&self, _: u32, rule: bool) -> bool {
            rule
        }
        fn grief_hooked(&self) -> bool {
            false
        }
        fn living_visibility(&self, _: &PlayerView, _: u32, visibility: f64) -> f64 {
            visibility
        }
        fn living_visibility_mob(&self, target: u32, _: u32, _: f64) -> f64 {
            if target == 2 { 0.0 } else { 1.0 }
        }
    }



    /// A mod's visibility answer scales a mob target's range as a player's.
    #[test]
    #[ignore = "daily"]
    fn a_mods_visibility_scales_a_mobs_range_to_a_mob() {
        assert_ne!(golem_targets(10.0, |_| {}, Some(&HideZombies)), Some(true));
        assert_ne!(
            golem_targets(1.5, |_| {}, Some(&HideZombies)),
            Some(false),
            "the floor is 2 blocks"
        );
    }
}
