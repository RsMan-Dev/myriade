//! Lot 6a of the rules of the mobs (`ai.rs` 1 to 44, the rules part): what never goes away for distance (villagers, traders, golems,
//! a shulker), the poison that the undead refuse, and the absorption that every accepted add refills. Expected values are the vanilla
//! 26.3 ones (`javap`: `AbstractGolem`, `Villager`, `WanderingTrader`, `MobEffects`), not the old `Mob`.
use crate::common::*;
use bevy_ecs::prelude::Entity;
use horizon_client_lib::items::ItemStack;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_ecs_proto::absorption::{absorb, Absorption};
use horizon_ecs_proto::behaviours::cat_state::Trusting;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::custom_name;
use horizon_ecs_proto::despawn::admit;
use horizon_ecs_proto::effects::{self, Brew, Invisible, Kind};
use horizon_ecs_proto::{equip_stack, nbt};

/// The species of the table by name, standing at (x, z).
fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// `Villager`, `WanderingTrader` and `AbstractGolem` return `removeWhenFarAway` false: their species has no distance despawn, while a
/// zombie does.
#[test]
fn villagers_traders_and_golems_never_despawn_for_distance() {
    if !have_data() {
        return;
    }
    for name in ["minecraft:villager", "minecraft:wandering_trader", "minecraft:iron_golem", "minecraft:snow_golem", "minecraft:copper_golem", "minecraft:shulker"] {
        let r = horizon_ecs_proto::species::registry().by_name(name).unwrap();
        assert!(r.despawn.is_none(), "{name} has a distance despawn");
    }
    let zombie = horizon_ecs_proto::species::registry().by_name("minecraft:zombie").unwrap();
    assert!(zombie.despawn.is_some(), "a zombie has no distance despawn");
}

/// `HealOrHarmMobEffect.applyEffectTick` (javap): healing gives `4 << level`, harming deals `6 << level`; a splash on the creature hit
/// takes it whole.
#[test]
fn instant_effects_shift_like_java_on_mobs() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let pig = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    let at = h.world.get::<Pos>(pig).unwrap().0;
    h.world.get_mut::<Health>(pig).unwrap().0 = 5.0;
    effects::splash(&mut h.world, at, Some(pig), &[Brew::Heal(1)], None);
    h.tick();
    assert_eq!(health(&h, pig), 13.0, "healing II gave 8 (4 << 1)");
    effects::splash(&mut h.world, at, Some(pig), &[Brew::Harm(0)], None);
    h.tick();
    assert_eq!(health(&h, pig), 7.0, "harming I did not deal 6");
}

/// `Frog.createAttributes` (javap): `MOVEMENT_SPEED` 1.0, the value the frog's species carries.
#[test]
fn a_frog_walks_at_the_vanilla_speed() {
    if !have_data() {
        return;
    }
    let h = Herd::with_slab_awake(8, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let frog = table.iter().find(|s| s.name == "minecraft:frog").unwrap();
    assert_eq!(frog.speed, 1.0, "the frog's movement speed is the vanilla 1.0");
}

/// `MobEffects.POISON` damages a living pig and not a zombie, which ignores poison and regeneration.
#[test]
fn poison_effect_damages_living_mobs_but_not_undead() {
    if !have_data() || wrong_species(&["minecraft:pig", "minecraft:zombie"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let pig = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    let zombie = spawn(&mut h, "minecraft:zombie", 2.5, 0.5);
    for e in [pig, zombie] {
        assert!(effects::add(&mut h.world, e, Kind::Poison, 0, 100), "poison refused");
    }
    let (pig0, zombie0) = (health(&h, pig), health(&h, zombie));
    for _ in 0..100 {
        h.tick();
    }
    assert!(health(&h, pig) < pig0, "poison did not hurt the pig");
    assert_eq!(health(&h, zombie), zombie0, "poison hurt the undead zombie");
}

/// `LivingEntity.addEffect` of absorption: each accepted add raises the absorption to 4 x (level + 1) at least, after a hit too; a
/// level III one gives 12. The hit takes the absorption first.
#[test]
fn absorption_refills_on_every_accepted_add() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    let amount = |h: &Herd| h.world.get::<Absorption>(e).map_or(0.0, |a| a.0);
    assert!(effects::add(&mut h.world, e, Kind::Absorption, 0, 600));
    assert_eq!(amount(&h), 4.0, "absorption I gives 4");
    absorb(&mut h.world, e, 4.0);
    assert_eq!(amount(&h), 0.0, "the hit did not take the absorption");
    effects::add(&mut h.world, e, Kind::Absorption, 0, 600);
    assert_eq!(amount(&h), 4.0, "after the hit, absorption I did not refill it to 4");
    absorb(&mut h.world, e, 2.0);
    effects::add(&mut h.world, e, Kind::Absorption, 0, 600);
    assert_eq!(amount(&h), 4.0, "absorption I on 2 left did not raise it to 4");
    effects::add(&mut h.world, e, Kind::Absorption, 2, 600);
    assert_eq!(amount(&h), 12.0, "absorption III did not give 12");
}

/// `Cat.removeWhenFarAway` and `Ocelot.removeWhenFarAway` (javap): an untamed feline older than 2400 ticks goes when far; a trusting
/// ocelot (`isTrusting`) stays.
#[test]
fn an_untamed_feline_goes_once_it_is_old() {
    if !have_data() || wrong_species(&["minecraft:cat", "minecraft:ocelot"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(200.5, 0.5)]);
    let cat = spawn(&mut h, "minecraft:cat", 0.5, 0.5);
    let ocelot = spawn(&mut h, "minecraft:ocelot", 2.5, 0.5);
    let trusting = spawn(&mut h, "minecraft:ocelot", 4.5, 0.5);
    h.world.entity_mut(trusting).insert(Trusting);
    for (e, name) in [(cat, "minecraft:cat"), (ocelot, "minecraft:ocelot"), (trusting, "minecraft:ocelot")] {
        let def = *horizon_ecs_proto::species::registry().by_name(name).unwrap().despawn.unwrap();
        admit(&mut h.world, e, def, 200.0 * 200.0);
    }
    for _ in 0..6000 {
        h.tick();
    }
    assert!(h.world.get_entity(cat).is_err(), "an untamed cat older than 2400 ticks stayed");
    assert!(h.world.get_entity(ocelot).is_err(), "an untamed ocelot older than 2400 ticks stayed");
    assert!(h.world.get_entity(trusting).is_ok(), "a trusting ocelot went");
}

/// `AbstractFish.removeWhenFarAway` and `Axolotl.removeWhenFarAway` (javap): `fromBucket() || hasCustomName()` keeps the creature; a
/// name saves no other mob, so a named zombie goes. An unnamed cod goes.
#[test]
fn a_named_fish_or_axolotl_never_despawns() {
    if !have_data() || wrong_species(&["minecraft:cod", "minecraft:axolotl", "minecraft:zombie"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(200.5, 0.5)]);
    let named_cod = spawn(&mut h, "minecraft:cod", 0.5, 0.5);
    let bare_cod = spawn(&mut h, "minecraft:cod", 2.5, 0.5);
    let named_axolotl = spawn(&mut h, "minecraft:axolotl", 4.5, 0.5);
    let named_zombie = spawn(&mut h, "minecraft:zombie", 6.5, 0.5);
    for e in [named_cod, named_axolotl, named_zombie] {
        custom_name::set(&mut h.world, e, Some("Bubbles".to_owned()));
    }
    for (e, name) in [(named_cod, "minecraft:cod"), (bare_cod, "minecraft:cod"), (named_axolotl, "minecraft:axolotl"), (named_zombie, "minecraft:zombie")] {
        let def = *horizon_ecs_proto::species::registry().by_name(name).unwrap().despawn.unwrap();
        admit(&mut h.world, e, def, 200.0 * 200.0);
    }
    for _ in 0..6000 {
        h.tick();
    }
    assert!(h.world.get_entity(named_cod).is_ok(), "a named cod went");
    assert!(h.world.get_entity(bare_cod).is_err(), "an unnamed cod stayed");
    assert!(h.world.get_entity(named_axolotl).is_ok(), "a named axolotl went");
    assert!(h.world.get_entity(named_zombie).is_err(), "a named zombie stayed");
}

/// `LivingEntity.getDamageAfterMagicAbsorb` (javap): Resistance I takes 5 of 25 off a hurt (6 becomes 4.8), Resistance II 10 of 25
/// (6 becomes 3.6).
#[test]
fn resistance_takes_five_of_twenty_five_per_level_off_a_hurt() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let one = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    let two = spawn(&mut h, "minecraft:pig", 2.5, 0.5);
    let at_one = h.world.get::<Pos>(one).unwrap().0;
    let at_two = h.world.get::<Pos>(two).unwrap().0;
    assert!(effects::add(&mut h.world, one, Kind::Resistance, 0, 600));
    assert!(effects::add(&mut h.world, two, Kind::Resistance, 1, 600));
    effects::splash(&mut h.world, at_one, Some(one), &[Brew::Harm(0)], None);
    effects::splash(&mut h.world, at_two, Some(two), &[Brew::Harm(0)], None);
    h.tick();
    assert!((health(&h, one) - 15.2).abs() < 1e-3, "resistance I did not take 1.2 of the 6");
    assert!((health(&h, two) - 16.4).abs() < 1e-3, "resistance II did not take 2.4 of the 6");
}

/// `LivingEntity.updateDirtyEffects` (javap): the invisibility flag is `hasEffect(INVISIBILITY)`, recomputed on each effect change; a
/// second effect does not clear it, and it goes when the invisibility ends.
#[test]
fn invisibility_follows_the_effect_on_every_change() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let pig = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    assert!(effects::add(&mut h.world, pig, Kind::Invisibility, 0, 40));
    assert!(h.world.get::<Invisible>(pig).is_some(), "invisibility did not hide the pig");
    assert!(effects::add(&mut h.world, pig, Kind::Speed, 0, 10));
    assert!(h.world.get::<Invisible>(pig).is_some(), "a second effect cleared the invisibility");
    for _ in 0..41 {
        h.tick();
    }
    assert!(h.world.get::<Invisible>(pig).is_none(), "the pig stayed invisible after the effect ended");
}

/// `PoisonMobEffect.applyEffectTick` hurts 1.0 magic, and Resistance I takes 5 of 25 off it: 0.8 of 1.0 (19.2 from 20).
#[test]
fn poison_hurts_less_under_resistance() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let pig = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    assert!(effects::add(&mut h.world, pig, Kind::Resistance, 0, 600));
    assert!(effects::add(&mut h.world, pig, Kind::Poison, 0, 100));
    h.tick();
    assert!((health(&h, pig) - 19.2).abs() < 1e-3, "poison under resistance I did not deal 0.8");
}

/// `MobEffectInstance.tickServer` returns false before any tick when there is no remaining duration (javap): a zero-duration poison
/// is over, with no damage and no effect left.
#[test]
fn a_zero_duration_effect_is_dropped_without_a_tick() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let pig = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    effects::add(&mut h.world, pig, Kind::Poison, 0, 0);
    h.tick();
    assert!(!effects::has(&h.world, pig, Kind::Poison), "a zero-duration poison stayed");
    assert_eq!(health(&h, pig), 20.0, "a zero-duration poison hurt the pig");
}

/// `checkTotemDeathProtection` (javap): a lethal effect hit that a totem saves leaves the totem's regeneration on the mob after the tick.
#[test]
fn a_totem_fired_by_effect_damage_keeps_its_effects() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let pig = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    horizon_ecs_proto::equip_stack::set(&mut h.world, pig, 0, Some(horizon_client_lib::items::ItemStack::new("minecraft:totem_of_undying", 1)));
    h.world.resource_mut::<DamageQueue>().add(pig, 30.0, Cause::Magic);
    h.tick();
    assert_eq!(health(&h, pig), 1.0, "the totem did not save the pig at 1 health");
    assert!(effects::has(&h.world, pig, Kind::Regeneration), "the totem's regeneration was lost after the tick");
}

/// `AbstractFish.requiresCustomPersistence` (javap: `fromBucket()`): a fish or an axolotl from a bucket is kept from the despawns, and
/// it is saved as `FromBucket` alone, not as `PersistenceRequired`. The save and the restore keep the mark; the natural ones go.
#[test]
fn a_creature_from_a_bucket_never_despawns_and_saves_no_persistence() {
    if !have_data() || wrong_species(&["minecraft:cod", "minecraft:axolotl"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(200.5, 0.5)]);
    let cod = spawn(&mut h, "minecraft:cod", 0.5, 0.5);
    let axolotl = spawn(&mut h, "minecraft:axolotl", 2.5, 0.5);
    let natural_cod = spawn(&mut h, "minecraft:cod", 4.5, 0.5);
    let natural_axolotl = spawn(&mut h, "minecraft:axolotl", 6.5, 0.5);
    let mut from_bucket = Compound::new();
    from_bucket.insert(JavaString::from("FromBucket"), Tag::Byte(1));
    assert!(nbt::load(&mut h.world, cod, &from_bucket).is_some(), "the cod refused FromBucket");
    assert!(nbt::load(&mut h.world, axolotl, &from_bucket).is_some(), "the axolotl refused FromBucket");
    let saved = nbt::save(&h.world, cod).expect("a saved cod");
    assert_eq!(saved.get(&JavaString::from("FromBucket")), Some(&Tag::Byte(1)), "FromBucket was not saved");
    assert_eq!(saved.get(&JavaString::from("PersistenceRequired")), None, "a bucket cod saved PersistenceRequired");
    let back = nbt::restore(&mut h.world, &saved).expect("a declared cod");
    for (e, name) in [(cod, "minecraft:cod"), (axolotl, "minecraft:axolotl"), (back, "minecraft:cod"), (natural_cod, "minecraft:cod"), (natural_axolotl, "minecraft:axolotl")] {
        let def = *horizon_ecs_proto::species::registry().by_name(name).unwrap().despawn.unwrap();
        admit(&mut h.world, e, def, 200.0 * 200.0);
    }
    for _ in 0..6000 {
        h.tick();
    }
    assert!(h.world.get_entity(cod).is_ok(), "a cod from a bucket went");
    assert!(h.world.get_entity(back).is_ok(), "a cod restored with FromBucket went");
    assert!(h.world.get_entity(axolotl).is_ok(), "an axolotl from a bucket went");
    assert!(h.world.get_entity(natural_cod).is_err(), "a natural cod stayed");
    assert!(h.world.get_entity(natural_axolotl).is_err(), "a natural axolotl stayed");
}

/// `checkTotemDeathProtection` (javap): `killingDamage.is(BYPASSES_INVULNERABILITY)` gives no totem; the tag holds `out_of_world` and
/// `generic_kill` (jar data/minecraft/tags/damage_type). The void's hit leaves the totem in hand and no regeneration.
#[test]
fn a_totem_does_not_save_from_the_void() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let pig = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    horizon_ecs_proto::equip_stack::set(&mut h.world, pig, 0, Some(horizon_client_lib::items::ItemStack::new("minecraft:totem_of_undying", 1)));
    h.world.resource_mut::<DamageQueue>().add(pig, 30.0, Cause::OutOfWorld);
    h.tick();
    assert!(equip_stack::get(&h.world, pig, 0).is_some(), "the void's hit used the totem");
    assert!(!effects::has(&h.world, pig, Kind::Regeneration), "the void's hit gave the totem's regeneration");
}

/// `getDamageAfterMagicAbsorb`: Resistance V (`(amp + 1) * 5` = 25) takes the whole hurt, and a higher level takes no less.
#[test]
fn resistance_of_level_five_or_more_stops_the_hurt() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let pig = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    let at = h.world.get::<Pos>(pig).unwrap().0;
    assert!(effects::add(&mut h.world, pig, Kind::Resistance, 4, 600));
    effects::splash(&mut h.world, at, Some(pig), &[Brew::Harm(0)], None);
    h.tick();
    assert_eq!(health(&h, pig), 20.0, "resistance V did not stop the magic hurt");
}
