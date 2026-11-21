//! The clients of the capabilities (status effects, teleport) wired by rules and data: the blow that poisons, the hunger a husk gives a player,
//! the undead that poison does not hurt (against the `Mob`), the invisible player a monster does not see, the enderman that leaves the water.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    cow::*,
    effects::{self, Kind},
    goals::*,
    rules::RuleEntry,
    targeting::{set_target, Target, Who},
};

fn species_named(h: &Herd, name: &str) -> Species {
    *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).unwrap()
}
fn spawn(h: &mut Herd, sp: &Species, x: f64, z: f64, seed: u64) -> Entity {
    let e = spawn_species(&mut h.world, sp, x, z, seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

/// The rule `on_hit_effects` end to end: a zombie that carries the cave spider's poison hits a cow, which is poisoned; and a player, who gets the list.
#[test]
fn a_blow_that_poisons_poisons_a_creature_and_a_player() {
    if !have_data() {
        return;
    }
    static RULES: [RuleEntry; 1] = [RuleEntry::new(&effects::ON_HIT, &effects::CAVE_SPIDER)];
    let mut h = Herd::with_slab_awake(40, false, false);
    set_lazy_look(&mut h.world, false);
    let mut z = species_named(&h, "minecraft:zombie");
    z.rules = &RULES;
    let mut cow = species_named(&h, "minecraft:cow");
    cow.goals = horizon_ecs_proto::species::Config { entries: &[] };
    let (zombie, prey) = (spawn(&mut h, &z, 0.5, 0.5, 1), spawn(&mut h, &cow, 1.4, 0.5, 2));
    h.tick();
    set_target(&mut h.world, zombie, Target::new(Who::Entity(prey), 200, u8::MAX));
    let mut poisoned_at = None;
    for t in 0..120 {
        h.tick();
        if poisoned_at.is_none() && effects::has(&h.world, prey, Kind::Poison) {
            poisoned_at = Some(t);
        }
    }
    eprintln!("cow poisoned at tick {poisoned_at:?}, health {}", health(&h, prey));
    assert!(poisoned_at.is_some());
    // The player: the husk's hunger lands in the list of the player, and is read until it ends.
    effects::add_player(&mut h.world, 1, Kind::Hunger, 0, 140);
    assert!(effects::player_has(&h.world, 1, Kind::Hunger));
    for _ in 0..141 {
        h.tick();
    }
    assert!(!effects::player_has(&h.world, 1, Kind::Hunger));
}

/// Poison and regeneration do nothing to the undead, as the `Mob`'s.
#[test]
fn poison_does_not_touch_the_undead_as_the_mob_does() {
    if !have_data() {
        return;
    }
    let mut r = MobRef::new(40, false);
    let i = r.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:zombie"));
    r.mobs[i].add_effect("minecraft:poison", 100, 0);
    let mut h = Herd::with_slab_awake(40, false, false);
    let z = species_named(&h, "minecraft:zombie");
    let e = spawn(&mut h, &z, 0.5, 0.5, 3);
    h.world.get_mut::<horizon_ecs_proto::cow::Pos>(e).unwrap();
    h.tick();
    effects::add(&mut h.world, e, Kind::Poison, 0, 100);
    let hp = health(&h, e);
    for _ in 0..110 {
        r.step(None);
        h.tick();
    }
    eprintln!("zombie health: mob {} ecs {} (was {hp})", mob_health(&r.mobs[i]), health(&h, e));
    assert_eq!(mob_health(&r.mobs[i]), health(&h, e));
}

/// An invisible player is seen from 7 % of the range: a zombie 10 blocks away does not take it as a target, a visible one does.
#[test]
fn an_invisible_player_is_not_seen_from_afar() {
    if !have_data() {
        return;
    }
    let seen = |invisible: bool| {
        let mut h = Herd::with_slab_awake(40, false, false);
        set_lazy_look(&mut h.world, false);
        h.set_online(&[(10.5, 0.5)]);
        let z = species_named(&h, "minecraft:zombie");
        let e = spawn(&mut h, &z, 0.5, 0.5, 4);
        if invisible {
            effects::add_player(&mut h.world, 1, Kind::Invisibility, 0, 6000);
        }
        for _ in 0..200 {
            h.tick();
        }
        h.world.get::<Target>(e).is_some()
    };
    assert!(seen(false), "the control: a visible player is taken");
    assert!(!seen(true), "an invisible one is not");
}

/// The enderman in the water is hurt and leaves it.
#[test]
fn an_enderman_leaves_the_water() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(40, false, false);
    set_lazy_look(&mut h.world, false);
    for x in 18..=22 {
        for z in 18..=22 {
            h.set_block(x, 0, z, "minecraft:water");
        }
    }
    h.tick();
    let end = species_named(&h, "minecraft:enderman");
    let e = spawn(&mut h, &end, 20.5, 20.5, 5);
    h.world.get_mut::<horizon_ecs_proto::cow::Pos>(e).unwrap().0.y = 0.0;
    let hp = health(&h, e);
    let mut left_at = None;
    for t in 0..200 {
        h.tick();
        let p = h.world.get::<horizon_ecs_proto::cow::Pos>(e).map(|p| p.0);
        if left_at.is_none() && p.is_some_and(|p| (p.x - 20.5).abs() > 4.0 || (p.z - 20.5).abs() > 4.0) {
            left_at = Some(t);
        }
    }
    eprintln!("enderman left the pool at tick {left_at:?}, health {hp} -> {}", health(&h, e));
    assert!(left_at.is_some() && health(&h, e) < hp);
}
