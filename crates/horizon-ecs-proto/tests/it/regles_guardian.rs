//! Règles du gardien, reprises de `guardian_tests.rs` (valeurs vanilla 26.3 inchangées) : les piques d'un gardien immobile
//! infligent 2 dégâts de ronces à la cause vivante directe, la marche aléatoire de rang 5 le ramène vers son foyer de 16 blocs,
//! et la marche garde les drapeaux MOVE et LOOK.
use horizon_ecs_proto::behaviours::{look_at_player, stroll::MODULE_LOOK};
use horizon_ecs_proto::goals::{LOOK, MOVE};
use horizon_ecs_proto::rules::guardian_thorns::{spikes_strike, THORNS};
use horizon_ecs_proto::rules::move_towards_restriction::{home_target, within_home};

#[test]
fn a_still_guardian_spikes_whoever_hits_it() {
    // (cas, en mouvement, direct, cause entité) -> piques. Ronces : 2 dégâts.
    let cases = [
        ("a player's sword", false, true, true, true),
        ("a mob's blow", false, true, true, true),
        ("a warden's boom", false, true, true, true),
        ("a refused blow", false, true, true, true),
        ("thorns", false, true, false, false),
        ("an arrow", false, false, true, false),
        ("magic", false, true, false, false),
        ("a swimming guardian", true, true, true, false),
    ];
    for (case, moving, direct, by_entity, spiked) in cases {
        assert_eq!(spikes_strike(moving, direct, by_entity), spiked, "{case}");
    }
    assert_eq!(THORNS, 2.0);
}

#[test]
fn a_displaced_elder_guardian_swims_back_toward_its_home() {
    // Foyer en (0, 62, 0), rayon 16 : à 60 blocs on est dehors, à 5 blocs dedans.
    let home = [0, 62, 0];
    assert!(!within_home(home, 16, [60.5, 62.0, 0.5]), "outside its home");
    assert!(within_home(home, 16, [5.5, 62.0, 0.5]), "inside its home");
    let target = home_target(home);
    let from_home = (target[0] * target[0] + target[2] * target[2]).sqrt();
    assert!(from_home < 60.0, "the path leads home, not further out: {target:?}");
}

#[test]
fn a_strolling_guardian_keeps_its_head_with_its_body() {
    // `randomStrollGoal.setFlags(MOVE, LOOK)` : la marche tient la tête avec le corps.
    assert_eq!(MODULE_LOOK.flags & (MOVE | LOOK), MOVE | LOOK);
}

#[test]
fn a_guardian_looks_at_another_guardian_elder_included() {
    // `LookAtPlayerGoal(this, Guardian.class, 12.0F, 0.01F)` : portée 12 blocs, une chance sur 100, élder inclus.
    // ECS : species/guardian.rs:23, portée lue dans la configuration (la portée des joueurs, 36 au carré, n'y sert pas).
    let reg = horizon_ecs_proto::species::registry();
    let p = reg.by_name("minecraft:guardian").unwrap().config.param::<look_at_player::Params>(&look_at_player::MODULE, 0).unwrap();
    assert_eq!(p.range, 12.0);
    assert_eq!(p.period, 100);
    assert!(p.species.contains(&"minecraft:guardian") && p.species.contains(&"minecraft:elder_guardian"));
}
