//! The experience a death leaves (`HORIZON_TEST_RESOURCES` set, the species of the run: `HORIZON_SPECIES`): a baby zombie's 2.5 factor, the
//! equipment's bonus, the projectile as the direct entity of the loot context, and the tadpole that leaves none.
use crate::common::*;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use horizon_ecs_proto::bridge::host::PlayerFact;
use horizon_ecs_proto::bridge::players::set_player;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::credit::PlayerCredit;
use horizon_ecs_proto::goals::Player;
use horizon_ecs_proto::loot::{damage_of, note_killer, DeathSeed};
use horizon_ecs_proto::orb::Orb;

/// The species killed in the last 100 ticks by player 1 (a baby, a helmet on it, as asked); the experience of the orbs it left.
fn kill(h: &mut Herd, seed: i64, baby: bool, helmet: bool) -> i32 {
    let player = Player { id: 1, x: 0.5, y: 1.0, z: 0.5, main: 0, off: 0, head: 0, chest: 0, legs: 0, feet: 0 };
    set_player(&mut h.world, player, PlayerFact { alive: true, targetable: true, hero: None, ..Default::default() });
    let sp = *species();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 9);
    if baby {
        h.make_baby(e, 100_000);
    }
    if helmet {
        horizon_ecs_proto::equip_stack::set(&mut h.world, e, 2, Some(ItemStack::new("minecraft:iron_helmet", 1)));
    }
    h.world.entity_mut(e).insert((DeathSeed(seed), PlayerCredit { player: Some(1), uuid: None, until: u64::MAX }));
    note_killer(&mut h.world, e, 1, 0.0, None);
    h.hit(e, 1000.0, (0.0, 0.0));
    h.tick();
    let orbs: Vec<(Entity, i32)> = h.world.query::<(Entity, &Orb)>().iter(&h.world).map(|(e, o)| (e, o.value * o.count as i32)).collect();
    for (e, _) in &orbs {
        h.world.entity_mut(*e).despawn();
    }
    orbs.iter().map(|x| x.1).sum()
}

/// `Zombie.getBaseExperienceReward`: a baby's 5 becomes 12 (`5 * 2.5`, truncated); an adult's stays 5.
#[test]
fn a_baby_zombie_leaves_two_and_a_half_times_the_experience() {
    if !have_data() || wrong_species(&["minecraft:zombie"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    assert_eq!(kill(&mut h, 11, false, false), 5);
    assert_eq!(kill(&mut h, 11, true, false), 12);
}

/// `Mob.getBaseExperienceReward`: a piece in a slot adds `1 + nextInt(3)`, so a helmeted zombie leaves 6 to 8.
#[test]
fn a_piece_of_equipment_adds_one_to_three() {
    if !have_data() || wrong_species(&["minecraft:zombie"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    for k in 0..20 {
        let xp = kill(&mut h, 100 + 31 * k, false, true);
        assert!((6..=8).contains(&xp), "seed {k}: {xp}");
    }
}

/// `getDirectEntity`: a blow carried by an arrow has the arrow as its direct entity and the shooter as its causing one.
#[test]
fn the_projectile_is_the_direct_entity_of_the_loot_context() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let sp = *species();
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 9);
    let who = Some(horizon_ecs_proto::targeting::Who::Player(1));
    record_combat(&mut h.world, e, "arrow", who, false, Some("minecraft:arrow"), 3.0);
    let d = damage_of(&h.world, e).expect("a blow is remembered");
    assert_eq!(d.direct.map(|x| x.kind), Some("minecraft:arrow".to_owned()));
    assert_eq!(d.source.map(|x| x.kind), Some("minecraft:player".to_owned()));
}

/// `Tadpole.shouldDropExperience` is false: killed by a player, it leaves no orb.
#[test]
fn a_tadpole_leaves_no_experience() {
    if !have_data() || wrong_species(&["minecraft:tadpole"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    assert_eq!(kill(&mut h, 5, false, false), 0);
}
