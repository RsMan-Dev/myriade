//! Règles des illageois et du gardien relues depuis les tests de l'ancien Mob (lot 17b) : la colère par suspect et ses seuils (`suspect_anger`,
//! fiche `species/warden.rs`), la sortie de terre sans son, le délai de fouille tenu en laisse, Johnny, la mémoire du sorcier, la portée du
//! wololo. Les autres tests du lot sont couverts ailleurs ou tenus au journal `suivi/ecs-portage/phase1-listes/regles-fermees-lot17b.txt`.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::behaviours::act_phase::ActPhase;
use horizon_ecs_proto::behaviours::emerge;
use horizon_ecs_proto::behaviours::suspect_anger::{self, Level};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::custom_name::{self, Johnny};
use horizon_ecs_proto::goals::Events;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::leash::{self, Holder};
use horizon_ecs_proto::rules::spell_use::near_sheep;
use horizon_ecs_proto::targeting::{self, set_target, Target, Who};

fn skip() -> bool {
    !have_data() || wrong_species(&["minecraft:warden"])
}

fn spawn(h: &mut Herd, x: f64) -> Entity {
    spawn_of(h, "minecraft:warden", x)
}

fn spawn_of(h: &mut Herd, name: &str, x: f64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, 0.5, 5);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    e
}

/// Compte les sons joués à l'hôte depuis le dernier appel.
fn heard(h: &mut Herd, sound: &str) -> usize {
    h.world.resource_mut::<Events>().host.drain().filter(|e| matches!(e, HostEvent::Sound { name, .. } if name == sound)).count()
}

/// Un joueur loin de tout, en ligne : une cible hors de vue, mais présente.
fn far_player(h: &mut Herd) {
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 200.5, y: 1.0, z: 0.5, ..Default::default() });
}

/// `Warden` : la colère d'un seul suspect suit les seuils de 40 (agité) et 80 (en colère) de l'espèce.
#[test]
fn anger_levels_follow_the_warden_thresholds() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let warden = spawn(&mut h, 0.5);
    let who = Who::Player(1);
    suspect_anger::raise(&mut h.world, warden, who, 39);
    assert!(matches!(suspect_anger::level(&h.world, warden), Level::Calm), "39 : calme");
    suspect_anger::raise(&mut h.world, warden, who, 1);
    assert!(matches!(suspect_anger::level(&h.world, warden), Level::Agitated), "40 : agité");
    suspect_anger::raise(&mut h.world, warden, who, 40);
    assert!(matches!(suspect_anger::level(&h.world, warden), Level::Angry), "80 : en colère");
}

/// `Warden` : une nouvelle colère contre le même suspect s'ajoute à la première.
#[test]
fn a_suspect_angered_again_adds_to_its_anger() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let warden = spawn(&mut h, 0.5);
    let who = Who::Player(1);
    suspect_anger::raise(&mut h.world, warden, who, 25);
    suspect_anger::raise(&mut h.world, warden, who, 25);
    assert!(matches!(suspect_anger::level(&h.world, warden), Level::Agitated), "50 : agité");
    suspect_anger::raise(&mut h.world, warden, who, 30);
    assert!(matches!(suspect_anger::level(&h.world, warden), Level::Angry), "80 : en colère");
}

/// `Warden` : la colère cumulée plafonne à 150 (`MAX_ANGER`).
#[test]
fn anger_accumulates_to_a_cap_of_150() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let warden = spawn(&mut h, 0.5);
    let total = suspect_anger::raise(&mut h.world, warden, Who::Player(1), 200);
    assert_eq!(total, 150, "la colère plafonne à 150");
}

/// `Warden`, sortie de terre déclenchée : elle ne joue pas de son d'émergence pendant la montée.
#[test]
fn a_silent_warden_climbs_out_without_a_sound() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let warden = spawn(&mut h, 0.5);
    emerge::triggered(&mut h.world, warden);
    heard(&mut h, "entity.warden.agitated");
    for _ in 0..140 {
        h.tick();
    }
    assert_eq!(heard(&mut h, "entity.warden.emerge"), 0, "la montée est muette");
}

/// `WardenAi.setDigCooldown` (warden_target_tests.rs:38-62) : tenu en laisse, le gardien remet son délai de fouille à 1200 ticks, comme
/// persistant ; un gardien libre ne le remet pas.
#[test]
fn a_leashed_warden_keeps_its_dig_cooldown_at_1200_ticks() {
    if skip() {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let warden = spawn(&mut h, 0.5);
    let cow = spawn_of(&mut h, "minecraft:cow", 3.5);
    let free = spawn(&mut h, 8.5);
    assert!(leash::attach(&mut h.world, warden, Holder::Entity(cow)), "témoin : le gardien est tenu en laisse");
    h.tick();
    let now = now_of(&h.world, warden);
    assert_eq!(h.world.get::<ActPhase>(warden).unwrap().dig_cooldown_until, now + 1200, "tenu en laisse : 1200 ticks");
    assert_eq!(h.world.get::<ActPhase>(free).unwrap().dig_cooldown_until, 0, "libre : pas de délai remis");
}

/// `Vindicator.addAdditionalSaveData` : `Johnny` est un octet écrit seulement quand le nom est posé ; un chargement à 1 pose le nom, à 0 le lève.
#[test]
fn johnny_is_a_byte_written_only_when_true_and_a_load_sets_it() {
    use horizon_core::nbt::Tag;
    use horizon_ecs_proto::nbt;
    let comp = |k: &'static str, t: Tag| match Tag::compound([(k, t)]) {
        Tag::Compound(c) => c,
        _ => unreachable!(),
    };
    if !have_data() || wrong_species(&["minecraft:vindicator"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let v = spawn_of(&mut h, "minecraft:vindicator", 0.5);
    assert_eq!(nbt::get(&h.world, v, "Johnny"), None, "sans le nom, pas de clé");
    custom_name::set(&mut h.world, v, Some("Johnny".to_string()));
    assert_eq!(nbt::get(&h.world, v, "Johnny"), Some(Tag::Byte(1)), "le nom écrit l'octet 1");
    assert_eq!(nbt::load(&mut h.world, v, &comp("Johnny", Tag::Byte(0))), Some(0));
    assert!(h.world.get::<Johnny>(v).is_none(), "l'octet 0 lève le nom");
    assert_eq!(nbt::load(&mut h.world, v, &comp("Johnny", Tag::Byte(1))), Some(0));
    assert!(h.world.get::<Johnny>(v).is_some(), "l'octet 1 pose le nom");
    assert_eq!(nbt::load(&mut h.world, v, &comp("Johnny", Tag::Int(1))), Some(1), "un mauvais type est refusé");
}

/// `Vindicator` nommé Johnny (illager.rs:1096) : il cible une créature vivante non illageoise, une vache à 3 blocs ; le même vindicateur
/// sans ce nom ne la cible pas.
#[test]
fn a_vindicator_named_johnny_hunts_a_cow_and_one_without_the_name_does_not() {
    if !have_data() || wrong_species(&["minecraft:vindicator"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let plain = spawn_of(&mut h, "minecraft:vindicator", 0.5);
    let johnny = spawn_of(&mut h, "minecraft:vindicator", 10.5);
    let cow = spawn_of(&mut h, "minecraft:cow", 3.5);
    custom_name::set(&mut h.world, johnny, Some("Johnny".to_string()));
    assert!(h.world.get::<Johnny>(johnny).is_some(), "témoin : le nom Johnny est latché");
    for _ in 0..60 {
        h.tick();
    }
    assert!(matches!(h.world.get::<Target>(johnny).map(|t| t.who), Some(Who::Entity(x)) if x == cow), "Johnny cible la vache");
    assert!(h.world.get::<Target>(plain).is_none(), "sans le nom, pas de cible sur la vache");
}

/// Le sorcier garde un joueur hors de vue 300 ticks (illager.rs:959, `unseen_memory` de species/evoker.rs:15) : la cible tient à 290
/// ticks, elle est oubliée passé 300.
#[test]
fn a_spellcaster_forgets_an_unseen_player_after_300_ticks() {
    if !have_data() || wrong_species(&["minecraft:evoker"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(8, true, false);
    let evoker = spawn_of(&mut h, "minecraft:evoker", 0.5);
    far_player(&mut h);
    set_target(&mut h.world, evoker, Target::player(1, 300));
    for _ in 0..290 {
        h.tick();
    }
    assert!(h.world.get::<Target>(evoker).is_some(), "290 ticks : la cible tient");
    for _ in 0..20 {
        h.tick();
    }
    assert!(h.world.get::<Target>(evoker).is_none(), "310 ticks : la cible est oubliée");
}

/// Le wololo porte 16 blocs à la verticale-horizontale, réduits par la visibilité du mouton (illager.rs:1160 et 1209) : un mouton bleu
/// invisible à 5 blocs n'est pas wololoé (sa portée tombe à 2 blocs), à 2 blocs il l'est ; à 14 blocs, la portée réduite l'exclut aussi.
#[test]
fn an_invisible_blue_sheep_is_wololoed_only_within_its_shrunken_reach() {
    let reach = targeting::sight_range(16.0, targeting::creature_visibility(true, None));
    assert!(!near_sheep(5.0, 0.0, 0.0, 25.0, reach), "5 blocs : hors de portée");
    assert!(near_sheep(2.0, 0.0, 0.0, 4.0, reach), "2 blocs : dans la portée");
    assert!(!near_sheep(14.0, 0.0, 0.0, 196.0, reach), "14 blocs : hors de portée");
}
