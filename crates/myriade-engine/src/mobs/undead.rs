use crate::health::DamageLevel;
use crate::projectiles::{Projectile, ProjectileKind};
use horizon_core::{geometry::Vec3, random::RandomSource};
use horizon_world::move_gate::MoveGate;



/// The target's isInWater, from the water at its feet.
fn in_water_at(w: &super::MobWorld, p: Vec3) -> bool {
    let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    w.fluid(x, y, z) == crate::blocks::Fluid::Water
}
/// `RangedAttackMob.rangedAttackUncertainty`: 14 on peaceful, four less per
/// difficulty step, so a hard drowned throws its trident almost straight.
/// `Drowned` overrides it to return its own field when that is not negative,
/// but nothing in the game sets that field, so the default always applies.
fn ranged_attack_uncertainty(difficulty: u8) -> f32 {
    14.0 - 4.0 * f32::from(difficulty)
}
/// Drowned.DrownedGoToBeachGoal: at night, swim near the surface to a
/// standable block with air above.
#[derive(Clone, Debug)]
struct DrownedGoToBeach(super::goals::MoveToBlock);
fn beach_target(w: &super::MobWorld, p: [i32; 3]) -> bool {
    let empty = |y: i32| {
        let state = w.world.block(p[0], y, p[2]);
        w.facts.is_air(state) && w.facts.get(state).fluid == crate::blocks::Fluid::None
    };
    // BlockState.entityCanStandOn: a full top face.
    empty(p[1] + 1)
        && empty(p[1] + 2)
        && w.facts.get(w.world.block(p[0], p[1], p[2])).sturdy_full & (1 << 1) != 0
}
/// Drowned.DrownedSwimUpGoal: at night, deep in water, head for the surface.
#[derive(Clone, Debug)]
struct DrownedSwimUp {
    stuck: bool,
}
/// RangedBowAttackGoal for skeletons.
#[derive(Clone, Debug)]
pub struct BowAttack {
    speed: f64,
    interval_min: i32,
    radius_sqr: f32,
    attack_time: i32,
    see_time: i32,
    strafe_clockwise: bool,
    strafe_back: bool,
    strafe_time: i32,
}
impl BowAttack {
    pub fn new(speed: f64, interval_min: i32, radius: f32) -> Self {
        Self {
            speed,
            interval_min,
            radius_sqr: radius * radius,
            attack_time: -1,
            see_time: 0,
            strafe_clockwise: false,
            strafe_back: false,
            strafe_time: -1,
        }
    }
}
#[cfg(test)]
mod use_overflow_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}

/// BowItem.getPowerForTime
pub fn bow_power(ticks: i32) -> f32 {
    let t = ticks as f32 / 20.0;
    ((t * t + t * 2.0) / 3.0).min(1.0)
}
/// Drowned.DrownedGoToWaterGoal.
#[derive(Clone, Debug, Default)]
pub struct DrownedGoToWater {
    wanted: [f64; 3],
}

/// `Zombie.ZombieAttackTurtleEggGoal` (a `RemoveBlockGoal` on turtle eggs):
/// walk to a turtle egg within 24 blocks and trample it.
#[derive(Clone, Debug)]
pub struct AttackTurtleEgg {
    base: super::goals::MoveToBlock,
    since_reached: i32,
}
impl Default for AttackTurtleEgg {
    fn default() -> Self {
        Self {
            base: super::goals::MoveToBlock::new(1.0, 24, 3),
            since_reached: 0,
        }
    }
}
/// RemoveBlockGoal.isValidTarget: the block with two air blocks above, in a loaded chunk,
/// that the mob `mob` may destroy (`canEntityDestroy`, no `LivingDestroyBlockEvent` here).
fn turtle_egg_target(w: &super::MobWorld, mob: u32, p: [i32; 3]) -> bool {
    w.world.loaded(p[0], p[2])
        && w.block_name(p[0], p[1], p[2]) == "minecraft:turtle_egg"
        && w.facts.is_air(w.world.block(p[0], p[1] + 1, p[2]))
        && w.facts.is_air(w.world.block(p[0], p[1] + 2, p[2]))
        && w.can_entity_destroy(mob, p, true)
}

#[cfg(test)]
mod conversion_tests {
    use super::*;
    use crate::{items::ItemStack, mobs::world::with_test_world};
    use std::sync::Arc;



    #[test]
    #[ignore = "daily"]
    fn a_drowning_zombie_keeps_its_gear_and_name_tag() {
        let Some(d) = drown("minecraft:zombie", |m| {
            m.custom_name = Some("Bob".into());
            m.name_visible = true;
            m.persistent = true;
            m.extra.set_slot_stack(
                horizon_items::equipment::HEAD,
                Some(ItemStack::new("minecraft:iron_helmet", 1)),
            );
            m.extra
                .set_main_hand_stack(Some(ItemStack::new("minecraft:iron_sword", 1)));
            m.extra.drop_chances = Some(Box::new([2.0; 8]));
            m.extra.pick_up_loot = true;
            m.add_effect("minecraft:speed", 600, 1);
        }) else {
            return;
        };
        assert_eq!(d.species(), Species::Drowned);
        assert_eq!(d.custom_name.as_deref(), Some("Bob"));
        assert!(d.name_visible && d.persistent);
        assert_eq!(d.extra.head_id(), Some("minecraft:iron_helmet"));
        assert_eq!(
            d.extra.slot_id(horizon_items::equipment::HEAD),
            Some("minecraft:iron_helmet")
        );
        assert_eq!(
            d.extra.main_hand_stack().map(|s| &*s.item),
            Some("minecraft:iron_sword")
        );
        assert_eq!(d.extra.drop_chances.as_deref(), Some(&[2.0; 8]));
        assert!(d.extra.pick_up_loot);
        assert!(d.has_effect("minecraft:speed"));
        assert_eq!(d.position, Vec3::new(0.5, 64.0, 0.5));
    }

    #[test]
    #[ignore = "daily"]
    fn a_husk_turns_into_a_zombie_with_its_flags() {
        let Some(z) = drown("minecraft:husk", |m| {
            m.custom_name = Some("Dusty".into());
            m.no_ai = false;
            m.silent = true;
            m.extra.age = i32::MIN / 2;
            m.extra.set_slot_stack(
                horizon_items::equipment::FEET,
                Some(ItemStack::new("minecraft:golden_boots", 1)),
            );
            m.extra.left_handed = true;
        }) else {
            return;
        };
        assert_eq!(z.species(), Species::Zombie);
        assert_eq!(z.custom_name.as_deref(), Some("Dusty"));
        assert!(z.silent && z.is_baby() && z.extra.left_handed);
        assert_eq!(
            z.extra.slot_id(horizon_items::equipment::FEET),
            Some("minecraft:golden_boots")
        );
    }
}

#[cfg(test)]
mod trident_tests {
    use super::ranged_attack_uncertainty;

    /// `RangedAttackMob.rangedAttackUncertainty` is 14 minus four per
    /// difficulty step, so a drowned's trident spread shrinks from 14 on
    /// peaceful to 2 on hard. Horizon used to hard-code the normal value.
    #[test]
    fn the_trident_spread_follows_the_difficulty() {
        assert_eq!(ranged_attack_uncertainty(0), 14.0, "peaceful");
        assert_eq!(ranged_attack_uncertainty(1), 10.0, "easy");
        assert_eq!(ranged_attack_uncertainty(2), 6.0, "normal");
        assert_eq!(ranged_attack_uncertainty(3), 2.0, "hard");
    }
}

#[cfg(test)]
mod creative_target_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use std::sync::Arc;

}

#[cfg(test)]
mod go_to_water_tests {
    use super::*;
    use crate::mobs::world::with_built_test_world;
    use std::sync::Arc;

}

#[cfg(test)]
mod turtle_egg_tests {
    use super::*;
    use crate::mobs::world::with_built_test_world;
    use std::sync::Arc;


    /// `Zombie.ZombieAttackTurtleEggGoal.playBreakSound`:
    /// `0.9F + level.getRandom().nextFloat() * 0.2F`. One draw of the level's
    /// random, not the mob's, so the pitch varies with the level seed over
    /// 0.9-1.1 rather than a fixed 1.0.
    #[test]
    #[ignore = "daily"]
    fn the_egg_break_pitch_draws_from_the_level_random() {
        // java.util.Random(seed)'s first nextFloat, from an independent
        // implementation: 0.7306990027427673 for 7, 0.0978928804397583 for
        // 4096, so the pitches are 0.9 + f * 0.2 in f32.
        let Some((pitch, next)) = break_tick(7) else {
            return;
        };
        assert_eq!(pitch, 1.046_139_7);
        // Exactly one draw: the seed's second float is what comes next.
        assert_eq!(next, 0.638_537_65);
        assert_eq!(break_tick(4096), Some((0.919_578_55, 0.875_477_85)));
    }
}

#[cfg(test)]
mod wither_arrow_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use std::sync::Arc;

    fn arrow() -> Projectile {
        Projectile::new(
            ProjectileKind::Arrow {
                base_damage: 2.0,
                critical: false,
                effect: None,
            },
            Vec3::ZERO,
            None,
        )
    }

    /// `WitherSkeleton.getArrow`: `super.getArrow(...)` then
    /// `igniteForSeconds(100)` on the arrow it returns — 2000 fire ticks —
    /// where a plain skeleton's arrow is not burning at all.
    #[test]
    fn only_a_wither_skeletons_arrow_comes_out_burning() {
        for (species, fire_ticks) in [(Species::WitherSkeleton, 2000), (Species::Skeleton, 0)] {
            let m = mob(species);
            let mut p = arrow();
            ignite_for_wither_skeleton(&m, &mut p);
            let a = p.arrow.as_ref().expect("an arrow");
            assert_eq!(a.fire_ticks, fire_ticks, "{species:?}");
            assert_eq!(a.on_fire(), fire_ticks > 0, "{species:?}");
        }
        // `igniteForTicks` keeps the longer burn: a Flame arrow already alight
        // for 4000 ticks is not shortened to the wither skeleton's 2000.
        let m = mob(Species::WitherSkeleton);
        let mut p = arrow();
        if let Some(a) = p.arrow.as_mut() {
            a.ignite_for_seconds(200.0);
        }
        ignite_for_wither_skeleton(&m, &mut p);
        assert_eq!(p.arrow.as_ref().unwrap().fire_ticks, 4000);
    }


}

#[cfg(test)]
mod trident_continue_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use std::sync::Arc;


}

#[cfg(test)]
mod turtle_egg_target_tests {
    use super::*;
    use std::sync::Arc;

    /// Refuses mob 1 the block at (2, 64, 0).
    struct Keep;
    impl crate::mobs::MobHooks for Keep {
        fn enderman_anger_suppressed(&self, _: u32, _: &crate::mobs::PlayerView) -> bool {
            false
        }
        fn can_grief(&self, _: u32, rule: bool) -> bool {
            rule
        }
        fn grief_hooked(&self) -> bool {
            false
        }
        fn living_visibility(&self, _: &crate::mobs::PlayerView, _: u32, v: f64) -> f64 {
            v
        }
        fn can_entity_destroy(&self, mob: u32, pos: [i32; 3], vanilla: bool) -> bool {
            vanilla && !(mob == 1 && pos == [2, 64, 0])
        }
    }

    /// `RemoveBlockGoal.isValidTarget` asks the egg's `canEntityDestroy` for
    /// the zombie: refused, the egg at (2, 64, 0) is no target; the one at
    /// (4, 64, 0), and the same egg for another zombie, still are.
    #[test]
    #[ignore = "daily"]
    fn a_refused_turtle_egg_is_no_target() {
        let build = |world: &mut crate::world::World,
                     registry: &horizon_world::registry::BlockRegistry| {
            world
                .chunks
                .insert((0, 0), Arc::new(crate::chunk::Chunk::empty(0, 0)));
            let egg = registry.default_state("minecraft:turtle_egg").unwrap();
            world.set(2, 64, 0, egg).unwrap();
            world.set(4, 64, 0, egg).unwrap();
        };
        assert!(crate::mobs::world::with_built_test_world(
            &[],
            build,
            |world, _| {
                let mut world = world.clone();
                assert!(turtle_egg_target(&world, 1, [2, 64, 0]));
                world.mob_hooks = Some(&Keep);
                assert!(!turtle_egg_target(&world, 1, [2, 64, 0]));
                assert!(turtle_egg_target(&world, 2, [2, 64, 0]));
                assert!(turtle_egg_target(&world, 1, [4, 64, 0]));
            }
        ));
    }
}
