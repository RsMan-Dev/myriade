//! `Rabbit` (`mobs/rabbit.rs:47-70`): float, climbing powder snow, panic, breeding and tempting with its food (the tag `rabbit_food`), raiding
//! the garden, looking at players. The flight from the player, the wolf and the monsters is for the others than the killer bunny
//! (`rabbit:killer`, the variant 99 drawn or read at the spawn), which attacks instead. Left out, waiting for a parameter or module: the
//! monsters are a list built at run time, the stroll that avoids water (`stroll` has no such parameter). It moves by bounds along a path
//! (`locomotion/hop.rs`, `Params::RABBIT`).
use super::Def;
use crate::locomotion::{hop, Medium};
use crate::behaviours::{avoid_entity::{self, Avoid}, breed, climb_on_powder_snow, float, hurt_by, look_at_player, melee, nearest_target, panic, raid_garden, stroll, tempt, Cond, Entry};

const FOOD: &[&str] = &["minecraft:carrot", "minecraft:golden_carrot", "minecraft:dandelion"];

static NOT_KILLER: Cond = Cond::Not(&Cond::Is("rabbit:killer"));
const fn flee(what: Avoid, max_dist: f64) -> avoid_entity::Params {
    avoid_entity::Params { what, max_dist, walk: 2.2, sprint: 2.2, ..avoid_entity::Params::DEFAULT }
}
static PLAYER: avoid_entity::Params = flee(Avoid::Player, 8.0);
static WOLF: avoid_entity::Params = flee(Avoid::Named(&["minecraft:wolf"]), 10.0);
static MONSTERS: avoid_entity::Params = flee(Avoid::Monsters, 4.0);

static KILLER: Cond = Cond::Is("rabbit:killer");
static KILLER_MELEE: melee::Params = melee::Params { speed: 1.4, follow_unseen: true, ..melee::Params::DEFAULT };

/// `getRandomRabbitVariant`: one `nextInt(100)`, then by the biome's tags (white 80/20 with the splotched, gold; else brown 50, salt 40,
/// black 10). `RabbitType` is saved; 99 is the killer bunny, which `Rabbit.setVariant` arms (armour 8, +5 attack, a default name).
static VARIANT: crate::rules::variant::Table = crate::rules::variant::Table {
    bound: 100,
    rows: &[
        crate::rules::variant::Row { tag: Some("minecraft:spawns_white_rabbits"), cuts: &[(80, 1)], last: 3 },
        crate::rules::variant::Row { tag: Some("minecraft:spawns_gold_rabbits"), cuts: &[], last: 4 },
    ],
    otherwise: crate::rules::variant::Row { tag: None, cuts: &[(50, 0), (90, 5)], last: 2 },
    key: "RabbitType",
    pick: crate::rules::variant::Pick::Rows,
    redraw: true,
    ids: &[0, 1, 2, 3, 4, 5, crate::rules::variant::RABBIT_EVIL],
    names: &[],
    component_names: &["brown", "white", "black", "white_splotched", "gold", "salt", "evil"],
    traits: &[crate::rules::variant::Trait { variant: crate::rules::variant::RABBIT_EVIL, armor: Some(8.0), damage_add: 5.0, name: Some("entity.minecraft.killer_bunny") }],
    group_baby: None,
    inherit: crate::rules::variant::Inherit::None,
};

/// `RabbitGroupData`: a pack's later members are babies with chance 1 (the draw is still made); the first one is not.
static DRAWS: crate::behaviours::spawn_draw::Draws = crate::behaviours::spawn_draw::Draws {
    group: Some(crate::spawn_finalize::Group::Ageable { should_baby: true, size: 0, chance: 1.0, variant: None }),
    list: &[],
};

pub static DEF: Def = Def {
    name: "minecraft:rabbit",
    draws: Some(&DRAWS),
    reference: None,
    variant: Some(&VARIANT),
    // The killer bunny's target goals (`Rabbit.setVariant(EVIL)`): hurt by (no alert), then the nearest player and wolf, both seen.
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::DEFAULT).when(&KILLER),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER).when(&KILLER),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params { types: &["minecraft:wolf"], ..nearest_target::Params::DEFAULT }).when(&KILLER),
    ],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&climb_on_powder_snow::MODULE, 1, &climb_on_powder_snow::Params::DEFAULT),
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 2.2, ..panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 2, &breed::Params { speed: 0.8, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.0, items: FOOD, ..tempt::Params::DEFAULT }),
        Entry::new(&avoid_entity::MODULE, 4, &PLAYER).when(&NOT_KILLER),
        Entry::new(&avoid_entity::MODULE, 4, &WOLF).when(&NOT_KILLER),
        Entry::new(&avoid_entity::MODULE, 4, &MONSTERS).when(&NOT_KILLER),
        Entry::new(&melee::MODULE, 4, &KILLER_MELEE).when(&KILLER),
        Entry::new(&raid_garden::MODULE, 5, &raid_garden::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 6, &stroll::Params { speed: 0.6, ..stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 11, &look_at_player::Params { range: 10.0, ..look_at_player::Params::DEFAULT }),
    ],
    locomotion: &[(Medium::Ground, Entry::new(&hop::MODULE, 0, &hop::Params::RABBIT))],
    spawn: Some(crate::spawning::SpawnDef { rule: crate::spawning::Rule::Bright { on: "minecraft:rabbits_spawnable_on" }, ..crate::spawning::SpawnDef::ANIMAL }),
    despawn: Some(crate::despawn::DespawnDef::KEPT),
    ..Def::EMPTY
};
