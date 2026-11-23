//! Lot 13, the equines of the old Mob (`equine.rs`): steering by a saddled rider, the temper (taming, refusal, clamp), the rolled
//! health, the hurt rear (one blow in three, 20 ticks), the llama that never rears, the bolt and its buck, the bred foal that
//! follows its mother and the skeleton horse's water sounds. Expected values are the old Mob's tests and vanilla 26.3
//! (`AbstractHorse.modifyTemper`, `hurtServer`, `makeMad`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::follow_mommy::{Bred, EatingHaystack};
use horizon_ecs_proto::behaviours::random_stand::Standing;
use horizon_ecs_proto::behaviours::run_around_like_crazy::Crazy;
use horizon_ecs_proto::cow::health::{Health, InvulnerableUntil, MaxHealth};
use horizon_ecs_proto::behaviours::elder_curse::Silent;
use horizon_ecs_proto::behaviours::swim_sound::SwimSounds;
use horizon_ecs_proto::cow::{spawn_species, Clock, Herd, Pos, SpeciesTable, Stroll};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::mount::{self, Mount};
use horizon_ecs_proto::move_events::MoveEventsOn;
use horizon_ecs_proto::pilot::{self, PilotInput};
use horizon_ecs_proto::targeting::Who;

fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

fn forward(f: f32) -> PilotInput {
    PilotInput { who: Who::Player(1), forward: f, strafe: 0.0, yaw: 0.0, pitch: 0.0, jump: 0.0, sprint: false, sneak: false, steer: false, keys: [false; 4] }
}

/// `AbstractHorse.setStanding`: the rear is on while the clock has not reached its end.
fn rearing(h: &Herd, e: Entity) -> bool {
    let now = h.world.resource::<Clock>().0;
    h.world.get::<Standing>(e).is_some_and(|s| now < s.until)
}

/// `AbstractHorse.isSaddled` steers the rider: a saddled horse walks forward under a player's forward input, an unsaddled one does not.
#[test]
fn only_an_abstract_horse_is_steered_by_its_saddled_player_rider() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let saddled = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    let bare = spawn(&mut h, "minecraft:horse", 8.5, 0.5);
    h.world.get_mut::<Mount>(saddled).unwrap().saddled = true;
    for e in [saddled, bare] {
        assert!(mount::ride(&mut h.world, e, Who::Player(1)), "the first seat is free");
        pilot::set(&mut h.world, e, forward(1.0));
    }
    let at = |w: &World, e: Entity| {
        let p = w.get::<Pos>(e).unwrap().0;
        (p.x, p.z)
    };
    let (a0, b0) = (at(&h.world, saddled), at(&h.world, bare));
    for _ in 0..40 {
        h.tick();
    }
    let moved = |w: &World, e: Entity, from: (f64, f64)| {
        let (x, z) = at(w, e);
        ((x - from.0).powi(2) + (z - from.1).powi(2)).sqrt()
    };
    assert!(moved(&h.world, saddled, a0) > 1.0, "the saddled horse follows its rider");
    assert!(moved(&h.world, bare, b0) < 0.1, "an unsaddled horse is not steered");
}

/// The temper that is already loaded reaches the taming: a horse with 1000 temper (over its 100 highest) is tamed by its
/// player rider on the first roll that lets it, and the temper stays as it was (`Mount.temper` is kept).
#[test]
fn temper_preservation_reaches_the_existing_taming_consumer() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    h.world.get_mut::<Mount>(e).unwrap().temper = 1000;
    assert!(mount::ride(&mut h.world, e, Who::Player(1)), "the first seat is free");
    let mut tamed = false;
    for _ in 0..2000 {
        h.tick();
        if h.world.get::<horizon_ecs_proto::tame::Owner>(e).is_some_and(|o| o.0.is_some()) {
            tamed = true;
            break;
        }
    }
    assert!(tamed, "a horse loaded with temper is tamed by its rider");
    assert_eq!(h.world.get::<Mount>(e).unwrap().temper, 1000, "the temper is kept as it was");
}

/// `AbstractHorse.modifyTemper`: the sum wraps as a Java int, then holds between 0 and the highest temper (100 for a horse).
#[test]
fn temper_preservation_modification_keeps_limits_and_java_overflow() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    let cases = [(-7, 5, 0), (99, 5, 100), (1000, 5, 100), (i32::MAX, 5, 0), (i32::MIN, -1, 100)];
    for (value, amount, expected) in cases {
        h.world.get_mut::<Mount>(e).unwrap().temper = value;
        mount::modify_temper(&mut h.world, e, amount);
        assert_eq!(h.world.get::<Mount>(e).unwrap().temper, expected, "temper {value} + {amount}");
    }
}

/// A refused rider (`RunAroundLikeCrazyGoal`, a player rider the temper does not tame): the temper rises by 5 (20 becomes 25, the
/// old Mob's value), the rider is thrown, and the horse rears.
#[test]
fn a_refused_rider_raises_the_temper_and_is_thrown() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    h.world.get_mut::<Mount>(e).unwrap().temper = 20;
    assert!(mount::ride(&mut h.world, e, Who::Player(1)), "the first seat is free");
    let mut thrown = false;
    for _ in 0..5000 {
        h.tick();
        if mount::rider_of(&h.world, e).is_none() {
            thrown = true;
            break;
        }
    }
    assert!(thrown, "the untamed horse throws its rider");
    assert_eq!(h.world.get::<Mount>(e).unwrap().temper, 25, "a refusal raises the temper by 5");
    assert!(rearing(&h, e), "the refusal makes the horse rear");
}

/// `AbstractHorse` rolled health: a horse's max health is drawn between 15 and 30, and its health starts full. Its speed and its jump
/// are drawn too (`horse_attrs`: speed `(0.45 + 0.3 * sum) * 0.25` below 0.3375, jump `0.4 + 0.2 * sum` below 1.0).
#[test]
fn rolled_health_is_the_attribute_base() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    let max = h.world.get::<MaxHealth>(e).unwrap().0;
    assert!((15.0..=30.0).contains(&max), "the rolled max health {max}");
    assert_eq!(h.world.get::<Health>(e).unwrap().0, max, "a new horse starts at its max health");
    let speed = horizon_ecs_proto::attr_mod::read(&h.world, e, "movement_speed", 0.0);
    assert!((0.1125..0.3375).contains(&speed), "the rolled speed {speed}");
    let jump = horizon_ecs_proto::attr_mod::read(&h.world, e, "jump_strength", 0.0);
    assert!((0.4..1.0).contains(&jump), "the rolled jump {jump}");
}

/// `AbstractHorse.hurtServer`: one blow in three (`nextInt(3) == 0`) makes the horse rear, so 300 blows give 60 to 140 rears.
#[test]
fn a_hurt_horse_rears_one_hit_in_three() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    let mut rears = 0;
    for _ in 0..300 {
        h.world.entity_mut(e).remove::<Standing>();
        h.world.entity_mut(e).remove::<InvulnerableUntil>();
        h.hit(e, 1.0, (0.5, 0.5));
        h.tick();
        if h.world.get::<Standing>(e).is_some() {
            rears += 1;
        }
    }
    assert!((60..=140).contains(&rears), "rears {rears} of 300 blows");
}

/// `AbstractHorse.standIfPossible`: the rear lasts 20 ticks. The blow is queued by `Herd::hit` and applied by the tick that sets the
/// rear (`until` = the clock then + 20). Counted are the ticks that end with the horse still rearing: that tick and the 19 after it.
#[test]
fn the_rear_ends_after_twenty_ticks() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    let mut hit = false;
    for _ in 0..300 {
        h.world.entity_mut(e).remove::<Standing>();
        h.world.entity_mut(e).remove::<InvulnerableUntil>();
        h.hit(e, 1.0, (0.5, 0.5));
        h.tick();
        if h.world.get::<Standing>(e).is_some() {
            hit = true;
            break;
        }
    }
    assert!(hit, "a blow makes the horse rear within 300 blows");
    let mut ticks = 1;
    while rearing(&h, e) {
        h.tick();
        if rearing(&h, e) {
            ticks += 1;
        }
    }
    assert_eq!(ticks, 20, "the rear is 20 ticks long");
}

/// `Llama` has no rearing (no `Stand` cycle, its `makeMad` gives no rear): 60 blows give no rear at all.
#[test]
fn llamas_and_camels_never_rear_when_hurt() {
    if !have_data() || wrong_species(&["minecraft:llama"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:llama", 0.5, 0.5);
    for _ in 0..60 {
        h.world.entity_mut(e).remove::<Standing>();
        h.world.entity_mut(e).remove::<InvulnerableUntil>();
        h.hit(e, 1.0, (0.5, 0.5));
        h.tick();
        assert!(h.world.get::<Standing>(e).is_none(), "a llama reared");
    }
}

/// A horse ridden by an untamed mount's rider bolts: `RunAroundLikeCrazyGoal` starts on the mounting, and the goal's flag is on.
#[test]
fn a_mob_riding_an_untamed_horse_starts_the_bolt() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    assert!(mount::ride(&mut h.world, e, Who::Player(1)), "the first seat is free");
    h.tick();
    assert!(h.world.get::<Crazy>(e).is_some(), "an untamed horse with a rider bolts");
    let zombie = spawn(&mut h, "minecraft:zombie_horse", 8.5, 0.5);
    let rider = spawn(&mut h, "minecraft:zombie", 8.5, 0.5);
    assert!(mount::ride(&mut h.world, zombie, Who::Entity(rider)), "the first seat is free");
    h.tick();
    assert!(h.world.get::<Crazy>(zombie).is_some(), "an untamed zombie horse with a mob rider bolts");
}

/// `RunAroundLikeCrazyGoal.tick`: the buck rolls `nextInt(reducedTickDelay(50))` = 1 in 25 on each full update (every second tick),
/// so a bolting horse with no temper (refusing its rider at once on the first buck) is thrown after 50 ticks on average.
#[test]
fn a_bolting_horse_rolls_a_buck_every_twenty_five_full_updates() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let mut total = 0u64;
    let mut count = 0u64;
    for i in 0..200 {
        let e = spawn(&mut h, "minecraft:horse", 0.5 + i as f64 * 4.0, 0.5);
        h.world.get_mut::<Mount>(e).unwrap().temper = 0;
        assert!(mount::ride(&mut h.world, e, Who::Player(1)), "the first seat is free");
        let start = h.world.resource::<Clock>().0;
        for _ in 0..2000 {
            h.tick();
            if mount::rider_of(&h.world, e).is_none() {
                total += h.world.resource::<Clock>().0 - start;
                count += 1;
                break;
            }
        }
    }
    assert_eq!(count, 200, "every bolting horse throws its rider");
    let mean = total as f64 / count as f64;
    assert!((35.0..=65.0).contains(&mean), "mean ticks before the buck {mean}");
}

/// `AbstractHorse.followMommy`: a bred baby that is not eating gets a path to the nearest bred adult of its kind, the path's target being
/// that adult's place as the tick finds it (`Stroll` `tx`, `tz`); a baby eating its haystack keeps its place.
#[test]
fn a_bred_foal_paths_to_the_nearest_bred_horse() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let foal = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    let mother = spawn(&mut h, "minecraft:horse", 10.5, 0.5);
    h.make_baby(foal, 24000);
    h.world.entity_mut(foal).insert(Bred);
    h.world.entity_mut(mother).insert(Bred);
    let mother_at = h.world.get::<Pos>(mother).unwrap().0;
    h.tick();
    let path = *h.world.get::<Stroll>(foal).unwrap();
    assert_eq!((path.tx, path.tz), (mother_at.x, mother_at.z), "the foal's path ends at its mother's place");
    h.world.entity_mut(foal).insert(EatingHaystack);
    let before = h.world.get::<Pos>(foal).unwrap().0;
    h.tick();
    assert_eq!(h.world.get::<Pos>(foal).unwrap().0, before, "a foal eating its haystack keeps its place");
}

/// A pool of water around the spawn cell, at the height `pool_if_aquatic` uses, wide enough for a steered horse to swim across.
fn pool(h: &mut Herd) {
    for x in -12..=12 {
        for z in -4..=4 {
            for y in 1..=2 {
                h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
}

/// `SkeletonHorse.getSwimSound` and `playSwimSound`: a ridden undead horse that swims in the water makes its own sounds
/// (`entity.skeleton_horse.*`, the generic horse one never), and each swim sound is quiet (`min(0.1, f * 25)`, at most 0.1).
#[test]
fn a_skeleton_horse_swims_with_its_own_quiet_sounds() {
    if !have_data() || wrong_species(&["minecraft:skeleton_horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.world.insert_resource(MoveEventsOn(true));
    let e = spawn(&mut h, "minecraft:skeleton_horse", 0.5, 0.5);
    pool(&mut h);
    assert!(mount::ride(&mut h.world, e, Who::Player(1)), "the first seat is free");
    pilot::set(&mut h.world, e, forward(1.0));
    for _ in 0..60 {
        h.tick();
    }
    let sounds: Vec<(String, f32)> = h
        .drain_host_out()
        .into_iter()
        .filter_map(|ev| match ev {
            HostEvent::Sound { name, volume, .. } if name.starts_with("entity.skeleton_horse.") => Some((name.to_string(), volume)),
            _ => None,
        })
        .collect();
    assert!(!sounds.is_empty(), "a swimming skeleton horse makes its own sounds");
    for (name, volume) in &sounds {
        assert!(name.ends_with("step_water") || name.ends_with("gallop_water") || name.ends_with("swim"), "the sound {name}");
        assert!(*volume <= 0.1, "a swim sound in the water is at most 0.1, got {volume} for {name}");
    }
}

/// A skeleton horse that stands in the water makes no swim sound: a swim plays at a step, and a horse at rest takes none.
#[test]
fn a_resting_skeleton_horse_in_the_water_makes_no_swim_sound() {
    if !have_data() || wrong_species(&["minecraft:skeleton_horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.world.insert_resource(MoveEventsOn(true));
    let _ = spawn(&mut h, "minecraft:skeleton_horse", 0.5, 0.5);
    pool(&mut h);
    for _ in 0..60 {
        h.tick();
    }
    let skeleton = h.drain_host_out().into_iter().filter(|ev| matches!(ev, HostEvent::Sound { name, .. } if name.starts_with("entity.skeleton_horse."))).count();
    assert_eq!(skeleton, 0, "a horse at rest does not swim");
}

/// `Entity.waterSwimSound` of a species without its own sound: a horse that swims in the water makes `entity.generic.swim`.
#[test]
fn a_horse_that_swims_makes_the_generic_swim_sound() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.world.insert_resource(MoveEventsOn(true));
    let e = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    pool(&mut h);
    assert!(mount::ride(&mut h.world, e, Who::Player(1)), "the first seat is free");
    pilot::set(&mut h.world, e, forward(1.0));
    for _ in 0..60 {
        h.tick();
    }
    let generic = h.drain_host_out().into_iter().filter(|ev| matches!(ev, HostEvent::Sound { name, .. } if name.starts_with("entity.generic.swim"))).count();
    assert!(generic > 0, "a swimming horse makes the generic swim sound");
}

/// `Entity.waterSwimSound` is not played by an entity that is silent: the same swimming horse, with the `Silent` mark, makes no swim sound.
#[test]
fn a_silent_horse_that_swims_makes_no_swim_sound() {
    if !have_data() || wrong_species(&["minecraft:horse"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.world.insert_resource(MoveEventsOn(true));
    let e = spawn(&mut h, "minecraft:horse", 0.5, 0.5);
    h.world.entity_mut(e).insert(Silent);
    pool(&mut h);
    assert!(mount::ride(&mut h.world, e, Who::Player(1)), "the first seat is free");
    pilot::set(&mut h.world, e, forward(1.0));
    for _ in 0..60 {
        h.tick();
    }
    let swims = h.drain_host_out().into_iter().filter(|ev| matches!(ev, HostEvent::Sound { name, .. } if name.ends_with(".swim"))).count();
    assert_eq!(swims, 0, "a silent horse makes no swim sound");
}

/// `SwimSounds` is posed at the creation of a species that has a water sound of its own (a monster has the hostile one, a horse none).
#[test]
fn a_monster_carries_the_hostile_swim_sound_and_a_horse_none() {
    if !have_data() || wrong_species(&["minecraft:horse", "minecraft:zombie"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let zombie = spawn(&mut h, "minecraft:zombie", 0.5, 0.5);
    let horse = spawn(&mut h, "minecraft:horse", 8.5, 0.5);
    assert_eq!(h.world.get::<SwimSounds>(zombie).map(|s| s.sounds.swim), Some("entity.hostile.swim"), "a zombie swims with the hostile sound");
    assert!(h.world.get::<SwimSounds>(horse).is_none(), "a horse has the generic sound, no component");
}
