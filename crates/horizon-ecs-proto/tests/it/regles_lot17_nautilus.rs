//! `AbstractNautilus` monté (nautilus.rs:828, lot 17b) : le saut d'une monture qui fait son dash joue son son au tick suivant, pas sur le tick
//! du saut (`executeRidersJump` pose le drapeau, `tickRidden` joue le son). `HORIZON_SPECIES=minecraft:nautilus`, `HORIZON_TEST_RESOURCES` set.
use crate::common;
use common::*;
use horizon_ecs_proto::behaviours::Interaction;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::hostile::PlayerState;
use horizon_ecs_proto::mount::{DashActive, Mount};
use horizon_ecs_proto::pilot::{self, PilotInput};
use horizon_ecs_proto::tame;
use horizon_ecs_proto::targeting::{set_target, Target, Who};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:nautilus"])
}
fn spawn_of(h: &mut Herd, name: &str, x: f64) -> bevy_ecs::entity::Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, 0.5, 5);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    e
}
fn player(h: &mut Herd, main: u32) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5, y: 1.0, z: 2.5, main, off: 0, ..Default::default() });
}
/// Compte les sons de dash joués à l'hôte depuis le dernier appel (l'eau et la terre).
fn heard_dash(h: &mut Herd) -> usize {
    h.world.resource_mut::<Events>().host.drain().filter(|e| matches!(e, HostEvent::Sound { name, .. } if name.starts_with("entity.nautilus.dash"))).count()
}

/// `ChargeAttack.dealKnockBack` (nautilus.rs:760) : au contact, la cible reçoit en plus du recul par défaut une poussée le long du museau
/// (+z pour yaw 0). La vache est en diagonale : sans la poussée du museau, elle irait autant en x qu'en z.
#[test]
fn a_charge_pushes_its_target_along_the_nautilus_facing() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let nautilus = spawn_of(&mut h, "minecraft:nautilus", 0.5);
    let cow = spawn_of(&mut h, "minecraft:cow", 2.5);
    for _ in 0..5 {
        h.tick();
    }
    set_target(&mut h.world, nautilus, Target::new(Who::Entity(cow), 200, u8::MAX));
    let start = h.world.get::<Health>(cow).unwrap().0;
    for _ in 0..80 {
        h.tick();
        if h.world.get::<Health>(cow).unwrap().0 < start {
            break;
        }
    }
    assert!(h.world.get::<Health>(cow).unwrap().0 < start, "témoin : la charge touche la vache");
    let v = h.world.get::<Vel>(cow).unwrap().0;
    assert!(v.z - v.x > 0.3, "poussée le long du museau en plus du recul : {v:?}");
}

/// `AbstractNautilus.dash` (nautilus.rs:231-250) et `tick` (l.314-321) : le drapeau du dash est levé au saut, il tombe au 7e tick (cooldown
/// sous 35) ; le son du dash joue au tick suivant, le son « prêt » au 40e tick.
#[test]
fn a_dash_raises_its_flag_for_six_ticks_and_rings_ready_at_forty() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let pet = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(pet);
    player(&mut h, 0);
    tame::set_owner(&mut h.world, pet, Who::Player(1));
    assert!(interact(&mut h.world, pet, &Interaction { player: 1, item: 0 }));
    for _ in 0..5 {
        h.tick();
    }
    heard_dash(&mut h);
    pilot::set(
        &mut h.world,
        pet,
        PilotInput { who: Who::Player(1), forward: 0.0, strafe: 0.0, yaw: 0.0, pitch: 0.0, jump: 1.0, sprint: false, sneak: false, steer: false, keys: [false; 4] },
    );
    h.tick();
    assert!(h.world.get::<DashActive>(pet).is_some(), "le saut lève le drapeau");
    let mut sounds = Vec::new();
    for step in 1..=40 {
        h.tick();
        if step == 6 {
            assert!(h.world.get::<DashActive>(pet).is_some(), "le drapeau tient 6 ticks après le saut");
        }
        if step == 7 {
            assert!(h.world.get::<DashActive>(pet).is_none(), "le drapeau tombe au 7e tick");
        }
        for ev in h.world.resource_mut::<Events>().host.drain() {
            if let HostEvent::Sound { name, .. } = ev {
                if name.starts_with("entity.nautilus.dash") {
                    sounds.push((step, name));
                }
            }
        }
    }
    assert_eq!(sounds.iter().map(|s| s.0).collect::<Vec<_>>(), vec![1, 40], "le son du dash au tick suivant, le son prêt au 40e");
    assert!(sounds[1].1.contains("dash_ready"), "{:?}", sounds[1].1);
}

/// `ChargeAttack.dealKnockBack` (nautilus.rs:760) sur un joueur : la poussée supplémentaire est déposée dans `PlayerState.pushes` (l'hôte
/// l'applique par le recul joueur), le museau à yaw 0 donne (sin 0, -cos 0).
#[test]
fn a_charge_queues_its_extra_push_for_the_player_it_hits() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let nautilus = spawn_of(&mut h, "minecraft:nautilus", 0.5);
    player(&mut h, 0);
    for _ in 0..5 {
        h.tick();
    }
    set_target(&mut h.world, nautilus, Target::new(Who::Player(1), 200, u8::MAX));
    for _ in 0..80 {
        h.tick();
        if !h.world.resource::<PlayerState>().pushes.is_empty() {
            break;
        }
    }
    let pushes = h.world.resource::<PlayerState>().pushes.clone();
    assert_eq!(pushes.len(), 1, "témoin : la charge touche le joueur");
    let (id, (x, z, power), by) = pushes[0];
    assert_eq!((id, by), (1, nautilus));
    assert!(x.abs() < 1e-9 && (z + 1.0).abs() < 1e-9, "museau à yaw 0 : (sin 0, -cos 0) : ({x}, {z})");
    assert!(power >= 0.4, "puissance de la poussée : {power}");
}

#[test]
fn a_dash_jump_plays_its_sound_on_the_tick_after() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let pet = spawn_species(&mut h.world, species(), 0.5, 0.5, 5);
    h.world.resource_mut::<Entered>().0.push(pet);
    player(&mut h, 0);
    tame::set_owner(&mut h.world, pet, Who::Player(1));
    assert!(interact(&mut h.world, pet, &Interaction { player: 1, item: 0 }));
    assert_eq!(h.world.get::<Mount>(pet).unwrap().rider, Some(Who::Player(1)), "témoin : le nautile est monté");
    for _ in 0..5 {
        h.tick();
    }
    heard_dash(&mut h);
    pilot::set(
        &mut h.world,
        pet,
        PilotInput { who: Who::Player(1), forward: 0.0, strafe: 0.0, yaw: 0.0, pitch: 0.0, jump: 1.0, sprint: false, sneak: false, steer: false, keys: [false; 4] },
    );
    h.tick();
    assert_eq!(heard_dash(&mut h), 0, "le son ne joue pas sur le tick du saut");
    h.tick();
    assert_eq!(heard_dash(&mut h), 1, "le son du dash joue au tick suivant");
}
