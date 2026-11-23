use super::finalize::{self, Attr};
use horizon_world::attribute_map::ValueKind;
use horizon_world::attributes::entity::{Catalog, EntityAttributes};
use horizon_world::attributes::{Instance, Observer};
use std::convert::Infallible;
use std::sync::Arc;


struct Calculation;
impl Observer for Calculation {
    type Error = Infallible;
    fn dirty(&mut self, _: &mut Instance) -> Result<(), Infallible> {
        Ok(())
    }
}

/// The attribute's value on the mob's own map, with `bonus` counted in the
/// additive phase; none without a map, or for an attribute its catalog lacks.
fn effective(map: Option<&EntityAttributes>, name: &str, base: f64, bonus: f64) -> Option<f64> {
    let map = map?;
    let key = map.catalog().id(name)?;
    if let Some(source) = map.instance(key) {
        // Equipment and the covered shell contribute in the additive
        // phase, before multiplicative modifiers and effective clamping.
        let mut value = source.clone();
        value
            .set_base_value(source.base_value() + bonus, &mut Calculation)
            .unwrap();
        return Some(value.value());
    }
    let definition = map.catalog().attribute(key).definition;
    Some(definition.sanitize(map.base_value(key).unwrap_or(base) + bonus))
}


/// Absorption's `max_absorption` modifier (`MobEffects.ABSORPTION`:
/// +4 per level, `ADD_VALUE`) for the effects a mob has.
pub fn absorption_bonus(effects: &[super::StoredEffect]) -> f64 {
    effects
        .iter()
        .find(|e| e.id == "minecraft:absorption")
        .map_or(0.0, |e| 4.0 * f64::from(e.amplifier + 1))
}
/// `getMaxAbsorption` from a mob's attribute map (its base and explicit
/// modifiers, `/attribute` keeps them there) and the Absorption effect's
/// `ADD_VALUE` modifier. `AttributeInstance.calculateValue` sums `ADD_VALUE`
/// into the base before the multiplying operations, so the effect counts in
/// that phase; the stored base and modifiers are not changed.
/// None for a catalog without the attribute.
pub fn max_absorption_of(map: &EntityAttributes, default: f64, bonus: f64) -> Option<f64> {
    effective(Some(map), "minecraft:max_absorption", default, bonus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::{Damage, DamageKind, Species};
    use horizon_core::geometry::Vec3;
    use horizon_world::attributes::{Modifier, Operation, entity::Catalog};
    use std::sync::Arc;


    #[test]
    fn effective_mob_armor_controls_damage_and_fractional_armor_floors() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        for (armor, toughness, source, expected) in [
            (20.0, 8.0, DamageKind::Cactus, 999039),
            (0.0, 8.0, DamageKind::Cactus, 996000),
            (0.5, 8.0, DamageKind::Cactus, 996000),
            (20.0, 8.0, DamageKind::SonicBoom, 996000),
        ] {
            let (mut mob, _) = pig(armor, toughness);
            assert!(mob.damage(level, 4.0, Damage::of(source)));
            assert_eq!((f64::from(mob.health) * 1000.0) as i32, expected);
        }
    }


    #[test]
    fn equipment_is_counted_once_before_effective_limits() {
        let (mut mob, catalog) = pig(20.0, 8.0);
        wear(&mut mob, &catalog, 3.0, Operation::AddValue);
        assert_eq!(values(&mob), (23.0, 8.0));
        let (mut mob, catalog) = pig(10.0, 8.0);
        wear(&mut mob, &catalog, 0.5, Operation::AddMultipliedTotal);
        assert_eq!(values(&mob), (15.0, 8.0));
        let (mob, _) = pig(100.0, 100.0);
        assert_eq!(values(&mob), (30.0, 20.0));
    }

    /// The effect bonus counted in the `ADD_VALUE` phase gives what an
    /// `Instance` carrying it as one more `ADD_VALUE` modifier gives, under
    /// the multiplying operations and the range clamp (the cases the pinned
    /// 26.3 server answered 36, 28, 60, 2042 and 2048 for), and the stored
    /// base and modifiers stay as they were.
    #[test]
    fn absorption_bonus_is_an_add_value_modifier_under_every_operation() {
        use horizon_world::attributes::{Modifier, Operation};
        let bits = |v: f64| format!("{:016x}", v.to_bits());
        let catalog = Arc::new(Catalog::from_json(&serde_json::json!({
            "data_version": horizon_world::DATA_VERSION,
            "attributes": [
                {"id":"minecraft:max_absorption", "default":bits(0.0), "min":bits(0.0), "max":bits(2048.0), "syncable":true}
            ],
            "entities": {"minecraft:pig":{"minecraft:max_absorption":bits(0.0)}}
        })).unwrap());
        let key = catalog.id("minecraft:max_absorption").unwrap();
        let cases: [(f64, &[(f64, Operation)], f64, f64); 5] = [
            (20.0, &[(0.5, Operation::AddMultipliedBase)], 4.0, 36.0),
            (10.0, &[(1.0, Operation::AddMultipliedTotal)], 4.0, 28.0),
            (
                10.0,
                &[
                    (2.0, Operation::AddValue),
                    (0.5, Operation::AddMultipliedBase),
                    (1.0, Operation::AddMultipliedTotal),
                ],
                8.0,
                60.0,
            ),
            (2048.0, &[(-10.0, Operation::AddValue)], 4.0, 2042.0),
            (2046.0, &[], 4.0, 2048.0),
        ];
        for (base, mods, bonus, java) in cases {
            let mut map = catalog.create("minecraft:pig").unwrap();
            map.edit(key, |value, observer| {
                value.set_base_value(base, observer).unwrap();
                for (n, (amount, op)) in mods.iter().enumerate() {
                    let m = Modifier::new(&format!("h203:m{n}"), *amount, *op).unwrap();
                    value.add_permanent(m, observer).unwrap();
                }
            });
            let mut with_effect = map.instance(key).unwrap().clone();
            with_effect
                .add_permanent(
                    Modifier::new("minecraft:effect.absorption", bonus, Operation::AddValue)
                        .unwrap(),
                    &mut Calculation,
                )
                .unwrap();
            let got = max_absorption_of(&map, 0.0, bonus).unwrap();
            assert_eq!(got, with_effect.value(), "base {base}");
            assert_eq!(got, java, "base {base}: the pinned Java value");
            assert_eq!(
                map.base_value(key).unwrap(),
                base,
                "the stored base is unchanged"
            );
            assert_eq!(
                max_absorption_of(&map, 0.0, 0.0).unwrap(),
                map.value(key).unwrap()
            );
        }
        // No map yet: the type default with the bonus, in the bounds its
        // type took from the catalog.
        let bounds = finalize::AttrBounds::of(&catalog);
        assert_eq!(bounds.sanitize_max_absorption(8.0), 8.0);
        assert_eq!(bounds.sanitize_max_absorption(-4.0), 0.0);
        assert_eq!(bounds.sanitize_max_absorption(2052.0), 2048.0);
    }


    /// MagmaCube.setSize writes `size * 3` into `Attributes.ARMOR`, and
    /// `LivingEntity.getArmorValue` reads that base back (no armor map needed).
    #[test]
    fn magma_cube_armor_is_its_size_times_three_and_softens_hits() {
        for size in [2, 4] {
            assert_eq!(
                values(&magma_cube(size)),
                ((size * 3) as f32, 0.0),
                "size {size}"
            );
        }
        let mut small = magma_cube(1);
        let mut big = magma_cube(4);
        let hit = Damage::of(DamageKind::Cactus);
        let small_before = small.health;
        let big_before = big.health;
        assert!(small.damage(&mut crate::health::TestLevel::new(0).level(), 0.5, hit));
        assert!(big.damage(&mut crate::health::TestLevel::new(0).level(), 0.5, hit));
        assert!(
            big_before - big.health < small_before - small.health,
            "size 4 lost {} and size 1 lost {}",
            big_before - big.health,
            small_before - small.health
        );
    }
}
