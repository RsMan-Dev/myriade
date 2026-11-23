//! L'échelle du corps (`LivingEntity.getScale`, `minecraft:scale`) : la boîte et les yeux suivent, bornés comme le jar, plafonnés par espèce,
//! la boîte endormie reste fixe. Valeurs vanilla 26.3 relues dans le jar (`EntityType.COW` 0.9 x 1.4, yeux 1.3 ; `SLEEPING_DIMENSIONS` 0.2).
use crate::common::*;
use bevy_ecs::entity::Entity;
use horizon_ecs_proto::attr_mod::{touched, AttrMods, Modifier, Operation};
use horizon_ecs_proto::behaviours::bed::lie_down;
use horizon_ecs_proto::rules::mount::seat_point;
use horizon_ecs_proto::cow::{model_scale, spawn_species, Body, Gravity, GrowAt, SpeciesTable};
use horizon_ecs_proto::mount::Mount;
use horizon_ecs_proto::targeting::{Eye, FollowRange};

fn spawn(h: &mut Herd, name: &str) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let species = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &species, 4.5, 4.5, 5)
}

/// Écrit la base de l'attribut `scale` comme une mise à jour serveur, puis lance son événement.
fn set_scale(h: &mut Herd, e: Entity, base: f64) {
    h.world.entity_mut(e).insert(AttrMods(vec![("scale".to_string(), base, Vec::new())]));
    touched(&mut h.world, e, "scale");
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

/// `LivingEntity.maxUpStep` reads `Attributes.STEP_HEIGHT` alone: a written step is the body's step, and the scale leaves it as it is.
#[test]
fn a_step_height_attribute_sets_the_step_and_scale_does_not_change_it() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a, "minecraft:cow");
    a.world.entity_mut(cow).insert(AttrMods(vec![("step_height".to_string(), 1.5, Vec::new())]));
    touched(&mut a.world, cow, "step_height");
    assert!(close(a.world.get::<Body>(cow).unwrap().step_height, 1.5));
    set_scale(&mut a, cow, 2.0);
    assert!(close(a.world.get::<Body>(cow).unwrap().step_height, 1.5), "step at scale 2: {}", a.world.get::<Body>(cow).unwrap().step_height);
}

/// `LivingEntity.getDimensions` is `getDefaultDimensions(pose).scale(getScale())` on every living entity: a cow at scale 2 is 1.8 x 2.8
/// with its eyes at 2.6, and the floor of the range (0.0625) shrinks it to 0.05625 x 0.0875, eyes at 0.08125.
#[test]
fn a_scale_attribute_grows_every_living_entitys_box() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a, "minecraft:cow");
    assert_eq!(model_scale(&a.world, cow), 1.0);
    set_scale(&mut a, cow, 2.0);
    assert_eq!(model_scale(&a.world, cow), 2.0);
    let body = *a.world.get::<Body>(cow).unwrap();
    assert!(close(body.width, 1.8) && close(body.height, 2.8), "scale 2: {}x{}", body.width, body.height);
    assert!(close(a.world.get::<Eye>(cow).unwrap().0 as f32, 2.6), "scale 2 eyes: {}", a.world.get::<Eye>(cow).unwrap().0);
    set_scale(&mut a, cow, 0.0625);
    let body = *a.world.get::<Body>(cow).unwrap();
    assert!(close(body.width, 0.05625) && close(body.height, 0.0875), "scale 0.0625: {}x{}", body.width, body.height);
    assert!(close(a.world.get::<Eye>(cow).unwrap().0 as f32, 0.08125));
    // The range's top: 16 at most, whatever the write.
    set_scale(&mut a, cow, 100.0);
    assert_eq!(model_scale(&a.world, cow), 16.0);
}

/// `LivingEntity.getDimensions` answers a sleeping entity with `SLEEPING_DIMENSIONS` before it reaches `getScale`: a sleeper's 0.2 box and
/// eyes stay 0.2 whatever its `minecraft:scale`.
#[test]
fn a_sleeping_mob_keeps_its_bed_box_unscaled() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let villager = spawn(&mut a, "minecraft:villager");
    set_scale(&mut a, villager, 4.0);
    lie_down(&mut a.world, villager, [4, 5, 4]);
    a.world.flush();
    let body = *a.world.get::<Body>(villager).unwrap();
    assert!(close(body.width, 0.2) && close(body.height, 0.2), "asleep: {}x{}", body.width, body.height);
    assert!(close(a.world.get::<Eye>(villager).unwrap().0 as f32, 0.2));
}

/// `Entity.getDefaultDimensions` scales a baby's type dimensions by `getAgeScale` (0.5), eyes included: a cow's eyes at 1.3 are 0.65 as
/// a baby, and the scale attribute multiplies them on top.
#[test]
fn a_baby_has_its_eyes_at_the_age_scale() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a, "minecraft:cow");
    a.world.entity_mut(cow).insert(GrowAt(u64::MAX));
    a.world.flush();
    assert!(close(a.world.get::<Eye>(cow).unwrap().0 as f32, 0.65), "baby eyes: {}", a.world.get::<Eye>(cow).unwrap().0);
    set_scale(&mut a, cow, 2.0);
    assert!(close(a.world.get::<Eye>(cow).unwrap().0 as f32, 1.3), "baby eyes at scale 2: {}", a.world.get::<Eye>(cow).unwrap().0);
}

/// `LivingEntity.sanitizeScale`: a shulker takes at most 3 and a happy ghast at most 1; every other species takes the attribute as it came.
/// The box takes the capped value.
#[test]
fn a_species_caps_the_scale_it_takes() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let table = a.world.resource::<SpeciesTable>().0.clone();
    let width = |name: &str| table.iter().find(|s| s.name == name).unwrap().width;
    let shulker = spawn(&mut a, "minecraft:shulker");
    set_scale(&mut a, shulker, 4.0);
    assert_eq!(model_scale(&a.world, shulker), 3.0);
    assert!(close(a.world.get::<Body>(shulker).unwrap().width, 3.0 * width("minecraft:shulker")));
    let ghast = spawn(&mut a, "minecraft:happy_ghast");
    set_scale(&mut a, ghast, 2.0);
    assert_eq!(model_scale(&a.world, ghast), 1.0);
    assert!(close(a.world.get::<Body>(ghast).unwrap().width, width("minecraft:happy_ghast")));
    let cow = spawn(&mut a, "minecraft:cow");
    set_scale(&mut a, cow, 4.0);
    assert_eq!(model_scale(&a.world, cow), 4.0);
}

/// `Entity.getDefaultPassengerAttachmentPoint` reads the dimensions' attachments, which `EntityDimensions.scale` scales with the box
/// (`EntityAttachments.scale(f, g, f)`): a rider's seat rises in proportion to the scaled height.
#[test]
fn a_mounts_seat_follows_its_scaled_box() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let horse = spawn(&mut a, "minecraft:horse");
    let at = horizon_core::geometry::Vec3::new(4.5, 64.0, 4.5);
    let rise = |a: &Herd| seat_point(at, 0.0, a.world.get::<Body>(horse).unwrap().height, 0.0, 0.5).y - 64.0;
    let adult = rise(&a);
    set_scale(&mut a, horse, 2.0);
    assert!(close(rise(&a) as f32, 2.0 * adult as f32), "seat at scale 2: {} against {}", rise(&a), 2.0 * adult);
}

/// Écrit la base d'un attribut quelconque, puis lance son événement (`touched`).
fn set_attr(h: &mut Herd, e: Entity, name: &str, base: f64) {
    h.world.entity_mut(e).insert(AttrMods(vec![(name.to_string(), base, Vec::new())]));
    touched(&mut h.world, e, name);
}

/// `LivingEntity.getSpeed` is `Attributes.MOVEMENT_SPEED` (modifiers included): a written base sets the body's speed, and a multiplier
/// modifier doubles it (`AddMultipliedTotal`, the amount 1.0).
#[test]
fn a_movement_speed_attribute_sets_the_body_speed() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a, "minecraft:cow");
    set_attr(&mut a, cow, "movement_speed", 0.5);
    assert!(close(a.world.get::<Body>(cow).unwrap().speed as f32, 0.5));
    a.world.entity_mut(cow).insert(AttrMods(vec![("movement_speed".to_string(), 0.5, vec![Modifier { id: "test:double".to_string(), amount: 1.0, operation: Operation::AddMultipliedTotal, permanent: false }])]));
    touched(&mut a.world, cow, "movement_speed");
    assert!(close(a.world.get::<Body>(cow).unwrap().speed as f32, 1.0));
}

/// `AbstractHorse.getJumpStrength` is the `Attributes.JUMP_STRENGTH` value the mount's jump reads.
#[test]
fn a_jump_strength_attribute_sets_the_mounts_jump() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a, "minecraft:cow");
    a.world.entity_mut(cow).insert(Mount::default());
    set_attr(&mut a, cow, "jump_strength", 0.8);
    assert!(close(a.world.get::<Mount>(cow).unwrap().jump_strength, 0.8));
}

/// `LivingEntity.getFollowRange` is `Attributes.FOLLOW_RANGE`: a written range is the one the mob's sensors read.
#[test]
fn a_follow_range_attribute_sets_the_follow_range() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a, "minecraft:cow");
    set_attr(&mut a, cow, "follow_range", 10.0);
    assert!((a.world.get::<FollowRange>(cow).unwrap().0 - 10.0).abs() < 1e-9);
}

/// `Body.fall_factor` is the hen's glide (`Chicken.aiStep`), not `Attributes.FALL_DAMAGE_MULTIPLIER`: writing the attribute leaves it as it was.
#[test]
fn a_fall_damage_multiplier_attribute_does_not_change_the_glide() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a, "minecraft:cow");
    set_attr(&mut a, cow, "fall_damage_multiplier", 2.0);
    assert!((a.world.get::<Body>(cow).unwrap().fall_factor - 1.0).abs() < 1e-9);
}

/// `LivingEntity.getGravity` is `Attributes.GRAVITY` (0.08 at rest): the body's `Gravity` follows the written base.
#[test]
fn a_gravity_attribute_sets_the_gravity() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a, "minecraft:cow");
    assert!((a.world.get::<Gravity>(cow).unwrap().0 - 0.08).abs() < 1e-9);
    set_attr(&mut a, cow, "gravity", 0.04);
    assert!((a.world.get::<Gravity>(cow).unwrap().0 - 0.04).abs() < 1e-9);
}

/// `LivingEntity.doHurtTarget` pushes with `Attributes.ATTACK_KNOCKBACK` (0 at rest): the read takes the written base and its modifiers.
#[test]
fn an_attack_knockback_attribute_is_read_at_the_hit() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a, "minecraft:cow");
    assert_eq!(horizon_ecs_proto::attr_mod::read(&a.world, cow, "attack_knockback", 0.0), 0.0);
    set_attr(&mut a, cow, "attack_knockback", 1.5);
    assert!((horizon_ecs_proto::attr_mod::read(&a.world, cow, "attack_knockback", 0.0) - 1.5).abs() < 1e-9);
}

/// `LivingEntity.igniteForTicks` multiplies the ticks by `Attributes.BURNING_TIME` (1 at rest) and rounds up: a written 2 doubles the fire.
#[test]
fn a_burning_time_attribute_doubles_the_fire() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a, "minecraft:cow");
    set_attr(&mut a, cow, "burning_time", 2.0);
    let now = horizon_ecs_proto::cow::now_of(&a.world, cow) as i64;
    horizon_ecs_proto::cow::set_on_fire(&mut a.world, cow, 100);
    assert_eq!(a.world.get::<horizon_ecs_proto::cow::Burning>(cow).unwrap().until - now, 200);
}

/// `LivingEntity.setAbsorptionAmount` clamps to `getMaxAbsorption` (`Attributes.MAX_ABSORPTION`, 0 at rest): a written cap of 4 keeps 10 at 4.
#[test]
fn a_max_absorption_attribute_caps_the_absorption() {
    if !have_data() {
        return;
    }
    let mut a = Herd::new(0, 1.0, true, false, 1);
    let cow = spawn(&mut a, "minecraft:cow");
    set_attr(&mut a, cow, "max_absorption", 4.0);
    horizon_ecs_proto::absorption::set(&mut a.world, cow, 10.0);
    assert_eq!(a.world.get::<horizon_ecs_proto::absorption::Absorption>(cow).unwrap().0, 4.0);
}
