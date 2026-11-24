//! `Bee`: flies by a path (`FlyingMoveControl(20, hovers)`, `FlyingPathNavigation`), the cycle of the nectar (`behaviours/bee.rs`: enter the hive
//! 1, validate the hive and the flower 3, pollinate 4, locate and go to the hive 5, the known flower 6, crops 7, wander 8 at one chance in 10 and
//! leaning towards a far hive), the sting and the anger (`BeeAttackGoal` 0 while angry and unstung; hurt by 1 alerting the bees; the player it is
//! angry at 2; `neutral_anger` keeps the anger), breeding and following the parent, tempted by flowers. The calm goals run only while it is not
//! angry (`bee:calm`: `BaseBeeGoal`). The flowers of the food are the common ones (`#flowers`).
use super::Def;
use crate::behaviours::{bee, breed, float, fly_stroll, follow_parent, hurt_by, melee, nearest_target, neutral_anger, tempt, Cap, Cond, Entry};
use crate::locomotion::{fly, Medium};

static CALM: Cond = Cond::Is("bee:calm");
static CAN_TARGET: Cond = Cond::Is("bee:can_target");
const FLOWERS: &[&str] = &[
    "minecraft:dandelion", "minecraft:poppy", "minecraft:blue_orchid", "minecraft:allium", "minecraft:azure_bluet", "minecraft:red_tulip", "minecraft:orange_tulip",
    "minecraft:white_tulip", "minecraft:pink_tulip", "minecraft:oxeye_daisy", "minecraft:cornflower", "minecraft:lily_of_the_valley", "minecraft:wither_rose",
    "minecraft:sunflower", "minecraft:lilac", "minecraft:rose_bush", "minecraft:peony", "minecraft:torchflower", "minecraft:pitcher_plant",
];

pub static DEF: Def = Def {
    name: "minecraft:bee",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, alert_types: &["minecraft:bee"], ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params { interval: 10, ..nearest_target::Params::PLAYER }).when(&CAN_TARGET),
    ],
    goals: &[
        Entry::new(&crate::behaviours::move_emission::MODULE, 0, &crate::behaviours::move_emission::Params { flap: crate::behaviours::move_emission::Flap::Every { period: 2, flying_only: true, offset: false }, ..crate::behaviours::move_emission::Params::DEFAULT }),
        Entry::new(&neutral_anger::MODULE, 255, &neutral_anger::Params::DEFAULT),
        Entry::new(&melee::MODULE, 0, &melee::Params { speed: 1.4, follow_unseen: true, ..melee::Params::DEFAULT }).when(&CAN_TARGET),
        Entry::new(&bee::ENTER_HIVE, 1, &bee::Params::DEFAULT).when(&CALM),
        Entry::new(&breed::MODULE, 2, &breed::Params { foods: FLOWERS, ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 3, &tempt::Params { speed: 1.25, items: FLOWERS, ..tempt::Params::DEFAULT }),
        Entry::new(&bee::VALIDATE_HIVE, 3, &bee::Params::DEFAULT).when(&CALM),
        Entry::new(&bee::VALIDATE_FLOWER, 3, &bee::Params::DEFAULT).when(&CALM),
        Entry::new(&bee::POLLINATE, 4, &bee::Params::DEFAULT).when(&CALM),
        Entry::new(&follow_parent::MODULE, 5, &follow_parent::Params { speed: 1.25 }),
        Entry::new(&bee::LOCATE_HIVE, 5, &bee::Params::DEFAULT).when(&CALM),
        Entry::new(&bee::GO_TO_HIVE, 5, &bee::Params::DEFAULT).when(&CALM),
        Entry::new(&bee::GO_TO_KNOWN_FLOWER, 6, &bee::Params::DEFAULT).when(&CALM),
        Entry::new(&bee::GROW_CROP, 7, &bee::Params::DEFAULT).when(&CALM),
        Entry::new(&fly_stroll::MODULE, 8, &fly_stroll::Params { interval: 10, toward: Some(bee::toward_far_hive), ..fly_stroll::Params::DEFAULT }),
        Entry::new(&float::MODULE, 9, &float::Params::DEFAULT),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::IgnoresBerryBush]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Path, max_turn: 20.0, hovers: true, ..fly::Params::DEFAULT }))],

    rules: &[crate::rules::RuleEntry::new(&crate::rules::NO_FALL, &())],
    ..Def::EMPTY
};
