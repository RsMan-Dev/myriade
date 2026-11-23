//! Lot 6c of the rules of the mobs (`ai.rs` 67 to 88): a silverfish prefers an infested host, an iron golem targets the monster that
//! bumps it, a held weapon adds its attack, the powder snow never freezes a skeleton, the breathing of the water creatures, the walk
//! value of the animals. Expected values are the vanilla 26.3 ones (`javap`, `Silverfish`, `Animal`, `Entity.baseTick`, `Skeleton`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::colony_register::MiscOut;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::effects::CanBreatheUnderWater;
use horizon_ecs_proto::equipment::set_slot;
use horizon_ecs_proto::goals::Slot;
use horizon_ecs_proto::host_out_misc::MiscEvent;
use horizon_ecs_proto::hostile::PlayerState;
use horizon_ecs_proto::items;
use horizon_ecs_proto::spots::{walk_target_value, WalkValue};
use horizon_ecs_proto::targeting::{Target, Who};

/// The species of the table by name, standing at (x, z).
fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// The level's world the walk values read.
fn mob_world(h: &Herd) -> &'static horizon_client_lib::mobs::world::MobWorld<'static> {
    h.world.resource::<Level>().mw.0
}

#[test]
fn a_silverfish_prefers_a_block_on_an_infested_host() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, false, false);
    h.set_block(0, 64, 0, "minecraft:stone");
    h.set_block(2, 64, 0, "minecraft:dirt");
    let mw = mob_world(&h);
    // `Silverfish.getWalkTargetValue`: the spot on an infested host is 10, the others the monster's value.
    assert_eq!(walk_target_value(mw, WalkValue::Silverfish, [0, 65, 0]), 10.0);
    assert!(walk_target_value(mw, WalkValue::Silverfish, [2, 65, 0]) < 10.0);
}

#[test]
fn an_iron_golem_targets_the_monster_that_bumps_it() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, false, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let slime = *table.iter().find(|s| s.name == "minecraft:slime").unwrap();
    let c = spawn_species(&mut h.world, &slime, 0.5, 0.5, 4246);
    horizon_ecs_proto::size::set_size(&mut h.world, c, 2);
    h.world.resource_mut::<Entered>().0.push(c);
    let g = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5);
    h.world.resource_mut::<Entered>().0.push(g);
    let mut told = None;
    for _ in 0..40 {
        h.tick();
        told = told.or(h.world.get::<Target>(g).map(|t| t.who));
    }
    // `HurtByTargetGoal`: the one that hurt the golem by touch becomes its target.
    assert_eq!(told, Some(Who::Entity(c)));
}

#[test]
fn a_held_weapon_adds_its_attack_damage_to_the_melee_hit() {
    if !have_data() {
        return;
    }
    // (zombie's base attack, iron sword's modifier 5): the hit on the player is the sum.
    let mut h = Herd::with_slab_awake(60, false, false);
    let z = spawn(&mut h, "minecraft:zombie", 0.5, 0.5);
    h.world.resource_mut::<Entered>().0.push(z);
    set_slot(&mut h.world, z, Slot::Main, items::item_id("minecraft:iron_sword"));
    let base = h.world.get::<AttackDamage>(z).map_or(0.0, |d| d.0) as f64;
    h.world.resource_mut::<PlayerState>().immortal = true;
    place_player(&mut h, 1.5);
    let mut hit = None;
    for _ in 0..60 {
        h.tick();
        hit = hit.or(h.world.resource::<PlayerState>().hits.iter().find(|x| x.player == 1).map(|x| x.amount));
    }
    assert_eq!(hit.map(f64::from), Some(base + items::attack_damage_of(items::item_id("minecraft:iron_sword"))));
}

/// The host's way to give a player beside the mobs (as in `cube_contact_damage`).
fn place_player(h: &mut Herd, x: f64) {
    let fact = horizon_ecs_proto::bridge::players::PlayerFact { alive: true, targetable: true, hero: None, ..Default::default() };
    horizon_ecs_proto::bridge::players::set_player(&mut h.world, Player { id: 1, x, y: 1.0, z: 0.5, ..Default::default() }, fact);
}

#[test]
fn a_skeleton_never_freezes() {
    if !have_data() {
        return;
    }
    // `Skeleton.canFreeze` is false: the powder snow adds no frozen ticks and no freeze damage, a cow beside it does.
    for (name, freezes) in [("minecraft:skeleton", false), ("minecraft:cow", true)] {
        let mut h = Herd::with_slab_awake(60, false, false);
        h.set_block(0, 1, 0, "minecraft:powder_snow");
        let e = spawn(&mut h, name, 0.5, 0.5);
        h.world.resource_mut::<Entered>().0.push(e);
        let full = h.world.get::<Health>(e).unwrap().0;
        for _ in 0..200 {
            h.tick();
        }
        let frozen = h.world.get::<Frozen>(e).map_or(0, |f| f.0);
        assert_eq!(frozen > 0, freezes, "{name}: frozen {frozen}");
        assert_eq!(h.world.get::<Health>(e).map_or(0.0, |x| x.0) < full, freezes, "{name}: health");
    }
}

#[test]
fn a_stroll_scores_its_ground_the_way_the_jar_does() {
    if !have_data() {
        return;
    }
    // `Animal.getWalkTargetValue`: grass 10 under the animal, else the light above the dark; the monster the dark.
    let mut h = Herd::with_slab_awake(60, false, false);
    h.set_block(-2, 63, 0, "minecraft:grass_block");
    h.set_block(0, 63, 0, "minecraft:stone");
    h.set_block(2, 63, 0, "minecraft:mycelium");
    let mw = mob_world(&h);
    let light = mw.brightness(0, 64, 0) - 0.5;
    assert_eq!(walk_target_value(mw, WalkValue::Animal, [-2, 64, 0]), 10.0);
    assert_eq!(walk_target_value(mw, WalkValue::Animal, [0, 64, 0]), light);
    assert_eq!(walk_target_value(mw, WalkValue::Mooshroom, [2, 64, 0]), 10.0);
    assert_eq!(walk_target_value(mw, WalkValue::Monster, [0, 64, 0]), -light);
    assert_eq!(walk_target_value(mw, WalkValue::Zero, [0, 64, 0]), 0.0);
}

/// A 25 x 25 pool two blocks deep under a stone lid, ticked once so that the cells are read.
fn lidded_pool(h: &mut Herd) {
    for x in -12..=12 {
        for z in -12..=12 {
            h.set_block(x, 1, z, "minecraft:water");
            h.set_block(x, 2, z, "minecraft:water");
            h.set_block(x, 3, z, "minecraft:stone");
        }
    }
    h.tick();
}

#[test]
fn the_types_the_jar_lists_breathe_under_water() {
    if !have_data() {
        return;
    }
    // `minecraft:can_breathe_under_water` as the jar lists it (the undead tag left out): each listed type carries the marker.
    let listed = ["minecraft:axolotl", "minecraft:frog", "minecraft:guardian", "minecraft:elder_guardian", "minecraft:turtle", "minecraft:glow_squid", "minecraft:cod", "minecraft:pufferfish", "minecraft:salmon", "minecraft:squid", "minecraft:tropical_fish", "minecraft:tadpole", "minecraft:copper_golem"];
    let mut h = Herd::with_slab_awake(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut seen = 0;
    for name in listed {
        let Some(sp) = table.iter().find(|s| s.name == name) else { continue };
        let e = spawn_species(&mut h.world, sp, 0.5, 0.5, 9);
        seen += 1;
        assert!(h.world.get::<CanBreatheUnderWater>(e).is_some(), "{name} carries can_breathe_under_water");
    }
    assert!(seen >= 8, "{seen} listed types spawned");
}

#[test]
fn a_water_creature_below_minus_twenty_still_drowns() {
    if !have_data() {
        return;
    }
    // `LivingEntity.baseTick`: the air at -20 or below drowns, whatever its value below.
    let mut h = Herd::with_slab_awake(60, true, false);
    lidded_pool(&mut h);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
    h.world.resource_mut::<Entered>().0.push(e);
    h.world.get_mut::<Air>(e).unwrap().0 = -25;
    for _ in 0..3 {
        h.tick();
    }
    assert!(h.world.get_resource::<MiscOut>().is_some_and(|m| m.0.iter().any(|x| matches!(x, MiscEvent::Drowned { entity, .. } if *entity == e))), "drowned below -20");
}

#[test]
fn a_copper_golem_breathes_under_water() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    lidded_pool(&mut h);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:copper_golem").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 62);
    h.world.resource_mut::<Entered>().0.push(e);
    for _ in 0..400 {
        h.tick();
    }
    let drowned = h.world.get_resource::<MiscOut>().is_some_and(|m| m.0.iter().any(|x| matches!(x, MiscEvent::Drowned { entity, .. } if *entity == e)));
    assert!(!drowned, "a copper golem under water never drowns");
}

#[test]
fn a_stronger_speed_waits_hidden_behind_a_shorter_weaker_one() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, false, false);
    let e = spawn(&mut h, "minecraft:copper_golem", 0.5, 0.5);
    use horizon_ecs_proto::effects::{add, Effects, Kind};
    assert!(add(&mut h.world, e, Kind::Speed, 1, 600));
    assert!(add(&mut h.world, e, Kind::Speed, 0, 100));
    let shown = |w: &World| {
        let f = w.get::<Effects>(e).unwrap();
        (f.list.iter().map(|x| x.amp).collect::<Vec<_>>(), f.hidden.len())
    };
    // `MobEffectInstance.update`: the stronger one is hidden while the shorter weaker one runs.
    assert_eq!(shown(&h.world), (vec![0], 1));
    for _ in 0..150 {
        h.tick();
    }
    // `downgradeToHiddenEffect`: the stronger one comes back when the weaker one ends.
    assert_eq!(shown(&h.world), (vec![1], 0));
}

#[test]
fn an_allay_heals_one_point_on_each_tenth_tick() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, false, false);
    let e = spawn(&mut h, "minecraft:allay", 0.5, 0.5);
    h.world.get_mut::<Health>(e).unwrap().0 = 10.0;
    for _ in 0..9 {
        h.tick();
    }
    // `Allay.aiStep`: the heal runs when the age is a multiple of 10, from the server's side.
    assert_eq!(h.world.get::<Health>(e).unwrap().0, 10.0, "no heal before the tenth tick");
    h.tick();
    assert_eq!(h.world.get::<Health>(e).unwrap().0, 11.0, "one point on the tenth tick");
    for _ in 0..10 {
        h.tick();
    }
    assert_eq!(h.world.get::<Health>(e).unwrap().0, 12.0, "and one more on the twentieth");
}

#[test]
fn a_blocked_ravager_is_stunned_then_roars() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::behaviours::ravager_roar::OCC_ROARING;
    use horizon_ecs_proto::behaviours::shield_stun::{on_blocked, OCC_STUNNED};
    use horizon_ecs_proto::goals::is_occupied;
    let mut h = Herd::with_slab_awake(60, false, false);
    let e = spawn(&mut h, "minecraft:ravager", 0.5, 0.5);
    // `Ravager.blockedByItem`: each blow draws once; the stun comes with the first draw that succeeds.
    let stunned = (0..64).any(|_| on_blocked(&mut h.world, e));
    assert!(stunned, "a blocked blow stuns the ravager at its chance");
    assert!(is_occupied(&h.world, e, OCC_STUNNED));
    for _ in 0..40 {
        h.tick();
    }
    // The stun's end starts the roar, which is over after its 20 ticks.
    assert!(!is_occupied(&h.world, e, OCC_STUNNED), "the stun lasts 40 ticks");
    assert!(is_occupied(&h.world, e, OCC_ROARING), "the stun's end starts the roar");
    for _ in 0..25 {
        h.tick();
    }
    assert!(!is_occupied(&h.world, e, OCC_ROARING), "the roar is over");
}

#[test]
fn a_hidden_absorption_takes_over_at_its_own_level() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::absorption::Absorption;
    use horizon_ecs_proto::effects::{add, Effects, Kind};
    let mut h = Herd::with_slab_awake(60, false, false);
    let e = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    assert!(add(&mut h.world, e, Kind::Absorption, 0, 600));
    assert!(add(&mut h.world, e, Kind::Absorption, 1, 2));
    // `MobEffectInstance.update`: the shorter stronger one shows, the longer weaker one waits hidden.
    assert_eq!(h.world.get::<Effects>(e).unwrap().hidden.len(), 1, "Absorption I waits");
    assert_eq!(h.world.get::<Absorption>(e).unwrap().0, 8.0);
    for _ in 0..3 {
        h.tick();
    }
    let f = h.world.get::<Effects>(e).unwrap();
    assert_eq!(f.list.iter().map(|x| x.amp).collect::<Vec<_>>(), vec![0], "Absorption I took over");
    assert!(f.hidden.is_empty());
    // `onEffectStarted` of the promoted one: the amount is clamped to its level.
    assert_eq!(h.world.get::<Absorption>(e).unwrap().0, 4.0);
}

#[test]
fn a_happy_ghast_ambient_sound_plays_at_its_position() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::host_out::HostEvent;
    let mut h = Herd::with_slab_awake(60, false, false);
    let adult = spawn(&mut h, "minecraft:happy_ghast", 10.5, 10.5);
    let baby = spawn(&mut h, "minecraft:happy_ghast", 20.5, 20.5);
    h.world.entity_mut(baby).insert(GrowAt(u64::MAX));
    // `HappyGhast.getAmbientSound`: the adult's sound, the ghastling's for a baby; the sound is at the creature.
    let mut heard = [false; 2];
    for _ in 0..1200 {
        h.tick();
        for ev in h.drain_host_out() {
            if let HostEvent::Sound { name, at, .. } = ev {
                for (i, (e, sound)) in [(adult, "entity.happy_ghast.ambient"), (baby, "entity.ghastling.ambient")].into_iter().enumerate() {
                    let p = h.world.get::<Pos>(e).unwrap().0;
                    if name == sound && at == [p.x, p.y, p.z] {
                        heard[i] = true;
                    }
                }
            }
        }
    }
    assert_eq!(heard, [true, true], "each ghast plays its ambient where it is");
}

/// A dolphin in the lidded pool, at the water's bottom, its air at `air`.
fn dolphin_in_pool(h: &mut Herd, air: i32) -> Entity {
    lidded_pool(h);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:dolphin").unwrap();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 62);
    h.world.resource_mut::<Entered>().0.push(e);
    h.world.get_mut::<Air>(e).unwrap().0 = air;
    e
}

#[test]
fn a_dolphin_holds_its_breath_under_water() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = dolphin_in_pool(&mut h, 4800);
    for _ in 0..10 {
        h.tick();
    }
    // `Dolphin.getMaxAirSupply` 4800, and `LivingEntity.baseTick` takes one a tick under water.
    assert_eq!(h.world.get::<Air>(e).unwrap().0, 4790);
}

#[test]
fn a_dolphin_refills_to_its_own_maximum() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::effects::{add, Kind};
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = dolphin_in_pool(&mut h, 100);
    assert!(add(&mut h.world, e, Kind::WaterBreathing, 0, 100));
    h.tick();
    // `Dolphin.increaseAirSupply` hands back the whole maximum at once.
    assert_eq!(h.world.get::<Air>(e).unwrap().0, 4800);
}

#[test]
fn noai_dolphins_restore_air_after_the_full_mob_tick() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = dolphin_in_pool(&mut h, 100);
    h.world.entity_mut(e).insert(horizon_ecs_proto::dragon::flight::NoAi);
    h.tick();
    // `Dolphin.tick` with `isNoAi`: the air is set to the maximum and nothing else runs.
    assert_eq!(h.world.get::<Air>(e).unwrap().0, 4800);
}

#[test]
fn normal_dolphins_keep_their_underwater_countdown() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = dolphin_in_pool(&mut h, 4800);
    h.tick();
    assert_eq!(h.world.get::<Air>(e).unwrap().0, 4799);
    h.tick();
    assert_eq!(h.world.get::<Air>(e).unwrap().0, 4798);
}

/// A nautilus at `air`, in the pool (`water`) or on the far side of it, without AI when `no_ai`.
fn nautilus_at(h: &mut Herd, air: i32, water: bool, no_ai: bool) -> Entity {
    lidded_pool(h);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:nautilus").unwrap();
    let x = if water { 0.5 } else { 20.5 };
    let e = spawn_species(&mut h.world, &sp, x, 0.5, 62);
    h.world.resource_mut::<Entered>().0.push(e);
    h.world.get_mut::<Air>(e).unwrap().0 = air;
    if no_ai {
        h.world.entity_mut(e).insert(horizon_ecs_proto::dragon::flight::NoAi);
    }
    e
}

#[test]
fn noai_nautilus_keeps_superclass_refill_in_both_media() {
    if !have_data() {
        return;
    }
    // `Nautilus.baseTick` skips handleAirSupply without AI; the superclass still refills by four.
    for water in [false, true] {
        let mut h = Herd::with_slab_awake(60, true, false);
        let e = nautilus_at(&mut h, 100, water, true);
        h.tick();
        assert_eq!(h.world.get::<Air>(e).unwrap().0, 104);
        h.tick();
        assert_eq!(h.world.get::<Air>(e).unwrap().0, 108);
    }
}

#[test]
fn normal_nautilus_keeps_its_custom_handler() {
    if !have_data() {
        return;
    }
    // With AI, `handleAirSupply` uses the air before the tick on land (less one) and writes the maximum in water.
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = nautilus_at(&mut h, 100, false, false);
    h.tick();
    assert_eq!(h.world.get::<Air>(e).unwrap().0, 99);
    h.tick();
    assert_eq!(h.world.get::<Air>(e).unwrap().0, 98);
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = nautilus_at(&mut h, 100, true, false);
    h.tick();
    assert_eq!(h.world.get::<Air>(e).unwrap().0, 300);
}

#[test]
fn a_zero_duration_takes_its_hidden_part_with_it() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::effects::{add, has, Effects, Kind};
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_at(&mut h, "minecraft:pig", 0.5, 0.5);
    assert!(add(&mut h.world, e, Kind::Speed, 0, 200));
    assert!(add(&mut h.world, e, Kind::Speed, 1, 0));
    h.tick();
    // `MobEffectInstance.tickServer`: a zero duration is dropped before it applies, and never promotes its hidden part.
    assert!(!has(&h.world, e, Kind::Speed));
    assert!(h.world.get::<Effects>(e).is_none_or(|f| f.hidden.is_empty()));
}

#[test]
fn an_infinite_effect_never_ends() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::effects::{add, has, Kind};
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_at(&mut h, "minecraft:pig", 0.5, 0.5);
    assert!(add(&mut h.world, e, Kind::Speed, 0, -1));
    for _ in 0..300 {
        h.tick();
    }
    // `MobEffectInstance.INFINITE_DURATION` (-1) is never counted down.
    assert!(has(&h.world, e, Kind::Speed));
}

/// A mob of `name` at `(x, 62, z)`, in the entered list.
fn spawn_at(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, 62);
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

#[test]
fn a_babys_speed_follows_its_baby_modifier() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::attr_mod::{read, AttrMods, BABY_BONUS};
    use horizon_ecs_proto::spawn_finalize::set_baby;
    // `Zombie.SPEED_MODIFIER_BABY` is +0.5, `Piglin`'s 0.2 widened from a float; both ADD_MULTIPLIED_BASE on the speed. The husk, the drowned,
    // the zombie villager and the zombified piglin extend `Zombie`, so they carry the same bonus.
    for (name, bonus) in [
        ("minecraft:zombie", 0.5),
        ("minecraft:husk", 0.5),
        ("minecraft:drowned", 0.5),
        ("minecraft:zombie_villager", 0.5),
        ("minecraft:zombified_piglin", 0.5),
        ("minecraft:piglin", f64::from(0.2f32)),
    ] {
        let mut h = Herd::with_slab_awake(60, true, false);
        let adult = spawn_at(&mut h, name, 0.5, 0.5);
        let baby = spawn_at(&mut h, name, 2.5, 0.5);
        set_baby(&mut h.world, baby, u64::MAX);
        let ratio = |h: &Herd| read(&h.world, baby, "movement_speed", 1.0) / read(&h.world, adult, "movement_speed", 1.0);
        assert!((ratio(&h) - (1.0 + bonus)).abs() < 1e-9, "{name} baby");
        // A `/attribute ... modifier remove` takes the transient modifier away; `setBaby` brings it back.
        let mut mods = h.world.get_mut::<AttrMods>(baby).unwrap();
        for x in mods.0.iter_mut().filter(|x| x.0 == "movement_speed") {
            x.2.retain(|m| m.id != BABY_BONUS);
        }
        drop(mods);
        assert!((ratio(&h) - 1.0).abs() < 1e-9, "{name} removed");
        set_baby(&mut h.world, baby, u64::MAX);
        assert!((ratio(&h) - (1.0 + bonus)).abs() < 1e-9, "{name} back");
    }
}

#[test]
fn a_hidden_effect_is_saved_inside_its_effect_and_loads_back() {
    if !have_data() {
        return;
    }
    use horizon_core::nbt::{Compound, JavaString, Tag};
    use horizon_ecs_proto::effects::{add, Effects, Kind};
    use horizon_ecs_proto::nbt;
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_at(&mut h, "minecraft:zombie", 0.5, 0.5);
    // A stronger effect shows; the weaker, longer one waits hidden under it (`MobEffectInstance.update`).
    add(&mut h.world, e, Kind::Speed, 1, 100);
    add(&mut h.world, e, Kind::Speed, 0, 400);
    let hidden = h.world.get::<Effects>(e).unwrap().hidden[0];
    let saved = nbt::get(&h.world, e, "active_effects").expect("active_effects");
    let Tag::List(items) = &saved else { panic!("a list") };
    let Tag::Compound(top) = &items[0] else { panic!("a compound") };
    // `MobEffectInstance.Details`: the hidden effect is the `hidden_effect` compound inside the effect.
    let Some(Tag::Compound(inner)) = top.get(&JavaString::from("hidden_effect")) else { panic!("a hidden_effect") };
    assert!(matches!(inner.get(&JavaString::from("id")), Some(Tag::String(s)) if s.to_string_lossy() == "minecraft:speed"));
    assert!(inner.get(&JavaString::from("amplifier")).is_none(), "amplifier 0 is omitted");
    let g = spawn_at(&mut h, "minecraft:zombie", 2.5, 0.5);
    let mut c = Compound::new();
    c.insert("active_effects".into(), saved.clone());
    nbt::load(&mut h.world, g, &c);
    let back = h.world.get::<Effects>(g).unwrap();
    assert_eq!(back.list.len(), 1);
    assert_eq!(back.hidden.len(), 1, "the hidden effect is read back");
    assert_eq!((back.hidden[0].kind, back.hidden[0].amp, back.hidden[0].ends), (hidden.kind, hidden.amp, hidden.ends));
}

#[test]
fn a_wandering_trader_goes_when_its_delay_runs_out() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::despawn_delay::{self, DespawnDelay};
    let mut h = Herd::with_slab_awake(60, true, false);
    let trader = spawn_at(&mut h, "minecraft:wandering_trader", 0.5, 0.5);
    let now = now_of(&h.world, trader);
    despawn_delay::start(&mut h.world, trader);
    // `WanderingTraderSpawner`: 48000 ticks from its spawn.
    assert_eq!(h.world.get::<DespawnDelay>(trader).unwrap().due, now + 48000);
    // A short delay: the trader is gone at its due, on the wheel.
    despawn_delay::restore(&mut h.world, trader, 3);
    for _ in 0..4 {
        h.tick();
    }
    assert!(h.world.get_entity(trader).is_err(), "the trader goes at its due");
}

#[test]
fn a_trader_pauses_its_count_while_it_trades() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::despawn_delay::{self, DespawnDelay};
    let mut h = Herd::with_slab_awake(60, true, false);
    let trader = spawn_at(&mut h, "minecraft:wandering_trader", 0.5, 0.5);
    let since = now_of(&h.world, trader);
    despawn_delay::start(&mut h.world, trader);
    let before = h.world.get::<DespawnDelay>(trader).unwrap().due;
    for _ in 0..5 {
        h.tick();
    }
    // The trade took five ticks: its count did not run, so the deadline moves by five.
    despawn_delay::trade_ended(&mut h.world, trader, since);
    assert_eq!(h.world.get::<DespawnDelay>(trader).unwrap().due, before + 5);
}

#[test]
fn a_llama_leashed_to_a_trader_goes_one_tick_before_it() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::despawn_delay;
    use horizon_ecs_proto::leash::{attach_from_save, Holder};
    let mut h = Herd::with_slab_awake(60, true, false);
    let trader = spawn_at(&mut h, "minecraft:wandering_trader", 0.5, 0.5);
    let llama = spawn_at(&mut h, "minecraft:trader_llama", 2.5, 0.5);
    assert!(attach_from_save(&mut h.world, llama, Holder::Entity(trader)), "the llama is leashed to the trader");
    despawn_delay::restore(&mut h.world, trader, 3);
    h.tick();
    h.tick();
    // `TraderLlama.maybeDespawn`: the trader's delay minus one reaches zero two ticks from now; the trader is still there.
    assert!(h.world.get_entity(llama).is_err(), "the llama goes first");
    assert!(h.world.get_entity(trader).is_ok(), "the trader stays one more tick");
}

#[test]
fn a_llama_on_its_own_keeps_its_own_delay() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::despawn_delay::{self, DespawnDelay};
    let mut h = Herd::with_slab_awake(60, true, false);
    let llama = spawn_at(&mut h, "minecraft:trader_llama", 2.5, 0.5);
    let now = now_of(&h.world, llama);
    // `TraderLlama.DEFAULT_DESPAWN_DELAY`: 47999 ticks from its spawn, posed by its species data at the birth.
    assert_eq!(h.world.get::<DespawnDelay>(llama).unwrap().due, now + 47999);
    despawn_delay::restore(&mut h.world, llama, 2);
    for _ in 0..3 {
        h.tick();
    }
    assert!(h.world.get_entity(llama).is_err(), "the llama goes at its own delay");
}

#[test]
fn a_nautilus_with_its_eye_above_the_water_breathes_in_as_long_as_its_body_is_in() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::cow::Pos;
    use horizon_core::geometry::Vec3;
    // `Nautilus.handleAirSupply` tests `isInWater`: the pool's top is at y 2.89, the feet at 2.6 are in it, the eye (about 0.8 up) is out.
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = nautilus_at(&mut h, 100, true, false);
    h.set_block(0, 3, 0, "minecraft:air");
    h.world.get_mut::<Pos>(e).unwrap().0 = Vec3::new(0.5, 2.6, 0.5);
    h.tick();
    assert_eq!(h.world.get::<Air>(e).unwrap().0, 300);
}

#[test]
fn a_summoned_trader_and_llama_get_their_delay_from_the_species_data() {
    if !have_data() {
        return;
    }
    use horizon_ecs_proto::despawn_delay::DespawnDelay;
    // The birth (`spawn_species`) reads `despawn_delay` of the species: the llama gets its 47999, the trader none (the spawner sets its 48000).
    let mut h = Herd::with_slab_awake(60, true, false);
    let trader = spawn_at(&mut h, "minecraft:wandering_trader", 0.5, 0.5);
    let llama = spawn_at(&mut h, "minecraft:trader_llama", 2.5, 0.5);
    assert!(h.world.get::<DespawnDelay>(trader).is_none(), "a summoned trader has no delay until the spawner sets it");
    assert_eq!(h.world.get::<DespawnDelay>(llama).unwrap().due, now_of(&h.world, llama) + 47999);
}

// @@END@@
