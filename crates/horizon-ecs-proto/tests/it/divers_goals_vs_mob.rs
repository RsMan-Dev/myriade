//! The goals of the lot's species in the frozen registry, one test per species. The registry is read, not a herd: no world, no
//! `HORIZON_SPECIES`. The aliases are the triage's: the Mob's goal → the ECS module.
//! NON FAIT : la même scène côté `Mob` (ses buts par espèce) : le Mob n'expose pas ici sa liste de buts, assertion ECS seule.
use horizon_ecs_proto::behaviours::{
    animal_panic, breed, charge_attack, defend_trusted, fish_swim, float, follow_mob, follow_parent, fly_stroll, give_to_owner,
    go_to_wanted, item_seek, land_on_shoulder, look_at_player, lookout, random_look, stay_close_to_owner, still_under_riders, stroll, tempt, Module,
};
use horizon_ecs_proto::species::config;

/// The resolved configuration of `name` carries each of the modules.
fn carries(name: &str, modules: &[&'static Module]) {
    let c = config(name);
    for &m in modules {
        assert!(c.has(m), "{name} does not carry {}", m.name);
    }
}

#[test]
fn allay_carries_its_goals() {
    carries(
        "minecraft:allay",
        &[&go_to_wanted::MODULE, &give_to_owner::MODULE, &stay_close_to_owner::MODULE, &fly_stroll::MODULE, &animal_panic::MODULE, &look_at_player::MODULE, &random_look::MODULE],
    );
}

#[test]
fn nautilus_carries_its_goals() {
    carries("minecraft:nautilus", &[&animal_panic::MODULE, &charge_attack::MODULE, &fish_swim::MODULE, &breed::MODULE, &tempt::MODULE]);
}

#[test]
fn zombie_nautilus_carries_its_goals() {
    carries("minecraft:zombie_nautilus", &[&charge_attack::MODULE, &fish_swim::MODULE, &tempt::MODULE]);
}

#[test]
fn dolphin_carries_its_goals() {
    carries("minecraft:dolphin", &[&fish_swim::MODULE]);
}

#[test]
fn frog_carries_its_goals() {
    carries("minecraft:frog", &[&animal_panic::MODULE]);
}

#[test]
fn sulfur_cube_carries_its_goals() {
    carries("minecraft:sulfur_cube", &[&item_seek::MODULE, &tempt::MODULE]);
}

#[test]
fn parrot_carries_its_goals() {
    carries("minecraft:parrot", &[&land_on_shoulder::MODULE, &follow_mob::MODULE, &fly_stroll::MODULE]);
}

#[test]
fn fox_carries_its_goals() {
    carries("minecraft:fox", &[&lookout::MODULE, &defend_trusted::MODULE]);
}

#[test]
fn camel_carries_follow_parent() {
    // Its entry is conditional (FollowWhenStanding): only the presence of the module is checked here.
    carries("minecraft:camel", &[&follow_parent::MODULE]);
}

#[test]
fn camel_husk_follows_its_parent_and_does_not_breed() {
    let c = config("minecraft:camel_husk");
    carries("minecraft:camel_husk", &[&follow_parent::MODULE]);
    assert!(!c.has(&breed::MODULE), "camel_husk breeds: the jar's canMate / canFallInLove are false");
}

#[test]
fn turtle_carries_breed() {
    carries("minecraft:turtle", &[&breed::MODULE]);
}

#[test]
fn happy_ghast_carries_its_goals() {
    carries("minecraft:happy_ghast", &[&float::MODULE, &tempt::MODULE, &still_under_riders::MODULE]);
}

#[test]
fn mannequin_carries_no_goal() {
    assert!(config("minecraft:mannequin").entries.is_empty(), "the jar gives the mannequin no goal");
}

#[test]
#[ignore = "le Mob donne à mannequin Float/Panic/Stroll/LookAtPlayer/RandomLookAround, le jar aucun but : suivi/ecs-ecarts.md"]
fn mannequin_carries_the_five_goals_of_the_mob() {
    carries("minecraft:mannequin", &[&float::MODULE, &animal_panic::MODULE, &stroll::MODULE, &look_at_player::MODULE, &random_look::MODULE]);
}
