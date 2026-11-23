//! Fiche de naissance (`suivi/ecs-portage/phase1-listes/finalisation-ecs-plan.md`, champs MANQUE) : une valeur tirée ou posée à la naissance
//! par règle vanilla 26.3 relue dans le jar. Les tirages passent par les `Draws` de l'espèce, comme à la finalisation.
use crate::common::*;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::World;
use horizon_core::random::LegacyRandom;
use horizon_ecs_proto::behaviours::act_phase::{ActPhase, RareState};
use horizon_ecs_proto::behaviours::emerge;
use horizon_ecs_proto::behaviours::nearest_target;
use horizon_ecs_proto::goals::{Events, Online, Player};
use horizon_ecs_proto::hostile::PlayerState;
use horizon_ecs_proto::targeting::{Target, Who};
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::behaviours::long_jump;
use horizon_ecs_proto::behaviours::posture;
use horizon_ecs_proto::behaviours::brute_stroll_home::{self, Home};
use horizon_ecs_proto::behaviours::ram;
use horizon_ecs_proto::behaviours::ram::Shed;
use horizon_ecs_proto::bridge::sync::{self, SyncValue};
use horizon_ecs_proto::nbt;
use horizon_ecs_proto::species::goat::Screaming;
use horizon_ecs_proto::behaviours::spawn_draw::{run, When};
use horizon_ecs_proto::attr_mod::AttrMods;
use horizon_ecs_proto::cow::{game_now, now_of, spawn_species, ActiveUntil, Anchor, FluidState, Health, Pos, SpeciesTable, Yaw};
use horizon_ecs_proto::size::Size;
use horizon_ecs_proto::species;
use horizon_ecs_proto::rules::spawn_draw::Ctx;

fn spawn(h: &mut Herd, name: &str) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let species = *table.iter().find(|s| s.name == name).unwrap();
    spawn_species(&mut h.world, &species, 4.5, 4.5, 5)
}

/// Les tirages de la naissance de l'espèce, dans l'ordre de la finalisation (avant puis après l'espèce), sur la graine `seed`.
fn birth(w: &mut World, e: Entity, name: &str, seed: i64) {
    let draws = species::registry().by_name(name).unwrap().draws.unwrap();
    let none = |_: &str| false;
    let c = Ctx { biome: "minecraft:plains", biome_has: &none, special: 1.0, baby: false };
    let mut rng = LegacyRandom::new(seed);
    let mut group = None;
    run(w, e, draws, When::Before, &mut group, &c, &mut rng);
    run(w, e, draws, When::After, &mut group, &c, &mut rng);
}

/// `GoatAi.initMemories`: le délai du saut de chèvre tiré dans [600, 1200] à chaque naissance.
#[test]
fn goat_long_jump_memory_is_drawn_from_600_to_1200() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    for seed in 1..=50 {
        let goat = spawn(&mut h, "minecraft:goat");
        birth(&mut h.world, goat, "minecraft:goat", seed);
        let left = long_jump::cooldown_left(&h.world, goat).expect("the memory is set at the birth");
        assert!((600..=1200).contains(&left), "seed {seed}: {left}");
    }
}

/// `GoatAi.initMemories`: le délai du bélier tiré dans [600, 6000] à chaque naissance, daté au temps du monde.
#[test]
fn goat_ram_memory_is_drawn_from_600_to_6000() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    for seed in 1..=50 {
        let goat = spawn(&mut h, "minecraft:goat");
        birth(&mut h.world, goat, "minecraft:goat", seed);
        let left = ram::cooldown_left(&h.world, goat).expect("the memory is set at the birth");
        assert!((600..=6000).contains(&left), "seed {seed}: {left}");
    }
}

/// `FrogAi.initMemories`: le délai du saut de grenouille tiré dans [100, 140].
#[test]
fn frog_long_jump_memory_is_drawn_from_100_to_140() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    for seed in 1..=50 {
        let frog = spawn(&mut h, "minecraft:frog");
        birth(&mut h.world, frog, "minecraft:frog", seed);
        let left = long_jump::cooldown_left(&h.world, frog).expect("the memory is set at the birth");
        assert!((100..=140).contains(&left), "seed {seed}: {left}");
    }
}

/// La mémoire de délai du bélier et celle de l'attaque se relisent par leur valeur restante (la lecture d'une sauvegarde).
#[test]
fn cooldown_memories_read_back_their_ticks_left() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let goat = spawn(&mut h, "minecraft:goat");
    ram::set_cooldown_left(&mut h.world, goat, Some(700));
    assert_eq!(ram::cooldown_left(&h.world, goat), Some(700));
    ram::set_cooldown_left(&mut h.world, goat, None);
    assert_eq!(ram::cooldown_left(&h.world, goat), None);
}

/// `WardenAi`, `Warden.finalizeSpawn` : `DIG_COOLDOWN` de 1200 ticks à chaque naissance.
#[test]
fn warden_dig_cooldown_is_1200_ticks_at_birth() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let warden = spawn(&mut h, "minecraft:warden");
    birth(&mut h.world, warden, "minecraft:warden", 3);
    let now = now_of(&h.world, warden);
    assert_eq!(h.world.get::<ActPhase>(warden).unwrap().dig_cooldown_until, now + 1200);
}

/// `Warden.finalizeSpawn` pour la raison `Triggered` seulement : l'émergence est demandée, sinon elle ne l'est pas.
#[test]
fn warden_emerges_only_when_triggered() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let warden = spawn(&mut h, "minecraft:warden");
    assert!(!h.world.get::<RareState>(warden).unwrap().emerge_pending, "a natural warden does not emerge");
    emerge::triggered(&mut h.world, warden);
    assert!(h.world.get::<RareState>(warden).unwrap().emerge_pending, "a triggered warden emerges");
}

/// `Phantom.finalizeSpawn` : `setPhantomSize(0)` à la naissance.
#[test]
fn phantom_is_size_zero_at_birth() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let phantom = spawn(&mut h, "minecraft:phantom");
    birth(&mut h.world, phantom, "minecraft:phantom", 4);
    assert_eq!(h.world.get::<Size>(phantom).map(|s| s.0), Some(0));
}

/// `Shulker.finalizeSpawn` : `setYRot(0)`, le coquillage regarde au nord à la naissance.
#[test]
fn shulker_faces_zero_at_birth() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let shulker = spawn(&mut h, "minecraft:shulker");
    birth(&mut h.world, shulker, "minecraft:shulker", 6);
    assert_eq!(h.world.get::<Yaw>(shulker).map(|y| y.0), Some(0.0));
}

/// `Camel.finalizeSpawn` : `resetLastPoseChangeTickToFullStand`, debout, la dernière bascule datée 53 ticks avant (au plus tôt à 0).
#[test]
fn camel_stands_at_birth_with_its_last_pose_53_ticks_back() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let camel = spawn(&mut h, "minecraft:camel");
    birth(&mut h.world, camel, "minecraft:camel", 8);
    let now = game_now(&h.world);
    assert!(!posture::is_sitting(&h.world, camel));
    assert_eq!(posture::last_pose_tick(&h.world, camel), Some((now - 53).max(0)));
}

/// `AbstractHorse.randomizeAttributes` du cheval : la vie (15 + nextInt(8) + nextInt(9)), puis la vitesse et la saut (trois doubles chacune).
#[test]
fn horse_draws_health_speed_and_jump() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let horse = spawn(&mut h, "minecraft:horse");
    let mods = h.world.get::<AttrMods>(horse).unwrap().0.clone();
    let base = |name: &str| mods.iter().find(|x| x.0 == name).map(|x| x.1).unwrap();
    let speed = base("movement_speed");
    let jump = base("jump_strength");
    assert!((0.1125..=0.3375).contains(&speed), "speed {speed}");
    assert!((0.4..=1.0).contains(&jump), "jump {jump}");
    let health = h.world.get::<Health>(horse).unwrap().0;
    assert!((15.0..=30.0).contains(&health), "max health {health}");
}

/// `ZombieHorse.randomizeAttributes` : son saut (0,5 + trois doubles / 15) et sa vitesse ((9 + trois doubles) / 42,16).
#[test]
fn zombie_horse_draws_its_own_jump_and_speed() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let zombie = spawn(&mut h, "minecraft:zombie_horse");
    let mods = h.world.get::<AttrMods>(zombie).unwrap().0.clone();
    let base = |name: &str| mods.iter().find(|x| x.0 == name).map(|x| x.1).unwrap();
    let speed = base("movement_speed");
    let jump = base("jump_strength");
    assert!((9.0 / 42.16..=12.0 / 42.16).contains(&speed), "speed {speed}");
    assert!((0.5..=0.7).contains(&jump), "jump {jump}");
}

/// `SkeletonHorse.randomizeAttributes` : seul le saut est tiré, pas la vitesse.
#[test]
fn skeleton_horse_draws_only_its_jump() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let skeleton = spawn(&mut h, "minecraft:skeleton_horse");
    let mods = h.world.get::<AttrMods>(skeleton).unwrap().0.clone();
    assert!(mods.iter().any(|x| x.0 == "jump_strength"));
    assert!(!mods.iter().any(|x| x.0 == "movement_speed"));
}

/// Les entrées de synchro de l'entité, en ordre de slot.
fn synced(w: &World, e: Entity) -> Vec<(u8, SyncValue)> {
    let mut out = Vec::new();
    sync::sync_entries(w, e, &mut out);
    out
}

/// Chèvre criarde et cornes : sauvées puis relues (`IsScreamingGoat`, `HasLeftHorn`, `HasRightHorn`), la synchro (18, 19, 20) suit.
#[test]
fn goat_screaming_and_horns_round_trip() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let goat = spawn(&mut h, "minecraft:goat");
    h.world.entity_mut(goat).insert((Screaming, Shed(3)));
    let before = synced(&h.world, goat);
    assert!(before.contains(&(18, SyncValue::Bool(true))) && before.contains(&(19, SyncValue::Bool(false))));
    let saved = nbt::save(&h.world, goat).expect("a saved goat");
    h.world.entity_mut(goat).remove::<(Screaming, Shed)>();
    nbt::load(&mut h.world, goat, &saved).expect("a declared goat");
    assert!(h.world.get::<Screaming>(goat).is_some(), "the screaming goat stays screaming");
    assert_eq!(h.world.get::<Shed>(goat).map(|s| s.0), Some(3), "both horns stay lost");
    assert_eq!(synced(&h.world, goat), before);
}

/// Phantom : la taille (`size`) et l'ancre (`anchor_pos`) sont sauvées puis relues ; la synchro de la taille suit.
#[test]
fn phantom_size_and_anchor_round_trip() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let phantom = spawn(&mut h, "minecraft:phantom");
    h.world.entity_mut(phantom).insert((Size(3), Anchor([5, 70, 9])));
    let before = synced(&h.world, phantom);
    assert_eq!(before, [(16, SyncValue::Int(3))]);
    let saved = nbt::save(&h.world, phantom).expect("a saved phantom");
    h.world.entity_mut(phantom).remove::<(Size, Anchor)>();
    nbt::load(&mut h.world, phantom, &saved).expect("a declared phantom");
    assert_eq!(h.world.get::<Size>(phantom).map(|s| s.0), Some(3));
    assert_eq!(h.world.get::<Anchor>(phantom).map(|a| a.0), Some([5, 70, 9]));
    assert_eq!(synced(&h.world, phantom), before);
}

/// Chameau : la date de sa dernière bascule de pose (`LastPoseTick`) est sauvée puis relue ; la synchro (20) la porte.
#[test]
fn camel_last_pose_tick_round_trip() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let camel = spawn(&mut h, "minecraft:camel");
    posture::load_pose_tick(&mut h.world, camel, 1234);
    let before = synced(&h.world, camel);
    assert_eq!(before, [(20, SyncValue::Long(1234))]);
    let saved = nbt::save(&h.world, camel).expect("a saved camel");
    posture::load_pose_tick(&mut h.world, camel, 999);
    nbt::load(&mut h.world, camel, &saved).expect("a declared camel");
    assert_eq!(posture::last_pose_tick(&h.world, camel), Some(1234));
    assert_eq!(synced(&h.world, camel), before);
}

/// Délais de mémoire : `minecraft:ram_cooldown_ticks` (le bélier) relu après une sauvegarde, la mémoire retirée avant.
#[test]
fn ram_cooldown_memory_round_trip() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let goat = spawn(&mut h, "minecraft:goat");
    ram::set_cooldown_left(&mut h.world, goat, Some(700));
    let saved = nbt::save(&h.world, goat).expect("a saved goat");
    ram::set_cooldown_left(&mut h.world, goat, None);
    nbt::load(&mut h.world, goat, &saved).expect("a declared goat");
    assert_eq!(ram::cooldown_left(&h.world, goat), Some(700));
}

/// Mémoire de fouille du gardien : `minecraft:dig_cooldown` (1200 ticks à la naissance) relue après une sauvegarde.
#[test]
fn warden_dig_cooldown_round_trip() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let warden = spawn(&mut h, "minecraft:warden");
    birth(&mut h.world, warden, "minecraft:warden", 3);
    let until = h.world.get::<ActPhase>(warden).unwrap().dig_cooldown_until;
    let saved = nbt::save(&h.world, warden).expect("a saved warden");
    h.world.get_mut::<ActPhase>(warden).unwrap().dig_cooldown_until = 0;
    nbt::load(&mut h.world, warden, &saved).expect("a declared warden");
    assert_eq!(h.world.get::<ActPhase>(warden).unwrap().dig_cooldown_until, until);
}

/// Maison de la brute : `minecraft:home`, un `GlobalPos` (dimension et position) relu après une sauvegarde.
#[test]
fn brute_home_round_trip() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let brute = spawn(&mut h, "minecraft:piglin_brute");
    h.world.entity_mut(brute).insert(Home { block: [10, 64, -3], def: &brute_stroll_home::Params::DEFAULT, next_ok: 0 });
    let saved = nbt::save(&h.world, brute).expect("a saved brute");
    h.world.entity_mut(brute).remove::<Home>();
    nbt::load(&mut h.world, brute, &saved).expect("a declared brute");
    assert_eq!(h.world.get::<Home>(brute).map(|x| x.block), Some([10, 64, -3]));
}

/// Attributs du cheval (`AttrMods`) : sauvés puis relus, vie, vitesse et saut inchangés.
#[test]
fn horse_attributes_round_trip() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let horse = spawn(&mut h, "minecraft:horse");
    let mods = |w: &World| w.get::<AttrMods>(horse).unwrap().0.iter().map(|x| (x.0.clone(), x.1)).collect::<Vec<_>>();
    let before = mods(&h.world);
    let saved = nbt::save(&h.world, horse).expect("a saved horse");
    h.world.entity_mut(horse).remove::<AttrMods>();
    nbt::load(&mut h.world, horse, &saved).expect("a declared horse");
    assert_eq!(mods(&h.world), before);
}

/// `NautilusAi.initMemories` : `ATTACK_TARGET_COOLDOWN` tiré dans [2400, 3600] à la naissance.
#[test]
fn nautilus_attack_cooldown_is_drawn_from_2400_to_3600() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    for seed in 1..=50 {
        let nautilus = spawn(&mut h, "minecraft:nautilus");
        birth(&mut h.world, nautilus, "minecraft:nautilus", seed);
        let left = nearest_target::attack_cooldown_left(&h.world, nautilus).expect("the memory is set at the birth");
        assert!((2400..=3600).contains(&left), "seed {seed}: {left}");
    }
}

/// Met l'animal dans l'eau pour le tick (le monde de test n'a pas d'eau).
fn wet(h: &mut Herd, e: Entity) {
    if let Some(mut f) = h.world.get_mut::<FluidState>(e) {
        f.in_water = true;
    }
}

/// `findNearestValidAttackTarget` : pendant le délai, la recherche ne tourne pas (le délai n'est ni reposé ni consommé) ;
/// à l'échéance, la recherche tourne et repose un délai de 2400 à 3600 ticks.
#[test]
fn nautilus_search_waits_for_the_cooldown_then_runs() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let nautilus = spawn(&mut h, "minecraft:nautilus");
    let prey = spawn(&mut h, "minecraft:cow");
    for e in [nautilus, prey] {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    }
    nearest_target::set_attack_cooldown_left(&mut h.world, nautilus, Some(200));
    for _ in 0..150 {
        wet(&mut h, nautilus);
        h.tick();
    }
    assert_eq!(nearest_target::attack_cooldown_left(&h.world, nautilus), Some(50), "pas de recherche avant l'échéance");
    let mut reset = None;
    for t in 0..400 {
        wet(&mut h, nautilus);
        h.tick();
        if nearest_target::attack_cooldown_left(&h.world, nautilus).is_some_and(|l| l > 1000) {
            reset = Some(t);
            break;
        }
    }
    assert!(reset.is_some(), "à l'échéance, la recherche tourne et repose le délai");
}

/// Délai d'attaque du nautile : `minecraft:attack_target_cooldown` sauvé puis relu.
#[test]
fn nautilus_attack_cooldown_round_trip() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let nautilus = spawn(&mut h, "minecraft:nautilus");
    nearest_target::set_attack_cooldown_left(&mut h.world, nautilus, Some(3000));
    let saved = nbt::save(&h.world, nautilus).expect("a saved nautilus");
    nearest_target::set_attack_cooldown_left(&mut h.world, nautilus, None);
    nbt::load(&mut h.world, nautilus, &saved).expect("a declared nautilus");
    assert_eq!(nearest_target::attack_cooldown_left(&h.world, nautilus), Some(3000));
}

/// `findNearestValidAttackTarget` : le nautile prend le poisson-globe mouillé (`nautilus_hostiles`), sur deux graines ; un tirage
/// à 50 % par recherche, la cible vient à la première recherche qui tire juste.
#[test]
fn nautilus_target_is_a_wet_pufferfish_on_two_seeds() {
    if !have_data() {
        return;
    }
    for seed in [1, 2] {
        let mut h = Herd::new_awake(0, 1.0, true, false, seed);
        let nautilus = spawn(&mut h, "minecraft:nautilus");
        let prey = spawn(&mut h, "minecraft:pufferfish");
        for e in [nautilus, prey] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        let mut picked = None;
        for _ in 0..1500 {
            wet(&mut h, nautilus);
            wet(&mut h, prey);
            h.tick();
            if let Some(t) = h.world.get::<Target>(nautilus) {
                picked = Some(t.who);
                break;
            }
        }
        assert_eq!(picked, Some(Who::Entity(prey)), "seed {seed}");
    }
}

/// `findNearestValidAttackTarget` : un joueur dans l'eau n'est pas cible sans `ANGRY_AT` ; avec la mémoire (le joueur qui a
/// frappé le nautile, 400 ticks), il l'est tant qu'il reste dans l'eau.
#[test]
fn nautilus_angry_at_a_player_in_the_water_is_its_target() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let nautilus = spawn(&mut h, "minecraft:nautilus");
    let at = h.world.get::<Pos>(nautilus).unwrap().0;
    h.world.resource_mut::<Online>().set(Player { id: 1, x: at.x, y: at.y, z: at.z, ..Default::default() });
    h.world.resource_mut::<PlayerState>().set_in_water(1, true);
    h.world.get_mut::<ActiveUntil>(nautilus).unwrap().0 = u64::MAX;
    for _ in 0..200 {
        wet(&mut h, nautilus);
        h.tick();
    }
    assert_eq!(h.world.get::<Target>(nautilus).map(|t| t.who), None, "sans mémoire ANGRY_AT, le joueur n'est pas cible");
    let until = now_of(&h.world, nautilus) + 400;
    h.world.entity_mut(nautilus).insert(nearest_target::AngerMemory { who: Who::Player(1), until });
    for _ in 0..200 {
        wet(&mut h, nautilus);
        h.tick();
    }
    assert_eq!(h.world.get::<Target>(nautilus).map(|t| t.who), Some(Who::Player(1)), "ANGRY_AT désigne le joueur");
}

/// Délai du nautile zombie (`ZombieNautilusAi`, `CountDownCooldownTicks` de `ATTACK_TARGET_COOLDOWN`) : une échéance, sans
/// décrément par tick ; il est fini au tick d'échéance.
#[test]
fn zombie_nautilus_cooldown_ends_at_its_deadline() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let zombie = spawn(&mut h, "minecraft:zombie_nautilus");
    h.world.get_mut::<ActiveUntil>(zombie).unwrap().0 = u64::MAX;
    nearest_target::set_attack_cooldown_left(&mut h.world, zombie, Some(100));
    for _ in 0..50 {
        h.tick();
    }
    assert_eq!(nearest_target::attack_cooldown_left(&h.world, zombie), Some(50));
    for _ in 0..50 {
        h.tick();
    }
    assert_eq!(nearest_target::attack_cooldown_left(&h.world, zombie), None, "fini à l'échéance");
}

/// Délai du nautile zombie : `minecraft:attack_target_cooldown` sauvé puis relu.
#[test]
fn zombie_nautilus_attack_cooldown_round_trip() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let zombie = spawn(&mut h, "minecraft:zombie_nautilus");
    nearest_target::set_attack_cooldown_left(&mut h.world, zombie, Some(3000));
    let saved = nbt::save(&h.world, zombie).expect("a saved zombie nautilus");
    nearest_target::set_attack_cooldown_left(&mut h.world, zombie, None);
    nbt::load(&mut h.world, zombie, &saved).expect("a declared zombie nautilus");
    assert_eq!(nearest_target::attack_cooldown_left(&h.world, zombie), Some(3000));
}

/// `Warden.finalizeSpawn`, raison TRIGGERED : le son `entity.warden.agitated` est joué à l'apparition déclenchée, pas à la naissance naturelle.
#[test]
fn triggered_warden_plays_agitated_at_once() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let warden = spawn(&mut h, "minecraft:warden");
    let agitated = |h: &mut Herd| {
        h.world
            .resource_mut::<Events>()
            .host
            .drain()
            .filter(|e| matches!(e, HostEvent::Sound { name, .. } if name == "entity.warden.agitated"))
            .count()
    };
    agitated(&mut h);
    emerge::triggered(&mut h.world, warden);
    assert_eq!(agitated(&mut h), 1, "le gardien déclenché joue entity.warden.agitated une fois");
}
