use horizon_core::geometry::trig::Mth;
use horizon_core::{
    geometry::{Aabb, Axis, Vec3},
    random::RandomSource,
};
use std::f32::consts::PI;

/// Wolf variants: registry name and texture file (`WolfVariants`).
pub const WOLF_VARIANTS: [(&str, &str); 9] = [
    ("pale", "wolf"),
    ("spotted", "wolf_spotted"),
    ("snowy", "wolf_snowy"),
    ("black", "wolf_black"),
    ("ashen", "wolf_ashen"),
    ("rusty", "wolf_rusty"),
    ("woods", "wolf_woods"),
    ("chestnut", "wolf_chestnut"),
    ("striped", "wolf_striped"),
];
/// The wolf variant a biome spawns (its high-priority `BiomeCheck`).
pub fn wolf_variant_for(biome: &str, has_tag: &dyn Fn(&str) -> bool) -> u8 {
    if has_tag("minecraft:is_savanna") {
        1
    } else if biome == "minecraft:grove" {
        2
    } else if biome == "minecraft:old_growth_pine_taiga" {
        3
    } else if biome == "minecraft:snowy_taiga" {
        4
    } else if has_tag("minecraft:is_jungle") {
        5
    } else if biome == "minecraft:forest" {
        6
    } else if biome == "minecraft:old_growth_spruce_taiga" {
        7
    } else if has_tag("minecraft:is_badlands") {
        8
    } else {
        0
    }
}
/// The default collar: red.
pub const RED: u8 = 14;

// AnimState slots: begging interest and the shake animation.
pub const INTEREST: usize = 0;
pub const SHAKE: usize = 1;

/// `LandOnOwnersShoulderGoal`: a standing parrot, 100 ticks old, lands on
/// the shoulder of the owner it touches; the host does the landing.
#[derive(Clone, Debug)]
struct LandOnOwnersShoulder;
/// Whether `uuid` is the saved owner. A host with no UUID yet owns what it
/// tamed under the placeholder `[0, 0, 0, 1]`.
pub fn owned_by(owner: Option<[i32; 4]>, uuid: [i32; 4]) -> bool {
    owner.is_some_and(|o| o == uuid || (o == [0, 0, 0, 1] && uuid == [0; 4]))
}

/// `SitWhenOrderedToGoal`.
#[derive(Clone, Debug)]
struct SitWhenOrdered;
/// `FollowOwnerGoal(speed, startDistance, stopDistance)`.
#[derive(Clone, Debug)]
struct FollowOwner {
    speed: f64,
    start: f64,
    stop: f64,
    recalc: i32,
}
impl FollowOwner {
    fn new(speed: f64, start: f64, stop: f64) -> Self {
        Self {
            speed,
            start,
            stop,
            recalc: 0,
        }
    }
}
/// `OwnerHurtByTargetGoal`: attack what hurt the owner.
#[derive(Clone, Debug)]
struct OwnerHurtBy {
    timestamp: i64,
}
/// `OwnerHurtTargetGoal`: attack what the owner attacks.
#[derive(Clone, Debug)]
struct OwnerHurt {
    timestamp: i64,
}
/// `BegGoal`: tilt the head at a player holding something interesting.
#[derive(Clone, Debug)]
struct Beg {
    food: std::sync::Arc<[String]>,
    ticks: i32,
}
impl Beg {
}

/// `WolfRenderState.getBodyRollAngle`.
fn body_roll(shake: f32, offset: f32) -> f32 {
    let p = ((shake + offset) / 1.8).clamp(0.0, 1.0);
    (p * PI).msin() * (p * PI * 11.0).msin() * 0.15 * PI
}

/// Cat variants (`CatVariants`): registry name, texture file.
pub const CAT_VARIANTS: [&str; 11] = [
    "tabby",
    "black",
    "red",
    "siamese",
    "british_shorthair",
    "calico",
    "persian",
    "ragdoll",
    "white",
    "jellie",
    "all_black",
];
// Cat AnimState slots.
const LIE: usize = 0;
const LIE_TAIL: usize = 1;
const RELAX: usize = 2;

/// `Ocelot.OcelotTemptGoal(ocelot, 0.6, ocelot food, true)`.
#[derive(Clone, Debug)]
struct OcelotTempt(super::goals::Tempt);
impl OcelotTempt {
}
/// `Cat.CatTemptGoal`: only wild cats come for fish. The second field is
/// `selectedPlayer`, the player the cat has picked to approach unscared.
#[derive(Clone, Debug)]
struct CatTempt(super::goals::Tempt, Option<[i32; 4]>);
/// `Cat.CatRelaxOnOwnerGoal`: while the owner sleeps, the cat lies down beside
/// them, and may leave a morning gift when they wake up.
#[derive(Clone, Debug, Default)]
struct CatRelaxOnOwner {
    goal: Option<[i32; 3]>,
    on_bed_ticks: i32,
}
impl CatRelaxOnOwner {
}
/// `CatLieOnBlockGoal`: tame cats lie on beds and other warm blocks.
#[derive(Clone, Debug)]
struct CatLieOnBlock(super::goals::MoveToBlock);
/// `CatSitOnBlockGoal`: unopened chests, lit furnaces and the foot of beds.
#[derive(Clone, Debug)]
struct CatSitOnBlock(super::goals::MoveToBlock);
/// `OcelotAttackGoal`: stalk slowly, dash in, swipe every second.
#[derive(Clone, Debug)]
struct OcelotAttack {
    attack_time: i32,
}
/// `Cat.finalizeSpawn` variant: any but all-black, which a full moon allows.
pub fn cat_variant(w_moon_full: bool, r: &mut impl RandomSource) -> u8 {
    let n = if w_moon_full { 11 } else { 10 };
    r.next_int_bound(n).unwrap_or(0) as u8
}
/// `Mth.rotLerp`: degrees-wrapping, applied by the feline models to radians.
fn rot_lerp(t: f32, from: f32, to: f32) -> f32 {
    from + t * super::living::wrap(to - from)
}
/// `FelineRenderState` values the feline models read.
#[derive(Clone, Copy, Debug, Default)]
pub struct Feline {
    pub crouching: bool,
    pub sprinting: bool,
    pub sitting: bool,
    pub lie_down: f32,
    pub lie_down_tail: f32,
    pub relax_one: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::{Damage, sensor_scan_tests::kind};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;



    /// `TamableAnimal.unableToMoveToOwner` (#12939): a pet on a lead, with lead
    /// data still to be found, or riding something neither follows nor
    /// teleports to its owner.
    #[test]
    #[ignore = "daily"]
    fn a_leashed_or_riding_pet_stays_put() {
        let ran = crate::mobs::world::with_test_world(&[], |w, _| {
            let mut m = wolf();
            set_tame(&mut m, true);
            assert!(!unable_to_move_to_owner(&m, w), "a free pet follows");
            m.extra.leash = Some(super::super::leash::Holder::Knot([0, 64, 3]));
            assert!(unable_to_move_to_owner(&m, w), "tied to a fence");
            m.extra.leash = None;
            m.extra.passenger = true;
            assert!(unable_to_move_to_owner(&m, w), "in a boat");
            m.extra.passenger = false;
            m.extra.sitting = true;
            assert!(unable_to_move_to_owner(&m, w), "ordered to sit");
        });
        if !ran {
            eprintln!("skipped: no extracted vanilla data");
        }
    }
    /// `SitWhenOrderedToGoal.start` only stops and sits: a wolf begging for the
    /// bone its owner holds keeps its head tilt (`BegGoal` owns it).
    #[test]
    #[ignore = "daily"]
    fn sitting_down_keeps_a_wolfs_begging_tilt() {
        let ran = crate::mobs::world::with_test_world(&[], |w, _| {
            let mut fixture = crate::health::TestLevel::new(0);
            let mut events = Vec::new();
            let mut cx = Cx {
                w,
                level: &mut fixture.level(),
                events: &mut events,
            };
            let mut m = wolf();
            set_tame(&mut m, true);
            m.extra.flag = true;
            m.extra.sitting = true;
            SitWhenOrdered.start(&mut m, &mut cx);
            assert!(m.extra.flag, "the begging tilt is BegGoal's to end");
        });
        if !ran {
            eprintln!("skipped: no extracted vanilla data");
        }
    }

    const WHITE: u32 = 0xFFFF_FFFF;

    #[test]
    fn a_cat_lying_near_a_sleeper_lies_on_top_of_them() {
        use horizon_core::geometry::Aabb;
        let sleeper = Aabb::new(Vec3::new(2.4, 64.0, 0.4), Vec3::new(2.6, 64.2, 0.6));
        let far = Aabb::new(Vec3::new(5.4, 64.0, 0.4), Vec3::new(5.6, 64.2, 0.6));
        let mut m = cat();
        m.anim.v[LIE] = 1.0;
        m.extra.flag = true;
        cat_lying_on_sleeper(&mut m, &[far]);
        assert!(!m.extra.on_sleeper);
        cat_lying_on_sleeper(&mut m, &[far, sleeper]);
        assert!(m.extra.on_sleeper);
        let on_top = cat_rotations(&m, 1.0);
        // The extra `translate(0.15 * lie, 0, 0)` after the roll.
        m.extra.on_sleeper = false;
        let plain = cat_rotations(&m, 1.0);
        assert_ne!(on_top, plain);
        assert!((on_top.w_axis.y - plain.w_axis.y - 0.15).abs() < 1.0e-4);
        // Not lying: nothing to lie on.
        m.extra.flag = false;
        cat_lying_on_sleeper(&mut m, &[sleeper]);
        assert!(!m.extra.on_sleeper);
    }
    /// `Wolf.die`: the wet flag, the shake counter and both shake animations
    /// are cleared before any loot is rolled. The death runs
    /// `drops::death_state` first, so the reset has to be asked there, not
    /// only exist.
    #[test]
    fn a_wolf_that_dies_mid_shake_is_dry_and_still() {
        let mut m = wolf();
        // Soaked, and shaking it off: the tick sets the wet flag and entity
        // event 8 starts the shake, which counts up 0.05 a tick.
        m.extra.flag2 = true;
        entity_event(&mut m, 8);
        m.anim.v[SHAKE] = 0.9;
        m.anim.vo[SHAKE] = 0.85;
        crate::mobs::drops::death_state(&mut m);
        assert!(!m.extra.flag2, "a dead wolf is not wet");
        assert_eq!(m.extra.counter2, 0, "a dead wolf is not shaking");
        assert_eq!(m.anim.v[SHAKE], 0.0, "shakeAnim");
        assert_eq!(m.anim.vo[SHAKE], 0.0, "shakeAnimO");
    }
    fn entity_events(events: &[MobEvent]) -> Vec<i8> {
        events
            .iter()
            .filter_map(|e| match e {
                MobEvent::EntityEvent { event, .. } => Some(*event),
                _ => None,
            })
            .collect()
    }

    /// A wolf of a server (the default single player's server thread, a LAN
    /// guest): `Wolf.tick` makes it wet in water on the client too, which
    /// `getWetShade` darkens; entity event 8 starts the shake, which the
    /// client runs to the end, splashing, and then the wolf is dry (#2885).
    #[test]
    fn a_wolf_of_a_server_darkens_in_water_and_shakes_dry_on_its_event() {
        let mut m = wolf();
        let mut events = Vec::new();
        m.fluids.in_water = true;
        client_tick(&mut m, false, &mut events);
        // 0.75 of the colour: 191.
        assert_eq!(tint(&m, 1.0), 0xFFBF_BFBF, "wet in water");
        m.fluids.in_water = false;
        client_tick(&mut m, false, &mut events);
        assert_eq!(tint(&m, 1.0), 0xFFBF_BFBF, "still wet before it shakes");
        assert_eq!(m.anim.v[SHAKE], 0.0);
        // Rain wets it as well.
        let mut dry = wolf();
        client_tick(&mut dry, true, &mut events);
        assert_ne!(tint(&dry, 1.0), WHITE, "wet in the rain");

        crate::mobs::sync::entity_event(&mut m, 8);
        let mut ticks = 0;
        while m.extra.flag2 && ticks < 100 {
            client_tick(&mut m, false, &mut events);
            ticks += 1;
            if ticks == 1 {
                assert!((m.anim.v[SHAKE] - 0.05).abs() < 1e-6);
                assert_eq!(m.anim.vo[SHAKE], 0.0);
            }
        }
        // shakeAnimO reaches 2 on the 41st tick.
        assert!((41..=42).contains(&ticks), "shook for {ticks} ticks");
        assert_eq!(tint(&m, 1.0), WHITE, "dry after the shake");
        assert_eq!((m.extra.counter2, m.anim.v[SHAKE]), (0, 0.0));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MobEvent::ParticleAt { kind: "splash", .. })),
            "the client draws the splashes"
        );
        assert!(
            !events.iter().any(|e| matches!(e, MobEvent::Sound(..))),
            "the shake sound is the server's"
        );
    }

    /// `Wolf.handleEntityEvent(56)`: back in the water, the server cancels
    /// the shake; the wolf stays wet. A client does not cancel on its own.
    #[test]
    fn a_wolf_of_a_server_stops_shaking_on_its_cancel_event() {
        let mut m = wolf();
        let mut events = Vec::new();
        m.fluids.in_water = true;
        client_tick(&mut m, false, &mut events);
        m.fluids.in_water = false;
        crate::mobs::sync::entity_event(&mut m, 8);
        for _ in 0..10 {
            client_tick(&mut m, false, &mut events);
        }
        assert!(m.anim.v[SHAKE] > 0.4);
        m.fluids.in_water = true;
        client_tick(&mut m, false, &mut events);
        assert!(m.extra.counter2 > 0, "only the server cancels");
        crate::mobs::sync::entity_event(&mut m, 56);
        assert_eq!((m.extra.counter2, m.anim.v[SHAKE]), (0, 0.0));
        assert_ne!(tint(&m, 1.0), WHITE, "still wet");
    }

    /// `Wolf.aiStep` broadcasts entity event 8 when a wet wolf standing on
    /// the ground starts shaking, and `Wolf.tick` event 56 when water or rain
    /// cancels it: the clients' only way to know.
    #[test]
    fn a_server_tells_the_clients_when_a_wolf_shakes() {
        let mut m = wolf();
        let mut events = Vec::new();
        server_shake(&mut m, true, &mut events);
        assert!(m.extra.flag2);
        // Out of the water: it starts and makes its sound in the same tick.
        server_shake(&mut m, false, &mut events);
        assert_eq!(entity_events(&events), [8]);
        assert!(m.extra.counter2 > 0);
        assert!((m.anim.v[SHAKE] - 0.05).abs() < 1e-6);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MobEvent::Sound("entity.wolf.shake", ..)))
        );
        server_shake(&mut m, true, &mut events);
        assert_eq!(entity_events(&events), [8, 56]);
        assert_eq!((m.extra.counter2, m.anim.v[SHAKE]), (0, 0.0));
        assert!(m.extra.flag2, "still wet");
    }


    #[test]
    fn a_hit_stands_a_sitting_wolf_or_parrot_up() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // Wolf.hurtServer and Parrot.hurtServer call setOrderedToSit(false)
        // before LivingEntity.hurtServer.
        for (species, name) in [
            (Species::Wolf, "minecraft:wolf"),
            (Species::Parrot, "minecraft:parrot"),
        ] {
            let mut m = sitting(species, name);
            assert!(m.damage(level, 1.0, Damage::player([0.0, 64.0, 1.0])));
            assert!(!m.extra.sitting, "{name} still sits after a hit");
            // A weaker hit inside the hurt frames deals nothing, but it
            // reached hurtServer, so it still stands the mob up.
            m.extra.sitting = true;
            assert!(!m.damage(level, 0.5, Damage::player([0.0, 64.0, 1.0])));
            assert!(!m.extra.sitting, "{name} still sits after a blocked hit");
        }
    }

    #[test]
    fn a_hit_leaves_a_sitting_cat_sitting() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // Cat has no hurtServer override: a hit does not stand it up.
        let mut m = sitting(Species::Cat, "minecraft:cat");
        assert!(m.damage(level, 1.0, Damage::player([0.0, 64.0, 1.0])));
        assert!(m.extra.sitting);
    }

    /// `Cat.canMate` and `Wolf.canMate`: the seeker only has to be tamed, so a
    /// mob ordered to sit still goes looking for a mate. Only a wolf's
    /// *partner* is refused while it sits (`wolf.isInSittingPose()`), which is
    /// the one sitting check vanilla makes.
    #[test]
    #[ignore = "daily"]
    fn a_sitting_tame_mob_still_looks_for_a_mate() {
        use crate::mobs::goals::{Breed, Cx, Goal};
        use crate::mobs::world::with_test_world;
        let named = |species, name: &str, id: u32, sat: bool| {
            let mut m = sitting(species, name);
            m.id = id;
            m.extra.in_love = 600;
            // Ordered to sit on the ground, so also in the pose the partner
            // check reads.
            m.extra.sitting = sat;
            m.extra.sitting_pose = sat;
            m
        };
        for (species, name, seeker_sat, partner_sat, breeds) in [
            // A tamed cat pairs with a tamed cat, sitting or not.
            (Species::Cat, "minecraft:cat", true, true, true),
            (Species::Cat, "minecraft:cat", true, false, true),
            // A wolf pairs unless the wolf it would take is sitting.
            (Species::Wolf, "minecraft:wolf", true, false, true),
            (Species::Wolf, "minecraft:wolf", true, true, false),
            (Species::Wolf, "minecraft:wolf", false, true, false),
        ] {
            let mut seeker = named(species, name, 1, seeker_sat);
            let partner = named(species, name, 2, partner_sat);
            let seen = [partner.seen(None, &crate::items::Items::empty())];
            assert!(
                with_test_world(&seen, |w, _| {
                    let mut events = Vec::new();
                    let mut test_level = crate::health::TestLevel::new(0);
                    let level = &mut test_level.level();
                    let mut cx = Cx {
                        w,
                        level,
                        events: &mut events,
                    };
                    let mut goal = Breed::new(1.0);
                    assert_eq!(
                        goal.can_use(&mut seeker, &mut cx),
                        breeds,
                        "{name}: seeker sits {seeker_sat}, partner sits {partner_sat}"
                    );
                }),
                "vanilla data is missing"
            );
        }
    }

}

/// `NeutralMob.isAngryAtAllPlayers`: with the `universal_anger` game rule on, a
/// wolf still angry with no target left goes for any player; with the rule off
/// it holds none, and a creative player is never one (`isValidPlayerTarget`).
#[cfg(test)]
mod universal_anger_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use std::sync::Arc;

}

#[cfg(test)]
mod cat_tempt_tests {
    use super::*;
    use crate::mobs::world::{test_player, with_test_world};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}

#[cfg(test)]
mod tamable_panic_tests {
    use super::*;
    use crate::mobs::world::with_built_test_world;
    use crate::mobs::{Damage, DamageKind};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;


    /// `TamableAnimal.TamableAnimalPanicGoal` is built with a damage tag: a
    /// cat's and a parrot's take the default `#panic_causes` (any damage that
    /// makes a mob panic), a wolf's `#panic_environmental_causes` (fire,
    /// freezing, cactus, lightning, lava...).
    #[test]
    #[ignore = "daily"]
    fn tamable_animals_panic_from_the_damage_they_fear() {
        // Needs the extracted mob data (`with_built_test_world`); CI has none.
        let (Some(cat), Some(frozen), Some(hit)) = (
            panics("minecraft:cat", register_cat, DamageKind::PlayerAttack),
            panics("minecraft:wolf", register, DamageKind::Freeze),
            panics("minecraft:wolf", register, DamageKind::PlayerAttack),
        ) else {
            eprintln!("skipped: no extracted mob data");
            return;
        };
        assert!(cat, "a cat hit by a player runs");
        assert!(frozen, "a freezing wolf runs");
        assert!(!hit, "a wolf hit by a player does not panic");
    }
}

#[cfg(test)]
mod non_tame_tests {
    use super::*;
    use crate::mobs::world::with_test_world;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

}


#[cfg(test)]
mod goal_timing_tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use std::sync::Arc;

    /// `FollowOwnerGoal` has no `requiresUpdateEveryTick`, so its `tick` runs on
    /// the full selector updates only and `adjustedTickDelay(10)` is
    /// `reducedTickDelay(10)` of them.
    #[test]
    fn a_pet_recalculates_its_path_on_full_updates_only() {
        let goal = FollowOwner::new(1.0, 10.0, 2.0);
        assert!(
            !goal.every_tick(),
            "FollowOwnerGoal has no requiresUpdateEveryTick override"
        );
    }

}

