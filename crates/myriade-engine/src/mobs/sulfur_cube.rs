use crate::health::DamageLevel;
use horizon_core::{geometry::Vec3, random::RandomSource};
use horizon_world::attributes::{Modifier, Operation};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

// Sulfur cube state (on top of `cube`'s `size`, `counter` jump delay,
// `flag` aggressive, `flag2` was on ground, `vector[0]` direction):
// `head` the swallowed block, `counter2` the fuse plus one (0 unprimed),
// `ints[3]` the pickup timer, `scratch[1]` the sounds pending (1 a fuse just
// lit, 2 a hit),
// `scratch[2]` the push sound cooldown, `scratch[3]` from a bucket,
// `point` the item it is after.
pub const PICKUP_TIMER: i32 = 100;

/// One `SulfurCubeArchetype` (the parts Horizon's physics can use).
#[derive(Clone, Copy, Debug)]
pub struct Archetype {
    pub name: &'static str,
    pub buoyant: bool,
    /// `explosion`: fuse ticks, power, causes fire.
    pub explosion: Option<(i32, f32, bool)>,
    /// `contact_damage` amount (`sulfur_cube_hot`).
    pub contact_damage: Option<f32>,
    /// The knockback resistance modifier (the attribute range is -2..1; negative values add knockback).
    pub knockback_resistance: f64,
    /// The explosion knockback resistance modifier (its own entry in the
    /// archetype's `attribute_modifiers`).
    pub explosion_knockback_resistance: f32,
    /// `Attributes.BOUNCINESS` (the modifier added to a base of 0).
    pub bounciness: f32,
    /// `Attributes.FRICTION_MODIFIER` and `AIR_DRAG_MODIFIER` (base 1 times
    /// the archetype's factor).
    pub friction_modifier: f32,
    pub air_drag_modifier: f32,
    pub horizontal_power: f32,
    pub vertical_power: f32,
    pub hit_sound: &'static str,
    pub push_sound: &'static str,
    pub push_cooldown: f32,
    pub push_threshold: f32,
}
macro_rules! arch {
    ($n:literal, $b:expr, $e:expr, $c:expr, $k:expr, $ek:expr, $phys:expr, $h:expr, $v:expr, $cd:expr, $t:expr) => {
        Archetype {
            name: $n,
            buoyant: $b,
            explosion: $e,
            contact_damage: $c,
            knockback_resistance: $k,
            explosion_knockback_resistance: $ek,
            bounciness: $phys.0,
            friction_modifier: $phys.1,
            air_drag_modifier: $phys.2,
            horizontal_power: $h,
            vertical_power: $v,
            hit_sound: concat!("entity.sulfur_cube.", $n, ".hit"),
            push_sound: concat!("entity.sulfur_cube.", $n, ".push"),
            push_cooldown: $cd,
            push_threshold: $t,
        }
    };
}
/// `data/minecraft/sulfur_cube_archetype/*.json`.
pub const ARCHETYPES: [Archetype; 12] = [
    arch!(
        "bouncy",
        true,
        None,
        None,
        -2.0,
        -2.0,
        (0.9, 0.3, 0.01),
        0.4125,
        0.105,
        0.7,
        0.3
    ),
    arch!(
        "explosive",
        true,
        Some((120, 3.0, false)),
        None,
        -1.0,
        -1.0,
        (0.5, 0.3, 0.3),
        0.4125,
        0.09,
        0.7,
        0.1
    ),
    arch!(
        "fast_flat",
        false,
        None,
        None,
        -1.0,
        -1.0,
        (0.5, 0.2, 0.01),
        0.9125,
        0.09,
        0.9,
        0.03
    ),
    arch!(
        "fast_sliding",
        false,
        None,
        None,
        0.5,
        0.5,
        (0.1, 0.05, 0.01),
        0.6625,
        0.09,
        1.0,
        0.05
    ),
    arch!(
        "high_resistance",
        false,
        None,
        None,
        0.7,
        0.7,
        (0.2, 1.0, 0.01),
        0.4125,
        0.09,
        0.7,
        0.03
    ),
    arch!(
        "hot",
        true,
        None,
        Some(1.0),
        -1.0,
        -1.0,
        (0.5, 0.3, 0.1),
        0.4125,
        0.09,
        0.7,
        0.2
    ),
    arch!(
        "light",
        true,
        None,
        None,
        -1.0,
        -1.0,
        (1.0, 0.3, 1.8),
        0.4125,
        0.18,
        0.7,
        0.2
    ),
    arch!(
        "regular",
        true,
        None,
        None,
        -1.0,
        -1.0,
        (0.5, 0.3, 0.1),
        0.4125,
        0.09,
        0.5,
        0.2
    ),
    arch!(
        "slow_bouncy",
        false,
        None,
        None,
        0.4,
        0.4,
        (0.6, 0.3, 0.05),
        0.4125,
        0.24,
        0.5,
        0.05
    ),
    arch!(
        "slow_flat",
        false,
        None,
        None,
        0.5,
        0.5,
        (0.4, 0.4, 0.1),
        0.4125,
        0.105,
        0.9,
        0.03
    ),
    arch!(
        "slow_sliding",
        false,
        None,
        None,
        0.8,
        0.8,
        (0.1, 0.05, 0.01),
        0.4125,
        0.09,
        1.0,
        0.02
    ),
    arch!(
        "sticky",
        false,
        None,
        None,
        -2.0,
        -2.0,
        (0.0, 2.0, 0.01),
        0.4125,
        0.09,
        0.5,
        0.05
    ),
];
/// `SulfurCubeArchetype.DEFAULT_*`: no block swallowed.
const DEFAULT_PUSH: (&str, f32, f32) = ("entity.sulfur_cube.regular.push", 0.5, 0.2);
static ITEMS: OnceLock<Vec<HashSet<String>>> = OnceLock::new();
/// The items of each archetype's `#sulfur_cube_archetype/<name>` tag.
pub fn load_archetype_items(tags: &HashMap<String, HashSet<String>>) {
    let _ = ITEMS.set(
        ARCHETYPES
            .iter()
            .map(|a| {
                tags.get(&format!("minecraft:sulfur_cube_archetype/{}", a.name))
                    .cloned()
                    .unwrap_or_default()
            })
            .collect(),
    );
}
// These unit fixtures share a process-wide item lookup. Whichever initializes
// it first must include both the explosive and hot bodies exercised below.
#[cfg(test)]
fn test_archetype_items() {
    load_archetype_items(&HashMap::from([
        (
            "minecraft:sulfur_cube_archetype/explosive".into(),
            HashSet::from(["minecraft:tnt".into()]),
        ),
        (
            "minecraft:sulfur_cube_archetype/hot".into(),
            HashSet::from(["minecraft:magma_block".into()]),
        ),
    ]));
}
/// `matchingArchetypes`: the last one wins (as `collectEquipmentChanges`
/// leaves it), buoyancy if any.
pub fn archetype_of(item: &str) -> Option<&'static Archetype> {
    let items = ITEMS.get()?;
    ARCHETYPES
        .iter()
        .zip(items)
        .rev()
        .find(|(_, set)| set.contains(item))
        .map(|(a, _)| a)
}
/// `matchingArchetypes`: every archetype whose tag holds the item, in
/// registry order.
fn matching(item: &str) -> impl Iterator<Item = &'static Archetype> + '_ {
    ITEMS
        .get()
        .into_iter()
        .flat_map(move |items| {
            ARCHETYPES
                .iter()
                .zip(items)
                .filter(move |(_, set)| set.contains(item))
        })
        .map(|(a, _)| a)
}
/// `SulfurCubeArchetype.attributeModifiers`: attribute, modifier id, amount
/// (the table's floats as the JSON's doubles) and operation.
pub fn attribute_modifiers(a: &Archetype) -> [(&'static str, String, f64, Operation); 5] {
    let id = |op: &str, attr: &str| format!("minecraft:{}_{op}_{attr}", a.name);
    [
        (
            "minecraft:knockback_resistance",
            id("add", "knockback_resistance"),
            // The data's float, widened (the physics table keeps a decimal double).
            f64::from(a.knockback_resistance as f32),
            Operation::AddValue,
        ),
        (
            "minecraft:explosion_knockback_resistance",
            id("add", "explosion_knockback_resistance"),
            f64::from(a.explosion_knockback_resistance),
            Operation::AddValue,
        ),
        (
            "minecraft:bounciness",
            id("add", "bounciness"),
            f64::from(a.bounciness),
            Operation::AddValue,
        ),
        (
            "minecraft:friction_modifier",
            id("mul", "friction_modifier"),
            f64::from(a.friction_modifier) - 1.0,
            Operation::AddMultipliedTotal,
        ),
        (
            "minecraft:air_drag_modifier",
            id("mul", "air_drag_modifier"),
            f64::from(a.air_drag_modifier) - 1.0,
            Operation::AddMultipliedTotal,
        ),
    ]
}
/// The attributes `attribute_modifiers` touch, routed to the cube's own map.
pub const ARCHETYPE_ATTRIBUTES: [&str; 5] = [
    "minecraft:knockback_resistance",
    "minecraft:explosion_knockback_resistance",
    "minecraft:bounciness",
    "minecraft:friction_modifier",
    "minecraft:air_drag_modifier",
];

#[cfg(test)]
mod knockback_sync_tests {
    use super::*;
    use crate::mobs::{Species};
    use std::sync::Arc;

}
/// Added controls (H192): the attribute part of `collectEquipmentChanges` on
/// the cube's own map, with the amounts and ids of the pinned 26.3
/// `sulfur_cube_archetype/{explosive,hot}.json`.
#[cfg(test)]
pub(crate) mod adopt_body_tests {
    use super::*;
    use crate::mobs::{Species};
    use horizon_world::attributes::entity::{Catalog, EntityAttributes};
    use std::sync::Arc;

    const KR: &str = "minecraft:knockback_resistance";
    const FRICTION: &str = "minecraft:friction_modifier";
    const AIR: &str = "minecraft:air_drag_modifier";


    #[test]
    fn a_body_change_swaps_the_archetype_modifiers_and_its_ids_shadow_then_drop_a_permanent_one() {
        let mut m = cube(Some("minecraft:tnt"));
        // A permanent modifier with the explosive archetype's own id, before adoption.
        let key = map(&mut m).catalog().id(KR).unwrap();
        let permanent_one = Modifier::new(
            "minecraft:explosive_add_knockback_resistance",
            0.25,
            Operation::AddValue,
        )
        .unwrap();
        map(&mut m).edit(key, |i, o| i.add_permanent(permanent_one, o).unwrap());
        assert_eq!(value(&mut m, KR), 0.25);

        adopt_body(&mut m);
        // addOrUpdateTransientModifier: the archetype's -1 shadows the permanent
        // 0.25, which stays saved.
        assert_eq!(value(&mut m, KR), -1.0);
        assert_eq!(
            permanent(&mut m, KR),
            [(
                "minecraft:explosive_add_knockback_resistance".to_string(),
                0.25
            )]
        );
        assert_eq!(value(&mut m, FRICTION), 1.0 + -0.699999988079071);
        // Nothing changes without a new body.
        adopt_body(&mut m);
        assert_eq!(value(&mut m, KR), -1.0);

        m.extra.set_body(Some("minecraft:magma_block".into()));
        adopt_body(&mut m);
        // removeModifier drops the transient and the permanent entry of that id.
        assert_eq!(permanent(&mut m, KR), []);
        assert_eq!(
            modifier(
                &mut m,
                FRICTION,
                "minecraft:explosive_mul_friction_modifier"
            ),
            None
        );
        assert_eq!(
            modifier(&mut m, AIR, "minecraft:hot_mul_air_drag_modifier"),
            Some(-0.8999999985098839)
        );
        assert_eq!(value(&mut m, KR), -1.0);

        m.extra.set_body(None);
        adopt_body(&mut m);
        assert_eq!(
            modifier(&mut m, AIR, "minecraft:hot_mul_air_drag_modifier"),
            None
        );
        for (attribute, default) in [
            (KR, 0.0),
            (FRICTION, 1.0),
            (AIR, 1.0),
            ("minecraft:bounciness", 0.0),
        ] {
            assert_eq!(value(&mut m, attribute), default, "{attribute}");
        }
    }

    #[test]
    fn the_modifier_amounts_are_the_archetype_floats_widened() {
        let by_name = |n: &str| ARCHETYPES.iter().find(|a| a.name == n).unwrap();
        // sulfur_cube_archetype/slow_bouncy.json and high_resistance.json.
        let slow_bouncy = attribute_modifiers(by_name("slow_bouncy"));
        assert_eq!(
            slow_bouncy[0].1,
            "minecraft:slow_bouncy_add_knockback_resistance"
        );
        assert_eq!(slow_bouncy[0].2, 0.4000000059604645);
        let high = attribute_modifiers(by_name("high_resistance"));
        assert_eq!(high[0].2, 0.699999988079071);
        assert_eq!(high[1].2, 0.699999988079071);
    }

    #[test]
    fn a_cube_without_a_map_or_of_another_species_adopts_nothing() {
        let mut m = cube(Some("minecraft:tnt"));
        m.extra.armor_attributes = None;
        adopt_body(&mut m);
        assert_eq!(m.extra.adopted_body, None);
        let mut other = cube(Some("minecraft:tnt"));
        Arc::make_mut(&mut other.kind).species = Species::Slime;
        adopt_body(&mut other);
        assert_eq!(value(&mut other, KR), 0.0);
    }
}

#[cfg(test)]
mod burning_arrow_tests {
    use super::*;
    use crate::mobs::{Species};
    use std::sync::Arc;


    #[test]
    fn a_burning_arrow_lights_the_long_fuse() {
        let mut m = cube(Some("minecraft:tnt"));
        assert!(burning_arrow_hit(&mut m));
        let (fuse_ticks, _, _) = archetype(&m).and_then(|a| a.explosion).unwrap();
        assert_eq!(fuse(&m), fuse_ticks);
        assert!(m.invulnerable_flag);
        // lit once: a second arrow leaves the fuse alone.
        m.extra.counter2 -= 5;
        assert!(!burning_arrow_hit(&mut m));
        assert_eq!(fuse(&m), fuse_ticks - 5);
    }

    #[test]
    fn a_burning_arrow_lights_only_an_explosive_block() {
        for head in [None, Some("minecraft:dirt")] {
            let mut m = cube(head);
            assert!(!burning_arrow_hit(&mut m), "{head:?}");
            assert!(!primed(&m) && !m.invulnerable_flag, "{head:?}");
        }
    }

    /// the arrow's knock, then the fuse lit: the next tick plays
    /// `entity.tnt.primed`, then the archetype's hit sound.
    #[test]
    #[ignore = "daily"]
    fn a_knock_and_a_lit_fuse_both_sound() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut events = Vec::new();
        let ran = crate::mobs::world::with_test_world(&[], |w, _| {
            let mut m = cube(Some("minecraft:tnt"));
            m.damage(level, 5.0, Damage::player([2.0, 0.0, 0.0]));
            assert!(burning_arrow_hit(&mut m));
            ai_step(&mut m, w, &mut events);
        });
        if !ran {
            return; // needs the prepared 26.3 data
        }
        let sounds: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                MobEvent::Sound(id, ..) => Some(*id),
                _ => None,
            })
            .collect();
        let hit = archetype_of("minecraft:tnt").unwrap().hit_sound;
        assert_eq!(sounds, ["entity.tnt.primed", hit]);
    }

    /// `SulfurCube.ageBoundaryReached` runs only where `AgeableMob.setAge`
    /// crosses the baby boundary: a loaded adult of size 1 and a big baby
    /// keep their size, and only a baby growing up becomes size 2 (#14916).
    #[test]
    #[ignore = "daily"]
    fn only_growing_up_makes_a_cube_size_2() {
        let mut events = Vec::new();
        let ran = crate::mobs::world::with_test_world(&[], |w, _| {
            // `/summon sulfur_cube` without Size or Age: an adult of size 1.
            let mut adult = cube(None);
            from_nbt(
                &mut adult,
                &horizon_core::nbt::Tag::Compound(Default::default()),
            );
            // A split of a size-4 cube: a baby of size 2.
            let mut baby = cube(None);
            baby.extra.age = -24000;
            set_size(&mut baby, 2, true);
            for tick in 1..=40 {
                adult.age = tick;
                baby.age = tick;
                ai_step(&mut adult, w, &mut events);
                ai_step(&mut baby, w, &mut events);
            }
            assert_eq!((adult.extra.size, adult.attrs.max_health), (1, 4.0));
            assert!(!adult.is_baby());
            assert_eq!((baby.extra.size, baby.attrs.max_health), (2, 8.0));
            // The baby boundary: a cub of size 1 that grows up is size 2.
            let mut cub = cube(None);
            cub.extra.age = -1;
            set_size(&mut cub, 1, true);
            crate::mobs::finalize::set_age(&mut cub, 0, Some(w));
            assert_eq!((cub.extra.size, cub.health), (2, 8.0));
        });
        if !ran {
            eprintln!("skipped: needs the prepared 26.3 data");
        }
    }
}

#[cfg(test)]
mod immune_tests {
    use super::*;
    use crate::mobs::{Species};
    use std::sync::Arc;


    /// `#sulfur_cube_with_block_immune_to` has these too: a cube with a block
    /// loses no health to any of them (and `hurtServer` still answers true), a cube without one does.
    #[test]
    fn a_cube_with_a_block_shrugs_off_the_whole_tag() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        for kind in [
            DamageKind::FallingAnvil,
            DamageKind::FallingBlock,
            DamageKind::FallingStalactite,
            DamageKind::DryOut,
            DamageKind::Spit,
            DamageKind::Stalagmite,
            DamageKind::SulfurCubeHot,
            DamageKind::WindCharge,
        ] {
            let mut m = cube(Some("minecraft:dirt"));
            let health = m.health;
            // `SulfurCube.hurtServer` returns true for these, though no health is lost.
            assert!(m.damage(level, 1.0, Damage::of(kind)), "{kind:?}");
            assert_eq!(m.health, health, "{kind:?}");
            let mut m = cube(None);
            assert!(m.damage(level, 1.0, Damage::of(kind)), "{kind:?}");
            assert_eq!(m.health, health - 1.0, "{kind:?}");
        }
    }

    /// `SulfurCube.knockback` with `comesFromEffect`: a sprint hit's extra push (power 0.5) is
    /// `0.5 * 0.25` of the plain push, a Knockback II hit's (power 1.0) a quarter of it.
    #[test]
    fn an_effect_push_is_a_fraction_of_the_blows_own() {
        let push = |scale: f32| {
            launch(
                (1.0, 0.5),
                0.0,
                4.0,
                (1.0, 0.0),
                [-2.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 0.5, 0.0],
                1.0,
                None,
                scale,
            )
        };
        let plain = push(1.0);
        assert!(plain[0].abs() > 0.01, "the plain push moves it: {plain:?}");
        for (power, scale) in [(0.5_f32, 0.125_f64), (1.0, 0.25)] {
            let extra = push(power * 0.25);
            for axis in 0..3 {
                assert!(
                    (extra[axis] - plain[axis] * scale).abs() < 1.0e-6,
                    "{power}: {extra:?} against {plain:?}"
                );
            }
        }
    }

    /// outside the tag (`magic`) a cube with a block is hurt.
    #[test]
    fn a_cube_with_a_block_takes_magic() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        let mut m = cube(Some("minecraft:dirt"));
        let health = m.health;
        assert!(m.damage(level, 1.0, Damage::of(DamageKind::Magic)));
        assert_eq!(m.health, health - 1.0);
    }
}
thread_local! {
    /// The `tnt_explodes` game rule of the level this thread ticks, which
    /// `primeTime` and `tickFuse` read (shared by the game each tick and on
    /// a rule change).
    static TNT_EXPLODES: Cell<bool> = const { Cell::new(true) };
}
/// The `tnt_explodes` rule this thread's sulfur cubes read.
pub fn tnt_explodes() -> bool {
    TNT_EXPLODES.get()
}
/// Share the `tnt_explodes` rule with this thread's sulfur cubes.
pub fn set_tnt_explodes(on: bool) {
    TNT_EXPLODES.set(on);
}
/// `Vec2.rotate`: `Mth.sin`/`Mth.cos` of the angle, in floats.
fn rotate(x: f32, y: f32, angle: f64) -> (f32, f32) {
    let (cosine, sine) = (
        horizon_core::geometry::trig::cos(angle),
        horizon_core::geometry::trig::sin(angle),
    );
    (x * cosine - y * sine, y * cosine + x * sine)
}
fn normalize(v: [f64; 3]) -> [f64; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length < f64::from(1.0e-5_f32) {
        [0.0; 3]
    } else {
        [v[0] / length, v[1] / length, v[2] / length]
    }
}
/// `SulfurCube.applyHorizontalHitAngleScale`: the knock's direction turned by how far the
/// attacker's aim is from the cube, in the horizontal plane, times the scale.
fn horizontal_hit_angle_scale(
    scale: f32,
    angle: (f32, f32),
    eye: [f64; 3],
    aim: [f64; 3],
    center: [f64; 3],
) -> (f32, f32) {
    let to_target = normalize([center[0] - eye[0], center[1] - eye[1], center[2] - eye[2]]);
    let diff = horizon_core::jmath::atan2(
        aim[0] * to_target[2] - aim[2] * to_target[0],
        aim[0] * to_target[0] + aim[2] * to_target[2],
    ) as f32;
    rotate(angle.0, angle.1, f64::from(diff * scale))
}
/// `SulfurCube.applyVerticalHitAnglePowerTransfer`: power moved between the horizontal and the
/// vertical launch by where the attacker aims between the cube's top and bottom.
fn vertical_hit_angle_power_transfer(
    scale: f32,
    powers: (f32, f32),
    eye: [f64; 3],
    aim: [f64; 3],
    center: [f64; 3],
    height: f32,
) -> (f32, f32) {
    let half = f64::from(0.5 * height);
    let top = normalize([
        center[0] - eye[0],
        center[1] + half - eye[1],
        center[2] - eye[2],
    ]);
    let bottom = normalize([
        center[0] - eye[0],
        center[1] - half - eye[1],
        center[2] - eye[2],
    ]);
    // Mth.clampedMap(aim.y, top.y, bottom.y, -1, 1): clampedLerp(inverseLerp(..), -1, 1).
    let t = (aim[1] - top[1]) / (bottom[1] - top[1]);
    let factor = if t < 0.0 {
        -1.0
    } else if t > 1.0 {
        1.0
    } else {
        -1.0 + t * 2.0
    } as f32;
    let mut ratio = (factor * scale).abs();
    if factor < 0.0 {
        ratio = -ratio;
    }
    (powers.0 * (1.0 - ratio), powers.1 * (1.0 + ratio))
}
/// The `knockback_resistance` attribute's value: vanilla clamps it to -2..1, and a negative one
/// (an explosive or bouncy block) adds knockback.
fn resistance(attribute: f64) -> f32 {
    attribute.clamp(-2.0, 1.0) as f32
}
/// `playerPush`'s scale: `Math.max(0, 1 - knockback_resistance)`, in double like vanilla.
fn push_factor(attribute: f64) -> f64 {
    (1.0 - attribute.clamp(-2.0, 1.0)).max(0.0)
}
/// What `SulfurCube.knockback` adds to the cube's movement for a blow of `damage`: the
/// archetype's `powers` (horizontal, vertical) turned by where the blow came from (`from`, the
/// direct entity: the direction of the knock) and, with the causing entity's `aim` (eye, view
/// direction), by where it points; the rotation by height uses its feet (`attacker_feet`).
#[allow(clippy::too_many_arguments)]
fn launch(
    powers: (f32, f32),
    resistance: f32,
    damage: f32,
    (xd, zd): (f64, f64),
    attacker_feet: [f64; 3],
    position: [f64; 3],
    center: [f64; 3],
    height: f32,
    aim: Option<([f64; 3], [f64; 3])>,
    effect_scale: f32,
) -> [f64; 3] {
    let (h0, v0) = powers;
    let (mut xd, mut zd) = (xd as f32, zd as f32);
    let (mut h, mut v) = (h0, v0);
    if let Some((eye, look)) = aim {
        let look = normalize(look);
        (xd, zd) = horizontal_hit_angle_scale(1.6, (xd, zd), eye, look, center);
        (h, v) = vertical_hit_angle_power_transfer(0.5, (h, v), eye, look, center, height);
    }
    // applyVerticalPositionAnglePowerRotation from feet to feet.
    let d = [
        position[0] - attacker_feet[0],
        position[1] - attacker_feet[1],
        position[2] - attacker_feet[2],
    ];
    let angle = horizon_core::jmath::atan2(-d[1], (d[0] * d[0] + d[2] * d[2]).sqrt()) as f32;
    let (mut h, mut v) = rotate(h, v, f64::from(-angle * 0.8));
    let hr = if h0 > 0.0 { h.abs() / h0 } else { 0.0 };
    let vr = if v0 > 0.0 { v.abs() / v0 } else { 0.0 };
    let max = hr.max(vr);
    if max > 1.0 {
        h *= 1.0 / max;
        v *= 1.0 / max;
    }
    // `Mth.sqrt(damage) * (comesFromEffect ? power * 0.25 : 1)`.
    let k = damage.sqrt() * effect_scale;
    h *= k;
    v *= k;
    h *= 1.0 - resistance;
    v *= 1.0 - resistance;
    h *= 0.4;
    let (h, v) = (h.clamp(-128.0, 128.0), v.clamp(-128.0, 128.0));
    let (xd, zd) = (f64::from(xd), f64::from(zd));
    let len = (xd * xd + zd * zd).sqrt();
    let (kx, kz) = if len < f64::from(1.0e-5_f32) {
        (0.0, 0.0)
    } else {
        (xd / len * f64::from(h), zd / len * f64::from(h))
    };
    [-kx, f64::from(v) * 1.2, -kz]
}


/// `SulfurCubeTemptGoal` (`TemptGoal.ForNonPathfinders`): face a player
/// holding slime balls (babies) or something to swallow (adults) and hop.
#[derive(Clone, Debug)]
struct SulfurTempt {
    player: Option<[f64; 3]>,
}
/// `SulfurCubeSearchForItemsGoal`: hop toward the nearest thing it could
/// swallow (the game marks it in `point`).
#[derive(Clone, Debug)]
struct SearchForItems;

#[cfg(test)]
mod tnt_rule_tests {
    use super::*;
    use crate::mobs::{Damage, DamageKind, Species};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;


    #[test]
    #[ignore = "daily"]
    fn mob_tick_counts_the_fuse_but_keeps_noai_cooldowns_still() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        crate::mobs::world::with_test_world(&[], |w, _| {
            for no_ai in [true, false] {
                let mut m = tnt_cube();
                m.no_ai = no_ai;
                m.no_gravity = true;
                m.persistent = true;
                m.extra.counter2 = 4; // loaded fuse 3
                m.extra.ints[3] = 100;
                m.extra.scratch[2] = 10;
                let mut events = Vec::new();
                for remaining in [2, 1] {
                    assert!(m.tick(w, level, &mut events));
                    assert_eq!(fuse(&m), remaining);
                    assert_eq!(m.extra.ints[3], if no_ai { 100 } else { 97 + remaining });
                    assert_eq!(m.extra.scratch[2], if no_ai { 10 } else { 7 + remaining });
                }
                assert!(!m.tick(w, level, &mut events));
                assert!(m.removed);
                assert_eq!(
                    events
                        .iter()
                        .filter(|e| matches!(e, MobEvent::Explode(..)))
                        .count(),
                    1
                );
                assert!(!m.tick(w, level, &mut events));
                assert_eq!(
                    events
                        .iter()
                        .filter(|e| matches!(e, MobEvent::Explode(..)))
                        .count(),
                    1
                );
            }
        });
    }

    #[test]
    fn no_fuse_is_lit_while_tnt_does_not_explode() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // primeTime checks the tnt_explodes rule: no fuse, no invulnerability,
        // and no draw for the short fuse.
        let mut m = tnt_cube();
        let state = m.random.state();
        set_tnt_explodes(false);
        m.damage(level, 5.0, Damage::of(DamageKind::Explosion));
        let lit = prime(&mut m, true);
        let (unprimed, vulnerable) = (!primed(&m), !m.invulnerable_flag);
        set_tnt_explodes(true);
        assert!(!lit && unprimed && vulnerable);
        assert_eq!(m.random.state(), state);
        assert!(prime(&mut m, false));
        assert!(m.invulnerable_flag);
    }

    #[test]
    #[ignore = "daily"]
    fn a_fuse_running_out_while_tnt_does_not_explode_makes_no_blast() {
        // tickFuse: the blast (and its source) only with tnt_explodes, the
        // discard either way.
        let mut events = Vec::new();
        let ran = crate::mobs::world::with_test_world(&[], |_, _| {
            let mut m = tnt_cube();
            assert!(prime(&mut m, false));
            m.extra.counter2 = 2; // fuse 1, 0 after this tick
            set_tnt_explodes(false);
            tick_fuse(&mut m, &mut events);
            set_tnt_explodes(true);
            assert!(m.removed);
        });
        if !ran {
            return; // needs the prepared 26.3 data
        }
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, MobEvent::Explode(..) | MobEvent::ExplosionSource { .. })),
            "{events:?}"
        );
    }
}

#[cfg(test)]
mod fuse_anim_tests {
    use super::*;
    use crate::mobs::{Species};
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;


    #[test]
    fn a_primed_cube_is_still_knocked_about() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // SulfurCube.hurtServer runs the immune knockback before
        // LivingEntity's invulnerability check, and priming makes it invulnerable.
        test_archetype_items();
        let mut m = cube();
        m.extra.set_body(Some("minecraft:tnt".into()));
        assert!(prime(&mut m, false));
        assert!(m.invulnerable_flag);
        let (health, fuse_left) = (m.health, fuse(&m));
        m.damage(level, 5.0, crate::mobs::Damage::player([2.0, 0.0, 0.0]));
        assert!(m.velocity.x < 0.0, "velocity {:?}", m.velocity);
        assert_eq!(m.health, health);
        assert_eq!(fuse(&m), fuse_left);
    }

    #[test]
    fn client_fuse_counts_down_without_going_negative() {
        let mut m = cube();
        m.extra.counter2 = 5; // fuse() == 4
        assert!(primed(&m));
        client_tick(&mut m);
        assert_eq!(fuse(&m), 3);
        for _ in 0..10 {
            client_tick(&mut m);
        }
        // Stops at counter2 == 1 → fuse 0 (primed until the server removes it).
        assert_eq!(m.extra.counter2, 1);
        assert_eq!(fuse(&m), 0);
    }

    #[test]
    fn a_rockets_burst_primes_a_tnt_cube_that_shrugs_it_off() {
        let mut test_level = crate::health::TestLevel::new(0);
        let level = &mut test_level.level();
        // `fireworks` is in #is_explosion, so it lights the short fuse
        // (`PrimedTnt.getRandomShortFuse`: 15 to 44 of 120 ticks), and so in
        // #sulfur_cube_with_block_immune_to; it is not in #no_knockback.
        test_archetype_items();
        let source = Damage::fireworks(Some(crate::mobs::Who::Player), [2.0, 0.0, 0.0]);
        assert_eq!(source.kind, DamageKind::Fireworks);
        assert!(!source.no_knockback());
        let mut m = cube();
        m.extra.set_body(Some("minecraft:tnt".into()));
        let health = m.health;
        m.damage(level, 7.0, source);
        assert!((15..45).contains(&fuse(&m)), "fuse {}", fuse(&m));
        assert_eq!(m.health, health);
        assert!(m.velocity.x < 0.0);
        // Without a block it takes the hit, records it and is knocked away
        // from the rocket.
        let mut m = cube();
        m.damage(level, 1.0, source);
        assert_eq!(m.health, health - 1.0);
        assert_eq!(
            m.last_damage.map(|(d, _)| d.kind),
            Some(DamageKind::Fireworks)
        );
        assert!(m.velocity.x < 0.0);
    }
}

#[cfg(test)]
mod launch_tests {
    use super::launch;

    /// What vanilla 26.3's `SulfurCube.knockback` adds to the movement, computed by running its
    /// three private helpers' bodies on the real `Vec2`, `Vec3` and `Mth` classes: four blows
    /// with the attacker at the blow's position (a melee hit), one with no aim (an attacker that
    /// gives none) and two from an arrow, whose position is where the knock comes from while
    /// the owner's feet give the rotation by height.
    #[test]
    fn the_launch_follows_the_vanilla_helpers() {
        struct Case {
            powers: (f32, f32),
            resistance: f32,
            damage: f32,
            /// The direct entity's position.
            from: [f64; 3],
            /// Its horizontal motion, if it is a projectile.
            motion: Option<[f64; 2]>,
            attacker_feet: [f64; 3],
            position: [f64; 3],
            aim: Option<([f64; 3], [f64; 3])>,
            expected: [f64; 3],
        }
        let height = 0.5;
        let cases = [
            Case {
                powers: (1.0, 0.6),
                resistance: 0.0,
                damage: 5.0,
                from: [0.5, 64.0, 0.5],
                motion: None,
                attacker_feet: [0.5, 64.0, 0.5],
                position: [3.5, 64.0, 0.5],
                aim: Some(([0.5, 65.62, 0.5], [1.0, -0.1, 0.0])),
                expected: [0.8944272398948669, 0.536656379699707, -0.0],
            },
            Case {
                powers: (1.0, 0.6),
                resistance: 0.25,
                damage: 8.0,
                from: [0.5, 64.0, 0.5],
                motion: None,
                attacker_feet: [0.5, 64.0, 0.5],
                position: [3.5, 64.0, 1.5],
                aim: Some(([0.5, 65.62, 0.5], [0.95, -0.3, 0.2])),
                expected: [0.7428210177564502, 0.5091168880462646, 0.41014247554144134],
            },
            Case {
                powers: (2.0, 1.0),
                resistance: 0.0,
                damage: 6.0,
                from: [10.5, 70.0, -3.5],
                motion: None,
                attacker_feet: [10.5, 70.0, -3.5],
                position: [7.5, 64.0, -2.0],
                aim: Some(([10.5, 71.62, -3.5], [-0.5, -0.7, 0.4])),
                expected: [-1.1930282398964365, -2.939387798309326, 0.15109800367878837],
            },
            Case {
                powers: (1.5, 0.8),
                resistance: 0.5,
                damage: 4.0,
                from: [0.5, 64.0, 0.5],
                motion: None,
                attacker_feet: [0.5, 64.0, 0.5],
                position: [2.5, 64.0, 0.5],
                aim: None,
                expected: [0.6000000238418579, 0.9600000143051147, -0.0],
            },
            Case {
                powers: (1.0, 0.6),
                resistance: 0.0,
                damage: 6.0,
                from: [1.5, 64.9, 0.5],
                motion: Some([0.9, 0.1]),
                attacker_feet: [-6.5, 64.0, 0.5],
                position: [3.5, 64.0, 0.5],
                aim: Some(([-6.5, 65.62, 0.5], [1.0, -0.05, 0.1])),
                expected: [0.9786302987781631, 0.5878775954246521, -0.04777874198383045],
            },
            Case {
                powers: (2.0, 1.0),
                resistance: 0.25,
                damage: 5.0,
                from: [-2.5, 66.0, 3.5],
                motion: Some([-0.7, 0.4]),
                attacker_feet: [-9.5, 64.0, 6.5],
                position: [1.5, 64.0, -0.5],
                aim: Some(([-9.5, 65.62, 6.5], [0.8, -0.2, -0.5])),
                expected: [-0.38538593044563885, 2.012461280822754, 0.22688699055044273],
            },
        ];
        for (i, c) in cases.into_iter().enumerate() {
            // The box centre: the feet position plus half the height.
            let center = [
                c.position[0],
                c.position[1] + f64::from(height) / 2.0,
                c.position[2],
            ];
            // dealDefaultKnockback: a projectile knocks the way it travelled, else away from the
            // source's position.
            let direction = match c.motion {
                Some([x, z]) => (-x, -z),
                None => (c.from[0] - c.position[0], c.from[2] - c.position[2]),
            };
            let got = launch(
                c.powers,
                c.resistance,
                c.damage,
                direction,
                c.attacker_feet,
                c.position,
                center,
                height,
                c.aim,
                1.0,
            );
            for axis in 0..3 {
                assert!(
                    (got[axis] - c.expected[axis]).abs() < 1.0e-9,
                    "case {i} axis {axis}: {got:?} against {:?}",
                    c.expected
                );
            }
        }
    }
}

#[cfg(test)]
mod resistance_tests {
    use super::{launch, push_factor, resistance};

    #[test]
    fn negative_resistance_adds_knockback() {
        assert_eq!(resistance(-1.0), -1.0);
        assert_eq!(resistance(-2.0), -2.0);
        assert_eq!(resistance(-5.0), -2.0);
        assert_eq!(resistance(3.0), 1.0);
        assert_eq!(push_factor(-1.0), 2.0);
        assert_eq!(push_factor(-2.0), 3.0);
        assert_eq!(push_factor(1.0), 0.0);
        assert_eq!(push_factor(0.3), 1.0 - 0.3);
        let push = |r: f64, damage: f32| {
            launch(
                (1.0, 0.6),
                resistance(r),
                damage,
                (1.0, 0.0),
                [0.0, 64.0, 0.0],
                [1.0, 64.0, 0.0],
                [1.0, 64.5, 0.0],
                1.0,
                None,
                1.0,
            )
        };
        let base = push(0.0, 4.0);
        let explosive = push(-1.0, 4.0);
        let bouncy = push(-2.0, 4.0);
        assert!((explosive[0] / base[0] - 2.0).abs() < 1.0e-6);
        assert!((bouncy[0] / base[0] - 3.0).abs() < 1.0e-6);
    }
}

#[cfg(test)]
mod mob_contact_tests {
    use super::*;
    use std::sync::Arc;

}
