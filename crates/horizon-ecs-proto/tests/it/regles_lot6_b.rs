//! Lot 6b of the rules of the mobs (`ai.rs` 45 to 66): a dying player takes the anger he caused with him and not another mob's, a
//! countdown outliving its target ends with that player's death, a peaceful level drops the player target of a golem, a wither rose
//! gives wither, a creature stuck in a berry bush or in powder snow loses its fall, and a wither is stuck by none of them. Expected
//! values are the vanilla 26.3 ones (`javap`, `NeutralMob`, `WitherRoseBlock`, `SweetBerryBushBlock`, `PowderSnowBlock`, `WitherBoss`).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, ShapeOp, VoxelShape, Vec3};
use myriade_engine_lib::items::ItemStack;
use horizon_ecs_proto::behaviours::neutral_anger::{anger_at, is_angry};
use horizon_ecs_proto::age_lock::AgeLocked;
use horizon_ecs_proto::behaviours::bed::{lie_down, Sleeping};
use horizon_ecs_proto::behaviours::trade_session::Trading;
use myriade_engine_lib::merchant::Session;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::brain::{erase as brain_erase, set as brain_set, set_activity, MemValue};
use horizon_ecs_proto::species::villager::HOME;
use horizon_ecs_proto::goals::interact;
use horizon_ecs_proto::hand_feed::age_up;
use horizon_ecs_proto::items;
use horizon_ecs_proto::spawn_finalize::set_baby;
use horizon_ecs_proto::used_stack::UsedStack;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::effects::{self, Difficulty, Kind};
use horizon_ecs_proto::forgive_dead_player::forgive;
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::hostile::Daylight;
use horizon_ecs_proto::hostile::PlayerState;
use horizon_ecs_proto::inside_effects::{stuck_factor, WasInPowder, K_BERRY, K_POWDER};
use horizon_ecs_proto::targeting::{clear_target, set_target, Target, Who};

/// The species of the table by name, standing at (x, z).
fn spawn(h: &mut Herd, name: &str, x: f64, z: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &sp, x, z, 5)
}

/// A creature falling at `p` (the cow's box), living when `living`: the body the inside-block rules read.
fn body(h: &mut Herd, p: Vec3, living: bool) -> Entity {
    let e = h
        .world
        .spawn((
            Pos(p),
            Vel(Vec3::ZERO),
            Body { width: 0.6, height: 1.8, step_height: 0.6, speed: 0.0, fall_factor: 1.0, safe_fall: 3.0, fall_mult: 1.0, movement_efficiency: 0.0, object: false, orb: false, tnt: false, lava_walker: false },
            Gravity(0.08),
            Bounds(box_at(p, 0.6, 1.8)),
            Cell(u32::MAX, [i32::MIN; 8], u32::MAX),
            Contact { on_ground: false, horizontal: false, fall_distance: 0.0, stuck: 0 },
            FluidState::default(),
            Moving,
        ))
        .id();
    if living {
        h.world.entity_mut(e).insert((Health(20.0), HurtAt { at: 0, last: 0.0 }));
    }
    enter_index(&mut h.world, e);
    e
}

/// An enderman holding player 1 as its target, the player online beside it.
fn holding_enderman(h: &mut Herd) -> Entity {
    h.set_online(&[(6.5, 0.5)]);
    let e = spawn(h, "minecraft:enderman", 0.5, 0.5);
    set_target(&mut h.world, e, Target::player(1, 20));
    h.tick();
    e
}

/// `NeutralMob.playerDied` through `stopBeingAngry`: the anger for the dying player ends with its target, the anger for another
/// player does not.
#[test]
fn a_dying_players_anger_ends_with_its_target_and_another_players_does_not() {
    if !have_data() || wrong_species(&["minecraft:enderman"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = holding_enderman(&mut h);
    assert_eq!(anger_at(&h.world, e), Some(Who::Player(1)), "held: angry at player 1");
    assert!(!forgive(&mut h.world, e, 2), "another player's death ended the anger");
    assert!(is_angry(&h.world, e), "another player's death ended the anger");
    assert!(forgive(&mut h.world, e, 1), "the dying player's anger was not forgiven");
    assert!(!is_angry(&h.world, e) && anger_at(&h.world, e).is_none(), "still angry after the forgiveness");
    assert!(h.world.get::<Target>(e).is_none(), "the target was kept after the forgiveness");
}

/// Anger at another mob is not the dying player's to end: the target and the anger stay.
#[test]
fn anger_at_another_mob_survives_a_dying_players_death() {
    if !have_data() || wrong_species(&["minecraft:enderman"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = spawn(&mut h, "minecraft:enderman", 0.5, 0.5);
    let foe = spawn(&mut h, "minecraft:enderman", 3.5, 0.5);
    let mut t = Target::new(Who::Entity(foe), u16::MAX, u8::MAX);
    t.must_see = false;
    t.follow_scale = f64::INFINITY;
    set_target(&mut h.world, e, t);
    h.tick();
    assert_eq!(anger_at(&h.world, e), Some(Who::Entity(foe)), "held: angry at the mob");
    assert!(!forgive(&mut h.world, e, 1), "a dying player forgave the anger at a mob");
    assert!(is_angry(&h.world, e), "the anger at a mob ended with a player's death");
    assert_eq!(h.world.get::<Target>(e).map(|t| t.who), Some(Who::Entity(foe)), "the target at the mob was dropped");
}

/// Once the target is lost the anger is a countdown; the dying player's death ends that countdown and the anger with it, as
/// `NeutralMob.playerDied` does whether or not the mob still holds a target.
#[test]
fn a_countdown_outliving_its_target_ends_with_the_dying_players_death() {
    if !have_data() || wrong_species(&["minecraft:enderman"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    let e = holding_enderman(&mut h);
    clear_target(&mut h.world, e);
    h.tick();
    assert_eq!(anger_at(&h.world, e), Some(Who::Player(1)), "lost target: the anger is still for player 1");
    assert!(forgive(&mut h.world, e, 1), "the countdown was not ended by the dying player's death");
    assert!(!is_angry(&h.world, e), "the countdown still runs after the forgiveness");
    assert!(h.world.get::<Target>(e).is_none(), "a target appeared after the forgiveness");
}

/// `NeutralMob.playerDied` and the peaceful level: an iron golem whose player target is no longer targetable (Peaceful) drops it,
/// and its anger ends with it.
#[test]
fn a_peaceful_difficulty_drops_an_iron_golems_player_target() {
    if !have_data() || wrong_species(&["minecraft:iron_golem"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    h.set_online(&[(6.5, 0.5)]);
    let e = spawn(&mut h, "minecraft:iron_golem", 0.5, 0.5);
    set_target(&mut h.world, e, Target::player(1, 20));
    h.tick();
    h.world.resource_mut::<PlayerState>().peaceful = true;
    h.tick();
    assert!(h.world.get::<Target>(e).is_none(), "peaceful: the golem kept its player target");
    assert!(anger_at(&h.world, e).is_none(), "peaceful: the anger was kept");
}

/// `WitherRoseBlock.entityInside`: a living creature in the rose gets wither (40 ticks) at any hit timer, except at peaceful.
#[test]
fn a_wither_rose_gives_wither_except_at_peaceful() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    h.set_block_state(0, 1, 0, "minecraft:wither_rose", &[]);
    h.tick();
    let e = body(&mut h, Vec3::new(0.5, 1.0, 0.5), true);
    h.world.insert_resource(Difficulty(2));
    h.tick();
    assert!(effects::has(&h.world, e, Kind::Wither), "a creature in the rose at normal difficulty has no wither");
    let f = body(&mut h, Vec3::new(3.5, 1.0, 0.5), true);
    h.world.insert_resource(Difficulty(0));
    h.tick();
    assert!(!effects::has(&h.world, f, Kind::Wither), "a creature in the rose at peaceful has wither");
}

/// A creature falling past a wither rose gets wither on the way; one falling beside it does not.
#[test]
fn a_falling_creature_passing_a_wither_rose_gets_wither_and_one_beside_it_does_not() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    h.set_block_state(0, 1, 0, "minecraft:wither_rose", &[]);
    h.tick();
    let e = body(&mut h, Vec3::new(0.5, 3.0, 0.5), true);
    for _ in 0..15 {
        h.tick();
    }
    assert!(effects::has(&h.world, e, Kind::Wither), "the rose on the way did not give wither");
    let beside = body(&mut h, Vec3::new(3.5, 3.0, 0.5), true);
    for _ in 0..15 {
        h.tick();
    }
    assert!(!effects::has(&h.world, beside, Kind::Wither), "a creature beside the rose got wither");
}

/// `SweetBerryBushBlock.entityInside` sticks a living creature (0.8, 0.75, 0.8) and `Entity.makeStuckInBlock` resets its fall.
#[test]
fn a_cow_in_a_sweet_berry_bush_is_stuck_and_loses_its_fall() {
    if !have_data() {
        return;
    }
    assert_eq!(stuck_factor(K_BERRY), Vec3::new(0.8, 0.75, 0.8));
    let mut h = Herd::with_slab_awake(8, true, true);
    h.set_block_state(0, 1, 0, "minecraft:sweet_berry_bush", &[("age", "0")]);
    h.tick();
    let e = body(&mut h, Vec3::new(0.5, 3.0, 0.5), true);
    for _ in 0..3 {
        h.tick();
    }
    let c = h.world.get::<Contact>(e).unwrap();
    assert_eq!(c.stuck, K_BERRY, "the bush did not stick the cow");
    assert_eq!(c.fall_distance, 0.0, "the stuck cow kept its fall");
}

/// `PowderSnowBlock.entityInside`: a living creature is stuck (0.9, 1.5, 0.9) once it stands in the powder snow it is in (its own
/// cell), not on the first step.
#[test]
fn a_cow_in_powder_snow_is_stuck_once_it_stands_in_its_own_cell() {
    if !have_data() {
        return;
    }
    assert_eq!(stuck_factor(K_POWDER), Vec3::new(0.9, 1.5, 0.9));
    let mut h = Herd::with_slab_awake(8, true, true);
    h.set_block(0, 1, 0, "minecraft:powder_snow");
    h.tick();
    let e = body(&mut h, Vec3::new(0.5, 1.0, 0.5), true);
    for _ in 0..4 {
        h.tick();
    }
    assert_eq!(h.world.get::<Contact>(e).unwrap().stuck, K_POWDER, "the powder snow did not stick the cow");
}

/// `WitherBoss.makeStuckInBlock` is empty: a wither in a sweet berry bush is not stuck, so its fall is not reset.
#[test]
fn a_wither_in_a_sweet_berry_bush_is_not_stuck() {
    if !have_data() || wrong_species(&["minecraft:wither"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    h.set_block_state(0, 1, 0, "minecraft:sweet_berry_bush", &[("age", "0")]);
    h.tick();
    let e = spawn(&mut h, "minecraft:wither", 0.5, 0.5);
    for _ in 0..3 {
        h.tick();
    }
    assert_eq!(h.world.get::<Contact>(e).map_or(0, |c| c.stuck), 0, "the bush stuck the wither");
}

/// `WitherBoss.makeStuckInBlock` is empty for powder snow too: a wither standing in it is not stuck.
#[test]
fn a_wither_in_powder_snow_is_not_stuck() {
    if !have_data() || wrong_species(&["minecraft:wither"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    h.set_block(0, 1, 0, "minecraft:powder_snow");
    h.tick();
    let e = spawn(&mut h, "minecraft:wither", 0.5, 0.5);
    for _ in 0..4 {
        h.tick();
    }
    assert_eq!(h.world.get::<Contact>(e).map_or(0, |c| c.stuck), 0, "the powder snow stuck the wither");
}

/// An endermite walks on powder snow as a rabbit does (`#powder_snow_walkable_mobs`): it stands on it, a zombie sinks.
#[test]
fn an_endermite_stands_on_powder_snow_like_a_rabbit() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    for y in 1..=5 {
        h.set_block(0, y, 0, "minecraft:powder_snow");
    }
    let e = spawn(&mut h, "minecraft:endermite", 0.0, 7.0);
    for _ in 0..60 {
        h.tick();
    }
    let y = h.world.get::<Pos>(e).unwrap().0.y;
    assert!(y > 5.5, "an endermite stands on powder snow (y = {y})");
}

/// `LivingEntity.baseTick` in lava: 4 at once, fire for 300 ticks, and the next 4 once the invulnerable time runs down (tick 11).
#[test]
fn a_creature_standing_in_lava_is_hit_and_set_on_fire_then_hit_again_on_the_eleventh_tick() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(20, false, false);
    for y in 1..=4 {
        h.set_block(0, y, 0, "minecraft:lava");
    }
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 6);
    let start = health(&h, e);
    h.tick();
    assert!(health(&h, e) <= start - 4.0, "one lava hit on the first tick");
    let burn = h.world.get::<Burning>(e).expect("set on fire").until - now_of(&h.world, e) as i64;
    assert_eq!(burn, 300, "fire for 300 ticks");
    for _ in 0..10 {
        h.tick();
    }
    assert!(health(&h, e) <= start - 8.0, "the second lava hit on tick 11");
}

/// `PowderSnowBlock.entityInside`: a burning creature breaks the powder snow it stands in (no drop, `mobGriefing`) and is put out.
#[test]
fn a_burning_cow_in_powder_snow_breaks_the_block_and_is_put_out() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    h.set_block(0, 1, 0, "minecraft:powder_snow");
    let e = ecs_cow(&mut h, 0.5, 1.0, 0.5, 6);
    set_on_fire(&mut h.world, e, 100);
    h.tick();
    let broke = h.world.resource_mut::<Events>().host.drain().any(|ev| matches!(ev, HostEvent::BlockBreak { at: [0, 1, 0], drop: false, .. }));
    assert!(broke, "the powder snow breaks under a burning creature");
    assert!(h.world.get::<Burning>(e).is_none(), "the creature is put out");
}

fn fox_sounds(h: &mut Herd) -> Vec<(String, f32)> {
    h.world
        .resource_mut::<Events>()
        .host
        .drain()
        .filter_map(|e| match e {
            HostEvent::Sound { name, volume, .. } if name.starts_with("entity.fox.") => Some((name.to_string(), volume)),
            _ => None,
        })
        .collect()
}

/// `Fox.getAmbientSound` and `playAmbientSound`: at night with no player within 16 blocks a screech comes, at volume 2; by day or with a player near, never.
#[test]
#[ignore = "daily"]
fn a_fox_screeches_at_night_with_no_player_near_at_volume_two() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let e = spawn(&mut h, "minecraft:fox", 0.0, 7.0);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    for _ in 0..100_000 {
        h.tick();
    }
    let night = fox_sounds(&mut h);
    assert!(night.iter().any(|(n, v)| n == "entity.fox.screech" && *v == 2.0), "a screech at night, at volume 2");
    h.set_online(&[(2.0, 7.0)]);
    for _ in 0..100_000 {
        h.tick();
    }
    assert!(!fox_sounds(&mut h).iter().any(|(n, _)| n == "entity.fox.screech"), "a player within 16 blocks");
    // A spectator does not keep it off (`!player.isSpectator()` in the box).
    h.world.resource_mut::<PlayerState>().spectator = vec![false, true];
    for _ in 0..100_000 {
        h.tick();
    }
    assert!(fox_sounds(&mut h).iter().any(|(n, v)| n == "entity.fox.screech" && *v == 2.0), "a spectator within 16 blocks");
    h.world.resource_mut::<Online>().remove(1);
    h.world.resource_mut::<horizon_ecs_proto::hostile::Bright>().0 = true;
    for _ in 0..100_000 {
        h.tick();
    }
    assert!(!fox_sounds(&mut h).iter().any(|(n, _)| n == "entity.fox.screech"), "by day");
}

/// `Mob.isSunBurnTick` with `wasInPowderSnow`: a zombie that stepped out of powder snow is still wet at its first move out, and the mark
/// goes the tick after.
#[test]
fn a_zombie_that_stepped_out_of_powder_snow_keeps_its_memory_one_tick() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    h.world.resource_mut::<Daylight>().0 = true;
    for y in 1..=3 {
        h.set_block(0, y, 0, "minecraft:powder_snow");
    }
    let e = spawn(&mut h, "minecraft:zombie", 0.5, 0.5);
    for _ in 0..60 {
        h.tick();
    }
    assert!(h.world.get::<WasInPowder>(e).is_some(), "in the powder snow");
    let out = Vec3::new(3.5, 2.0, 3.5);
    h.world.entity_mut(e).insert((Pos(out), Bounds(box_at(out, 0.6, 1.8))));
    h.tick();
    assert!(h.world.get::<WasInPowder>(e).is_some(), "the tick it stepped out");
    h.tick();
    h.tick();
    assert!(h.world.get::<WasInPowder>(e).is_none(), "the tick after it stepped out");
}

/// `SweetBerryBushBlock.entityInside` hurts a wither (living, not a fox or a bee) once for 1, through its armor 4 (`WitherBoss` ARMOR):
/// 1 x (1 - 3.5 / 25) = 0.86.
#[test]
fn wither_takes_damage_when_moving_in_mature_sweet_berry_bush() {
    if !have_data() || wrong_species(&["minecraft:wither"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, true);
    h.set_block_state(0, 1, 0, "minecraft:sweet_berry_bush", &[("age", "3")]);
    h.tick();
    let e = spawn(&mut h, "minecraft:wither", 0.5, 0.5);
    let start = health(&h, e);
    h.tick();
    let hurt = start - health(&h, e);
    assert!((hurt - 0.86).abs() < 1e-3, "one prick of 0.86 through the armor, got {hurt}");
}

/// `Goat.ageBoundaryReached` on the tick the baby grows up (`setBaby(false)`): the baby's attack of 1 gives way to the adult 2.
#[test]
fn a_goat_that_grows_up_on_its_tick_gets_the_adult_attack_damage() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:goat").expect("a goat");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    set_baby(&mut h.world, e, 3);
    assert_eq!(h.world.get::<AttackDamage>(e).map(|a| a.0), Some(1.0), "a baby goat hits for 1");
    for _ in 0..4 {
        h.tick();
    }
    assert_eq!(h.world.get::<AttackDamage>(e).map(|a| a.0), Some(2.0), "the adult goat hits for 2");
}

/// The same crossing through food (`AgeableMob.ageUp`): a meal takes 12 s x 0.1 off a 240-tick wait, so the goat grows on tick 220.
#[test]
fn a_baby_goat_fed_past_its_boundary_gets_the_adult_attack_damage() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:goat").expect("a goat");
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    set_baby(&mut h.world, e, 240);
    let wheat = items::item_id("minecraft:wheat");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: wheat, off: 0, ..Default::default() });
    h.world.insert_resource(UsedStack { stack: ItemStack::new("minecraft:wheat", 1), creative: false });
    assert!(age_up(&mut h.world, e, &Interaction { player: 1, item: wheat }));
    for _ in 0..230 {
        h.tick();
    }
    assert!(h.world.get::<GrowAt>(e).is_none(), "the goat grew up");
    assert_eq!(h.world.get::<AttackDamage>(e).map(|a| a.0), Some(2.0), "the adult goat hits for 2");
}

/// `AgeableMob.aiStep` and `setAgeLocked`: a baby with no AI still grows up on its time, and a baby locked by a golden dandelion does not.
#[test]
fn a_baby_with_no_ai_grows_up_and_a_locked_one_does_not() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:pig").expect("a pig");
    let free = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    let locked = spawn_species(&mut h.world, &sp, 4.5, 0.5, 8);
    set_baby(&mut h.world, free, 3);
    h.set_no_ai(free, true);
    set_baby(&mut h.world, locked, 3);
    let dandelion = items::item_id("minecraft:golden_dandelion");
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main: dandelion, off: 0, ..Default::default() });
    h.world.insert_resource(UsedStack { stack: ItemStack::new("minecraft:golden_dandelion", 1), creative: false });
    assert!(interact(&mut h.world, locked, &Interaction { player: 1, item: dandelion }));
    for _ in 0..10 {
        h.tick();
    }
    assert!(h.world.get::<GrowAt>(free).is_none(), "the baby with no AI grew up");
    assert!(h.world.get::<AgeLocked>(locked).is_some(), "the dandelion locked the baby");
    assert!(h.world.get::<GrowAt>(locked).is_some(), "the locked baby is still a baby");
}

/// `Entity.isInWall` on a partial shape: a half slab suffocates an eye whose box reaches into it, not one above it.
#[test]
fn a_suffocating_partial_shape_must_reach_the_eye() {
    let slab = VoxelShape::from_box(Aabb::new(Vec3::ZERO, Vec3::new(1.0, 0.5, 1.0))).unwrap();
    let intersects = |x: i32, y: i32, z: i32, eye: &Aabb| {
        (x, y, z) == (0, 0, 0) && slab.join_is_not_empty(&VoxelShape::from_box(*eye).unwrap(), ShapeOp::AND).unwrap()
    };
    assert!(in_wall(0.6, [0.5, 0.25, 0.5], intersects));
    assert!(!in_wall(0.6, [0.5, 0.75, 0.5], intersects));
}

/// `Entity.isInWall` has no width guard (#5872): a hoglin (1.3965 wide) and a zombie (0.6) are taken by the wall at the eye, and the
/// hoglin's wider eye box reaches the wall where the zombie's does not.
#[test]
fn wide_mobs_suffocate_by_the_eye_box() {
    let wall = |x: i32, y: i32, z: i32, eye: &Aabb| {
        x >= 0 && VoxelShape::block().moved(Vec3::new(f64::from(x), f64::from(y), f64::from(z))).join_is_not_empty(&VoxelShape::from_box(*eye).unwrap(), ShapeOp::AND).unwrap()
    };
    assert!(in_wall(1.3965, [0.5, 64.5, 0.5], wall));
    assert!(in_wall(0.6, [0.5, 64.5, 0.5], wall));
    assert!(in_wall(0.6, [-0.2, 64.5, 0.5], wall));
    assert!(!in_wall(0.6, [-0.3, 64.5, 0.5], wall));
    assert!(in_wall(1.3965, [-0.3, 64.5, 0.5], wall));
    assert!(!in_wall(1.3965, [-3.0, 64.5, 0.5], wall));
    assert!(!in_wall(1.0, [-0.4, 64.5, 0.5], wall));
}

/// `SleepInBed.canStillUse`: a villager lying in its home bed keeps sleeping, with or without AI; once the home is gone it wakes.
#[test]
fn sleep_answer_is_applied_with_or_without_ai() {
    if !have_data() || wrong_species(&["minecraft:villager"]) {
        return;
    }
    for no_ai in [false, true] {
        let mut h = Herd::with_slab_awake(60, true, false);
        let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:villager").expect("a villager");
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        let bed = [0, 1, 0];
        brain_set(&mut h.world, e, &HOME, MemValue::Block(bed), None);
        set_activity(&mut h.world, e, "rest");
        lie_down(&mut h.world, e, bed);
        horizon_ecs_proto::dragon::flight::set_no_ai(&mut h.world, e, no_ai);
        h.tick();
        assert!(h.world.get::<Sleeping>(e).is_some(), "NoAI={no_ai}: the home bed keeps the sleeper");
        brain_erase(&mut h.world, e, &HOME);
        h.tick();
        assert!(h.world.get::<Sleeping>(e).is_none(), "NoAI={no_ai}: without a home it wakes");
    }
}

/// The sound names `Herd` wrote over `ticks` ticks.
fn ambient_names(h: &mut Herd, ticks: usize) -> Vec<String> {
    let mut out = Vec::new();
    for _ in 0..ticks {
        h.tick();
        for ev in h.drain_host_out() {
            if let HostEvent::Sound { name, .. } = ev {
                out.push(name.to_string());
            }
        }
    }
    out
}

/// `Mob.baseTick` plays the villager's ambient: `villager.ambient` awake, `villager.trade` in a trade, silence asleep.
#[test]
fn villager_ambient_sound_follows_sleep_and_trade() {
    if !have_data() || wrong_species(&["minecraft:villager"]) {
        return;
    }
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:villager").expect("a villager");
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    let awake = ambient_names(&mut h, 800);
    assert!(awake.iter().any(|n| n == "entity.villager.ambient"), "awake: {awake:?}");
    h.world.entity_mut(e).insert(Trading { player: 1, session: Session::new(Vec::new(), true), since: 0 });
    let trade = ambient_names(&mut h, 800);
    assert!(trade.iter().any(|n| n == "entity.villager.trade") && !trade.iter().any(|n| n == "entity.villager.ambient"), "trade: {trade:?}");
    h.world.entity_mut(e).remove::<Trading>();
    brain_set(&mut h.world, e, &HOME, MemValue::Block([0, 1, 0]), None);
    lie_down(&mut h.world, e, [0, 1, 0]);
    let asleep = ambient_names(&mut h, 800);
    assert!(asleep.is_empty(), "asleep: {asleep:?}");
}

/// `WanderingTrader.getAmbientSound`: `wandering_trader.ambient` awake, `wandering_trader.trade` in a trade, no sleep rule.
#[test]
fn wandering_trader_ambient_sound_is_its_own_and_its_trade_one() {
    if !have_data() || wrong_species(&["minecraft:wandering_trader"]) {
        return;
    }
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:wandering_trader").expect("a wandering trader");
    let mut h = Herd::with_slab_awake(60, true, false);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    let awake = ambient_names(&mut h, 800);
    assert!(awake.iter().any(|n| n == "entity.wandering_trader.ambient"), "awake: {awake:?}");
    h.world.entity_mut(e).insert(Trading { player: 1, session: Session::new(Vec::new(), true), since: 0 });
    let trade = ambient_names(&mut h, 800);
    assert!(trade.iter().any(|n| n == "entity.wandering_trader.trade") && !trade.iter().any(|n| n == "entity.wandering_trader.ambient"), "trade: {trade:?}");
}
