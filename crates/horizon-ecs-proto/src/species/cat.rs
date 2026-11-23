//! `Cat.registerGoals` (`mobs/tame.rs` `register_cat`, the priorities of the code, not those of the fiches): float and panic, sit when
//! ordered, relax on the owner's bed, tempt and the fear of players while it is wild, lie and sit on blocks, follow the owner, leap and bite,
//! breed, stroll. A wild cat hunts rabbits and baby turtles on land. Taming is the rule `TAME` (cod and salmon, one chance in three).
use super::Def;
use crate::behaviours::avoid_entity::{self, Avoid};
use crate::behaviours::{breed, cat_lie_on_block, cat_relax_on_owner, cat_sit_on_block, float, follow_owner, leap_at_target, look_at_player, nearest_target, ocelot_attack, panic, sit_when_ordered, stroll, tempt, Cap, Cond, Entry};
use crate::rules::RuleEntry;
use crate::tame::{TameParams, TAME};

/// Cod and salmon, one chance in three (the health stays what the species has).
const TAME_CAT: TameParams = TameParams { items: &["minecraft:cod", "minecraft:salmon"], chance: 3, health: 10.0, sits: true, eat_sound: None };

const FOOD: &[&str] = &["minecraft:cod", "minecraft:salmon"];
static WILD: Cond = Cond::Not(&Cond::Is("horizon:is_tame"));
static AVOID: avoid_entity::Params = avoid_entity::Params { what: Avoid::Player, max_dist: 16.0, walk: 0.8, sprint: 1.33, ..avoid_entity::Params::DEFAULT };

pub static DEF: Def = Def {
    name: "minecraft:cat",
    despawn: Some(crate::despawn::DespawnDef { removable: crate::despawn::Removable::UntamedAfter(2400), ..crate::despawn::DespawnDef::MOB }),
    sounds: Some(&["minecraft:classic", "minecraft:royal"]),
    variant: Some(&super::variants::CAT),
    reference: None,
    targets: &[
        Entry::new(&nearest_target::MODULE, 1, &nearest_target::Params { types: &["minecraft:rabbit"], must_see: false, ..nearest_target::Params::DEFAULT }).when(&WILD),
        Entry::new(&nearest_target::MODULE, 1, &nearest_target::Params { types: &["minecraft:turtle"], predicate: Some("baby_on_land"), must_see: false, ..nearest_target::Params::DEFAULT }).when(&WILD),
    ],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 1.5, environmental: true, ..panic::Params::DEFAULT }),
        Entry::new(&sit_when_ordered::MODULE, 2, &sit_when_ordered::Params::DEFAULT),
        Entry::new(&cat_relax_on_owner::MODULE, 3, &cat_relax_on_owner::Params::DEFAULT),
        Entry::new(&tempt::MODULE, 4, &tempt::Params { speed: 0.6, items: FOOD, ..tempt::Params::DEFAULT }).when(&WILD),
        Entry::new(&cat_lie_on_block::MODULE, 5, &cat_lie_on_block::Params::DEFAULT),
        Entry::new(&follow_owner::MODULE, 6, &follow_owner::Params { speed: 1.0, start: 10.0, stop: 5.0 }),
        Entry::new(&cat_sit_on_block::MODULE, 7, &cat_sit_on_block::Params::DEFAULT),
        Entry::new(&leap_at_target::MODULE, 8, &leap_at_target::Params { yd: 0.3 }),
        Entry::new(&ocelot_attack::MODULE, 9, &ocelot_attack::Params::DEFAULT),
        Entry::new(&breed::MODULE, 10, &breed::Params { speed: 0.8, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&stroll::MODULE, 11, &stroll::Params { speed: 0.8, water_avoiding: 1.000_000_1e-5, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 12, &look_at_player::Params { range: 10.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 4, &AVOID).when(&WILD),
    ],
    rules: &[RuleEntry::new(&crate::walk_paces::PACES, &()), RuleEntry::new(&crate::dye::DYE,&crate::dye::DyeParams::CAT), RuleEntry::new(&TAME, &TAME_CAT), RuleEntry::new(&crate::hiss::HISSES, &crate::hiss::CAT_HISS)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Tamable]),
    ..Def::EMPTY
};
