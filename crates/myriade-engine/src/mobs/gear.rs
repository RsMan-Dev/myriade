//! Enchantments on the gear mobs wear and hold (`Mob.populateDefaultEquipmentEnchantments`,
//! raid buffs): the side table kept beside the stacks in `Extra::equipment`,
//! its saved form, and the combat numbers the enchantments give (Sharpness,
//! Smite, Bane of Arthropods, Fire Aspect, Knockback, Power, Punch, Flame,
//! Quick Charge and the protections).
use super::{DamageKind, Species};
use std::sync::Arc;

/// Equipment slots in the order `Extra` keeps them.
pub const MAIN: usize = 0;
pub const OFF: usize = 1;
pub const HEAD: usize = 2;
pub const CHEST: usize = 3;
pub const LEGS: usize = 4;
pub const FEET: usize = 5;
pub const ARMOR: [usize; 4] = [HEAD, CHEST, LEGS, FEET];
/// The `equipment` keys of each slot.
pub const KEYS: [&str; 6] = ["mainhand", "offhand", "head", "chest", "legs", "feet"];

pub type Levels = Vec<(Arc<str>, u32)>;
/// The same named/direct armor-trim holders as the full item stack.
pub type Trim = crate::item_data::Trim;

/// The enchantments and trims of each equipment slot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gear {
    pub slots: [Levels; 6],
    /// The `minecraft:trim` of each slot's item (a loot table's set_components).
    pub trims: [Option<Trim>; 6],
}
impl Gear {
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Vec::is_empty) && self.trims.iter().all(Option::is_none)
    }
    pub fn level(&self, slot: usize, id: &str) -> u32 {
        self.slots[slot]
            .iter()
            .find(|(e, _)| &**e == id)
            .map_or(0, |(_, l)| *l)
    }
    /// `ItemEnchantments.Mutable.upgrade`: keep the higher level.
    pub fn upgrade(&mut self, slot: usize, id: &str, level: u32) {
        match self.slots[slot].iter_mut().find(|(e, _)| &**e == id) {
            Some((_, l)) => *l = (*l).max(level),
            None => self.slots[slot].push((Arc::from(id), level)),
        }
    }
}


/// `#sensitive_to_smite` and `#sensitive_to_bane_of_arthropods`.
fn undead(s: Species) -> bool {
    matches!(
        s,
        Species::Zombie
            | Species::Husk
            | Species::Drowned
            | Species::ZombieVillager
            | Species::ZombifiedPiglin
            | Species::Skeleton
            | Species::Stray
            | Species::Bogged
            | Species::Parched
            | Species::WitherSkeleton
            | Species::Wither
            | Species::Phantom
            | Species::Zoglin
            | Species::SkeletonHorse
            | Species::ZombieHorse
            | Species::CamelHusk
            | Species::ZombieNautilus
    )
}
fn arthropod(s: Species) -> bool {
    matches!(
        s,
        Species::Spider
            | Species::CaveSpider
            | Species::Silverfish
            | Species::Endermite
            | Species::Bee
    )
}
/// The `minecraft:damage` bonus of a weapon's levels against `target`.
pub fn damage_bonus(levels: &[(Arc<str>, u32)], target: Option<Species>) -> f32 {
    let mut bonus = 0.0;
    for (id, l) in levels {
        let l = *l as f32;
        match &**id {
            "minecraft:sharpness" => bonus += 1.0 + 0.5 * (l - 1.0),
            "minecraft:smite" if target.is_some_and(undead) => bonus += 2.5 * l,
            "minecraft:bane_of_arthropods" if target.is_some_and(arthropod) => bonus += 2.5 * l,
            _ => {}
        }
    }
    bonus
}
/// What a bow adds to its arrows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArrowBonus {
    /// Power: 1 + 0.5 per level above the first.
    pub damage: f32,
    /// Punch: knockback levels.
    pub punch: u32,
    /// Piercing: entities an arrow passes through.
    pub pierce: u32,
}
pub fn protection_points(g: &Gear, kind: DamageKind) -> f32 {
    // getDamageAfterMagicAbsorb returns before enchantments for #bypasses_effects;
    // Protection itself skips #bypasses_invulnerability. Plain generic is protected.
    if kind.bypasses_invulnerability() || kind.bypasses_effects() {
        return 0.0;
    }
    let source = super::Damage::of(kind);
    let fire = source.is_fire();
    let projectile = source.is_projectile();
    let explosion = matches!(kind, DamageKind::Explosion | DamageKind::Fireworks)
        || matches!(kind, DamageKind::Named(n) if n.has(super::NamedDamage::IS_EXPLOSION));
    let fall = matches!(kind, DamageKind::Fall | DamageKind::Stalagmite)
        || matches!(kind, DamageKind::Named(n) if n.has(super::NamedDamage::IS_FALL));
    let mut total = 0.0;
    for slot in ARMOR {
        for (id, l) in &g.slots[slot] {
            let l = *l as f32;
            total += match &**id {
                "minecraft:protection" => l,
                "minecraft:fire_protection" if fire => 2.0 * l,
                "minecraft:blast_protection" if explosion => 2.0 * l,
                "minecraft:projectile_protection" if projectile => 2.0 * l,
                "minecraft:feather_falling" if fall => 3.0 * l,
                _ => 0.0,
            };
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(list: &[(&str, u32)]) -> Levels {
        list.iter().map(|(e, l)| (Arc::from(*e), *l)).collect()
    }
    #[test]
    fn sharpness_adds_half_a_heart_per_level_above_one() {
        let s = levels(&[("minecraft:sharpness", 3)]);
        assert_eq!(damage_bonus(&s, Some(Species::Cow)), 2.0);
    }
    #[test]
    fn smite_and_bane_need_the_right_target() {
        let l = levels(&[("minecraft:smite", 2), ("minecraft:bane_of_arthropods", 1)]);
        assert_eq!(damage_bonus(&l, Some(Species::Zombie)), 5.0);
        assert_eq!(damage_bonus(&l, Some(Species::Spider)), 2.5);
        assert_eq!(damage_bonus(&l, None), 0.0);
        for s in [Species::CamelHusk, Species::ZombieNautilus] {
            assert_eq!(damage_bonus(&l, Some(s)), 5.0);
        }
    }
    #[test]
    fn protections_count_by_damage_kind() {
        let mut g = Gear::default();
        g.upgrade(HEAD, "minecraft:protection", 4);
        g.upgrade(FEET, "minecraft:feather_falling", 4);
        g.upgrade(CHEST, "minecraft:fire_protection", 2);
        assert_eq!(protection_points(&g, DamageKind::MobAttack), 4.0);
        assert_eq!(protection_points(&g, DamageKind::Fall), 16.0);
        assert_eq!(protection_points(&g, DamageKind::OnFire), 8.0);
        assert_eq!(protection_points(&g, DamageKind::Generic), 4.0);
        assert_eq!(protection_points(&g, DamageKind::GenericKill), 0.0);
        assert_eq!(protection_points(&g, DamageKind::OutOfWorld), 0.0);
    }
    #[test]
    fn upgrading_keeps_the_higher_level() {
        let mut g = Gear::default();
        g.upgrade(MAIN, "minecraft:power", 2);
        g.upgrade(MAIN, "minecraft:power", 1);
        assert_eq!(g.level(MAIN, "minecraft:power"), 2);
        assert!(!g.is_empty());
    }
}
