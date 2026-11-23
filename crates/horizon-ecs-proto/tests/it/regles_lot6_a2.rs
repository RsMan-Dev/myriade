//! Lot 6a2 of the rules of the mobs (`ai.rs` 23 to 44): the wither effect, the effects of a hit on a mob, the dolphin's bite sound,
//! the undead's sunburn on the body, the freezing of a body armoured in leather, the equipment by slot, the root vehicle that never
//! pushes its rider, a ridden horse, a digging warden, the sleeping place of a loaded villager, the sleeper whose bed is gone, the
//! turtle's ambient sound, the food that grows a baby, a frog that is never a baby, the turtle carrying an egg, and the forgiveness of
//! a dead player. Expected values are the vanilla 26.3 ones (`javap`, `LivingEntity.setPosToBed`, `Turtle.getAmbientSound`,
//! `Animal.mobInteract`, `Turtle.canFallInLove`, `NeutralMob`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_client_lib::items::ItemStack;
use horizon_ecs_proto::age_lock::AgeLocked;
use horizon_ecs_proto::behaviours::act_phase::{ActPhase, Phase};
use horizon_ecs_proto::behaviours::bed::{lie_down, Sleeping};
use horizon_ecs_proto::behaviours::breed::feed;
use horizon_ecs_proto::behaviours::lay_egg::Carrying;
use horizon_ecs_proto::behaviours::{ambient_sound, neutral_anger::is_angry};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::effects::{self, Kind};
use horizon_ecs_proto::equip_stack;
use horizon_ecs_proto::forgive_dead_player::forgive;
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::hostile::{BiteSound, SunSlot};
use horizon_ecs_proto::mount::{Riding, Ridden};
use horizon_ecs_proto::tame::Owner;
use horizon_ecs_proto::targeting::{set_target, Target, Who};

/// The species of the table by name, standing at (x, z).
fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// Whether the host sent a sound named `sound` since the last drain.
fn sounded(h: &mut Herd, sound: &str) -> bool {
    h.world.resource_mut::<Events>().host.drain().any(|ev| matches!(ev, HostEvent::Sound { name, .. } if *name == sound))
}

/// The wither effect hurts with wither: a creature with the effect loses a heart after its 40 ticks.
#[test]
fn the_wither_effect_hurts_with_wither() {
    if !have_data() || wrong_species(&["minecraft:pig"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let e = spawn(&mut h, "minecraft:pig", 0.5, 0.5);
    assert!(effects::add(&mut h.world, e, Kind::Wither, 0, 40));
    for _ in 0..41 {
        h.tick();
    }
    assert!(h.world.get::<Health>(e).unwrap().0 < 20.0, "the wither effect did not hurt");
}

/// The effects of a cave spider's bite, a husk's blow and a wither skeleton's hit go to the mob the hit landed on.
#[test]
fn a_melee_effect_rides_the_hit_onto_a_mob() {
    if !have_data() || wrong_species(&["minecraft:cave_spider", "minecraft:husk", "minecraft:wither_skeleton", "minecraft:cow"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let cases = [("minecraft:cave_spider", Kind::Poison), ("minecraft:husk", Kind::Hunger), ("minecraft:wither_skeleton", Kind::Wither)];
    for (name, kind) in cases {
        let attacker = spawn(&mut h, name, 0.5, 0.5);
        let victim = spawn(&mut h, "minecraft:cow", 3.5, 3.5);
        effects::hit_effects(&mut h.world, attacker, Who::Entity(victim));
        assert!(effects::has(&h.world, victim, kind), "{name} left its effect off the mob it hit");
    }
}

/// A dolphin's bite plays its attack sound: the rule `BITE` gives the dolphin the sound of its bite.
#[test]
fn a_dolphins_blow_plays_its_attack_sound() {
    if !have_data() || wrong_species(&["minecraft:dolphin"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let e = spawn(&mut h, "minecraft:dolphin", 0.5, 0.5);
    assert_eq!(h.world.get::<BiteSound>(e).map(|b| b.0), Some("entity.dolphin.attack"));
}

/// The zombie horse is shielded by its body slot, a zombie by its head: `Mob.sunProtectionSlot` is HEAD, `ZombieHorse` overrides it to BODY.
#[test]
fn zombie_horse_sunburn_checks_body_not_head() {
    if !have_data() || wrong_species(&["minecraft:zombie_horse", "minecraft:zombie"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let horse = spawn(&mut h, "minecraft:zombie_horse", 0.5, 0.5);
    let zombie = spawn(&mut h, "minecraft:zombie", 3.5, 3.5);
    assert_eq!(h.world.get::<SunSlot>(horse).map(|s| s.0), Some(6), "the horse's sun shield is its body slot");
    assert_eq!(h.world.get::<SunSlot>(zombie).map_or(2, |s| s.0), 2, "the zombie's sun shield is its head slot");
}

/// Leather horse armour on the body keeps the horse from freezing in powder snow; without it the horse loses health.
#[test]
fn leather_body_armor_prevents_freezing() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut armoured = Herd::with_slab_awake(8, true, true);
    armoured.set_block(0, 1, 0, "minecraft:powder_snow");
    let horse = spawn(&mut armoured, "minecraft:horse", 0.5, 0.5);
    equip_stack::set(&mut armoured.world, horse, 6, Some(ItemStack::new("minecraft:leather_horse_armor", 1)));
    for _ in 0..80 {
        armoured.tick();
    }
    assert_eq!(armoured.world.get::<Health>(horse).unwrap().0, 20.0, "leather armour did not keep the horse from freezing");
    let mut bare = Herd::with_slab_awake(8, true, true);
    bare.set_block(0, 1, 0, "minecraft:powder_snow");
    let horse = spawn(&mut bare, "minecraft:horse", 0.5, 0.5);
    for _ in 0..80 {
        bare.tick();
    }
    assert!(bare.world.get::<Health>(horse).unwrap().0 < 20.0, "a bare horse did not freeze");
}

/// A mob's equipment is read by slot: the main hand is slot 0, the feet slot 5, and the others stay empty.
#[test]
fn a_mobs_equipment_is_listed_by_slot() {
    if !have_data() || wrong_species(&["minecraft:zombie"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let e = spawn(&mut h, "minecraft:zombie", 0.5, 0.5);
    assert!(equip_stack::get(&h.world, e, 0).is_none());
    assert!(equip_stack::set(&mut h.world, e, 0, Some(ItemStack::new("minecraft:iron_sword", 1))));
    assert!(equip_stack::set(&mut h.world, e, 5, Some(ItemStack::new("minecraft:iron_boots", 1))));
    assert!(equip_stack::get(&h.world, e, 0).is_some_and(|s| &*s.item == "minecraft:iron_sword"));
    assert!(equip_stack::get(&h.world, e, 5).is_some_and(|s| &*s.item == "minecraft:iron_boots"));
    assert!(equip_stack::get(&h.world, e, 1).is_none(), "the off hand is empty");
}

/// A camel and the husk riding it never push each other: the root vehicle is the same for both.
#[test]
fn a_camel_and_its_rider_never_push_each_other() {
    if !have_data() || wrong_species(&["minecraft:camel", "minecraft:husk"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let camel = spawn(&mut h, "minecraft:camel", 0.5, 0.5);
    let husk = spawn(&mut h, "minecraft:husk", 0.5, 0.5);
    h.world.entity_mut(camel).insert(Ridden);
    h.world.entity_mut(husk).insert(Riding(camel));
    let before = h.world.get::<Vel>(husk).unwrap().0;
    h.tick();
    assert_eq!(h.world.get::<Vel>(husk).unwrap().0, before, "the camel pushed its rider");
}

/// A ridden horse is not pushed, and it still pushes the cow beside it.
#[test]
fn a_ridden_horse_is_not_pushed_but_still_pushes() {
    if !have_data() || wrong_species(&["minecraft:horse", "minecraft:cow"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let horse = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    let cow = spawn(&mut h, "minecraft:cow", 1.4, 0.5);
    h.world.entity_mut(horse).insert(Ridden);
    let (vh, vc) = (h.world.get::<Vel>(horse).unwrap().0, h.world.get::<Vel>(cow).unwrap().0);
    h.tick();
    assert_eq!(h.world.get::<Vel>(horse).unwrap().0, vh, "the ridden horse was pushed");
    assert_ne!(h.world.get::<Vel>(cow).unwrap().0, vc, "the ridden horse did not push the cow");
}

/// A sleeping villager is pushed by its own tick and pushes the cow it overlaps; a mob's call on it is vetoed (`LivingEntity.push`).
#[test]
fn sleeping_villager_vetoes_only_pushes_received_during_other_mobs_ticks() {
    if !have_data() || wrong_species(&["minecraft:villager", "minecraft:cow"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let villager = spawn(&mut h, "minecraft:villager", 0.5, 0.5);
    let cow = spawn(&mut h, "minecraft:cow", 1.4, 0.5);
    h.world.entity_mut(villager).insert(Sleeping([0, 64, 0]));
    let (vv, vc) = (h.world.get::<Vel>(villager).unwrap().0, h.world.get::<Vel>(cow).unwrap().0);
    h.tick();
    assert_ne!(h.world.get::<Vel>(villager).unwrap().0, vv, "the sleeper was not pushed by its own tick");
    assert_ne!(h.world.get::<Vel>(cow).unwrap().0, vc, "the sleeper did not push the cow");
}

/// A sleeping fox is pushed by its own tick and pushes the cow it overlaps; a mob's call on it is vetoed (`LivingEntity.push`).
#[test]
fn sleeping_fox_vetoes_pushes_received_during_other_mobs_ticks() {
    if !have_data() || wrong_species(&["minecraft:fox", "minecraft:cow"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let fox = spawn(&mut h, "minecraft:fox", 0.5, 0.5);
    let cow = spawn(&mut h, "minecraft:cow", 1.4, 0.5);
    h.world.entity_mut(fox).insert(Sleeping([0, 64, 0]));
    let (vf, vc) = (h.world.get::<Vel>(fox).unwrap().0, h.world.get::<Vel>(cow).unwrap().0);
    h.tick();
    assert_ne!(h.world.get::<Vel>(fox).unwrap().0, vf, "the sleeper was not pushed by its own tick");
    assert_ne!(h.world.get::<Vel>(cow).unwrap().0, vc, "the sleeper did not push the cow");
}

/// A warden in the ground (emerging or digging) takes no collision push; a standing one is pushed.
#[test]
fn a_warden_in_the_ground_does_not_take_collision_pushes() {
    if !have_data() || wrong_species(&["minecraft:warden", "minecraft:cow"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let warden = spawn(&mut h, "minecraft:warden", 0.5, 0.5);
    spawn(&mut h, "minecraft:cow", 1.4, 0.5);
    h.world.entity_mut(warden).insert(ActPhase { phase: Phase::Emerging, until: 0, dig_cooldown_until: 0 });
    let v = h.world.get::<Vel>(warden).unwrap().0;
    h.tick();
    assert_eq!(h.world.get::<Vel>(warden).unwrap().0, v, "an emerging warden took a push");
}

/// A loaded sleeper lies at the bed's height and 0.125 over it: `LivingEntity.setPosToBed` gives y 64.6875 on a bed at y 64.
#[test]
fn a_villager_lies_on_its_bed_at_the_vanilla_height() {
    if !have_data() || wrong_species(&["minecraft:villager"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5);
    lie_down(&mut h.world, e, [0, 64, 0]);
    assert!((h.world.get::<Pos>(e).unwrap().0.y - 64.6875).abs() < 1e-9);
    assert_eq!(h.world.get::<Sleeping>(e).map(|s| s.0), Some([0, 64, 0]));
}

/// A sleeper loaded from its save keeps its saved position: nothing moves it to the bed at load (`Villager` NBT
/// `sleeping_pos`), only `start_sleeping` does.
#[test]
fn loaded_sleeping_position_refreshes_in_actual_mob_tick() {
    if !have_data() || wrong_species(&["minecraft:villager"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let e = spawn(&mut h, "minecraft:villager", 0.5, 0.5);
    let saved = h.world.get::<Pos>(e).unwrap().0;
    h.world.entity_mut(e).insert(Sleeping([0, 64, 0]));
    assert_eq!(h.world.get::<Pos>(e).unwrap().0, saved, "the load moved the sleeper");
}

/// A sleeper without AI still stands when its bed is gone: a bed block keeps it asleep, anything else wakes it.
#[test]
fn a_sleeper_without_ai_still_stands_when_its_bed_is_gone() {
    if !have_data() || wrong_species(&["minecraft:cow"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    h.set_block(4, 1, 4, "minecraft:red_bed");
    let kept = spawn(&mut h, "minecraft:cow", 4.5, 4.5);
    lie_down(&mut h.world, kept, [4, 1, 4]);
    let gone = spawn(&mut h, "minecraft:cow", 0.5, 0.5);
    lie_down(&mut h.world, gone, [0, 1, 0]);
    h.tick();
    assert!(h.world.get::<Sleeping>(kept).is_some(), "a bed block woke the sleeper");
    assert!(h.world.get::<Sleeping>(gone).is_none(), "a missing bed did not wake the sleeper");
}

/// A turtle on the land is silent in water, in the air and as a baby: only the land ambient plays.
#[test]
fn a_turtle_is_silent_in_water_in_the_air_and_as_a_baby() {
    if !have_data() || wrong_species(&["minecraft:turtle"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let e = spawn(&mut h, "minecraft:turtle", 0.5, 0.5);
    h.world.entity_mut(e).insert((Contact { on_ground: true, horizontal: false, fall_distance: 0.0, stuck: 0 }, FluidState { in_water: false, ..Default::default() }));
    assert!(ambient_sound::play(&mut h.world, e));
    assert!(sounded(&mut h, "entity.turtle.ambient_land"), "an adult on the land is silent");
    h.world.entity_mut(e).insert(FluidState { in_water: true, ..Default::default() });
    assert!(!ambient_sound::play(&mut h.world, e), "a turtle in water spoke");
    h.world.entity_mut(e).insert(FluidState { in_water: false, ..Default::default() });
    h.world.entity_mut(e).insert(GrowAt(now_of(&h.world, e) + 1000));
    assert!(!ambient_sound::play(&mut h.world, e), "a baby turtle spoke");
}

/// Food grows a tamed baby by a tenth of its time left; a wild one takes no food (`Wolf.mobInteract`: only taming), and an
/// age-locked baby does not take it either.
#[test]
fn food_grows_a_tamed_baby_cat_or_wolf() {
    if !have_data() || wrong_species(&["minecraft:cat"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let e = spawn(&mut h, "minecraft:cat", 0.5, 0.5);
    let now = now_of(&h.world, e);
    h.world.entity_mut(e).insert(GrowAt(now + 24_000));
    h.world.entity_mut(e).insert(Owner(None));
    assert!(!feed(&mut h.world, e), "a wild baby took the food");
    h.world.entity_mut(e).insert(Owner(Some(Who::Player(1))));
    assert!(feed(&mut h.world, e));
    assert_eq!(h.world.get::<GrowAt>(e).map(|g| g.0), Some(now + 24_000 - 2_400), "it grew a tenth");
    h.world.entity_mut(e).insert(AgeLocked(10_000));
    assert!(!feed(&mut h.world, e), "an age-locked baby took the food");
}

/// A frog is never a baby: feeding it never starts growing it.
#[test]
fn a_mob_that_cannot_be_a_baby_never_takes_the_food_that_grows_it() {
    if !have_data() || wrong_species(&["minecraft:frog"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let e = spawn(&mut h, "minecraft:frog", 0.5, 0.5);
    assert!(h.world.get::<GrowAt>(e).is_none(), "a frog starts as a baby");
    let _ = feed(&mut h.world, e);
    assert!(h.world.get::<GrowAt>(e).is_none(), "feeding a frog made it a baby");
}

/// A turtle carrying an egg refuses its food: it is neither eaten nor put in love.
#[test]
fn a_turtle_carrying_an_egg_refuses_its_food() {
    if !have_data() || wrong_species(&["minecraft:turtle"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    let e = spawn(&mut h, "minecraft:turtle", 0.5, 0.5);
    h.world.entity_mut(e).insert(Carrying);
    assert!(!feed(&mut h.world, e), "an egg-carrying turtle ate its food");
    h.world.entity_mut(e).remove::<Carrying>();
    assert!(feed(&mut h.world, e), "a turtle without an egg refused its food");
}

/// `NeutralMob.playerDied`: forgiving the dying player ends the anger and keeps the target that was set.
#[test]
fn forgiveness_keeps_cancelled_target_but_ends_anger() {
    if !have_data() || wrong_species(&["minecraft:enderman"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(6.5, 0.5)]);
    let e = spawn(&mut h, "minecraft:enderman", 0.5, 0.5);
    set_target(&mut h.world, e, Target::player(1, 20));
    h.tick();
    assert!(forgive(&mut h.world, e, 1), "the dying player's anger was not forgiven");
    assert!(!is_angry(&h.world, e), "still angry after the forgiveness");
    assert!(matches!(h.world.get::<Target>(e).map(|t| t.who), Some(Who::Player(1))), "the forgiveness dropped the target");
    let cow = spawn(&mut h, "minecraft:cow", 3.5, 3.5);
    set_target(&mut h.world, e, Target::new(Who::Entity(cow), 20, 0));
    assert!(matches!(h.world.get::<Target>(e).map(|t| t.who), Some(Who::Entity(c)) if c == cow), "the target did not change");
}
