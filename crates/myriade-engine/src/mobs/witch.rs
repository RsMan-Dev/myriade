use crate::projectiles::{Projectile, ProjectileKind};
use horizon_core::geometry::trig::Mth;
use horizon_core::{geometry::Vec3, random::RandomSource};
use horizon_world::native_load_context::LoopContext;


/// `WitchRenderer.extractRenderState`'s `isHoldingPotion`: the main hand holds
/// a potion, either a drunk one (kept by the potion's name, `drink`) or a
/// plain potion item.
pub fn holds_potion(item: &str) -> bool {
    matches!(
        item,
        "minecraft:potion"
            | "minecraft:water_breathing"
            | "minecraft:fire_resistance"
            | "minecraft:healing"
            | "minecraft:swiftness"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Witch.registerGoals` passes `Raider.class` to `HurtByTargetGoal`, so a
    /// raider that hits a witch is not taken as its target. The list is the
    /// `Raider` class and nothing else: a vex is not a raider.
    #[test]
    fn a_witch_ignores_the_raider_that_hit_it() {
        let goal = hurt_by();
        assert_eq!(
            goal.ignore,
            vec![
                Species::Pillager,
                Species::Vindicator,
                Species::Evoker,
                Species::Illusioner,
                Species::Witch,
                Species::Ravager,
            ]
        );
        assert!(!goal.ignore.contains(&Species::Vex));
        assert!(!goal.ignore.contains(&Species::Zombie));
    }


    /// How many drink sounds forty attempts produce. Vanilla plays
    /// `entity.witch.drink` at volume 1, and skips it when the witch is silent.
    fn drink_sounds(silent: bool) -> usize {
        let mut m = witch();
        m.silent = silent;
        m.random.set_seed(1);
        m.fluids.eye_in_water = true;
        m.fire_ticks = 20;
        m.health = 1.0;
        m.ai.target = Some(Who::Player);
        let mut events = Vec::new();
        for _ in 0..40 {
            m.extra.flag = false;
            drink(
                &mut m,
                crate::test_resources::server_loop_context(),
                &mut events,
                true,
            );
        }
        events
            .iter()
            .filter(|e| {
                matches!(e, MobEvent::Sound(s, _, volume, _) if *s == "entity.witch.drink" && *volume == 1.0)
            })
            .count()
    }

    /// `Witch.aiStep` and `performRangedAttack` play at volume 1 only when
    /// the witch is not silent. The pitch draw stays inside that check.
    #[test]
    fn a_silent_witch_does_not_play_its_drink_or_throw_sound() {
        use horizon_core::random::LegacyRandom;
        assert!(drink_sounds(false) > 0);
        assert_eq!(drink_sounds(true), 0);

        let mut loud = witch();
        loud.random.set_seed(11);
        let mut quiet = witch();
        quiet.silent = true;
        quiet.random.set_seed(11);
        let mut oracle = LegacyRandom::new(11);
        // Vanilla: 0.8F + nextFloat() * 0.4F, volume 1.
        let pitch = 0.8 + oracle.next_float() * 0.4;
        let mut events = Vec::new();
        witch_sound(&mut loud, &mut events, "entity.witch.throw");
        match &events[0] {
            MobEvent::Sound(name, _, volume, got) => {
                assert_eq!(*name, "entity.witch.throw");
                assert_eq!(*volume, 1.0);
                assert!((got - pitch).abs() < 1.0e-5);
            }
            other => panic!("throw sound, got {other:?}"),
        }
        events.clear();
        witch_sound(&mut quiet, &mut events, "entity.witch.throw");
        assert!(events.is_empty());
        let mut untouched = LegacyRandom::new(11);
        assert_eq!(quiet.random.next_float(), untouched.next_float());
    }

    /// The hoisted list is exactly the set `is_raider` answers yes for.
    #[test]
    fn the_raider_list_is_the_raider_class() {
        for def in crate::mobs::species::ALL {
            assert_eq!(
                crate::mobs::raider::is_raider(def.species),
                super::super::raider::RAIDERS.contains(&def.species),
                "{:?}",
                def.species
            );
        }
    }
}
