//! `Wolf.registerGoals` as far as the frame has the modules: float, sit when ordered, the leap and the bite, follow the owner, stroll and
//! look; the targets of a tame wolf (what hurt its owner, what its owner hit, what hurt it) and the wild wolf's prey. Taming is the rule
//! `TAME` (a bone, one chance in three, 40 health). Missing, to be complete: the anger (`NeutralMob`: it bites the player that hit it and
//! the ones the anger names), begging, the fear of llamas, the panic that teleports a far pet to its owner, breeding with meat, the
//! shake, the collar colour and its dyes, the healing by food, the variants, the howl, armour.
use super::Def;
use crate::behaviours::{follow_owner, hurt_by, leap_at_target, look_at_player, melee, nearest_target, owner_target, panic, random_look, sit_when_ordered, stroll, Cap, Cond, Entry};
use crate::rules::RuleEntry;
use crate::tame::{TameParams, TAME};

const NEVER: &[&str] = &["minecraft:creeper", "minecraft:ghast"];
const SPARED_TAME: &[&str] = &["minecraft:cat", "minecraft:parrot", "minecraft:horse", "minecraft:donkey", "minecraft:mule", "minecraft:llama"];
static WILD: Cond = Cond::Not(&Cond::Is("horizon:is_tame"));

pub static DEF: Def = Def {
    name: "minecraft:wolf",
    reference: None,
    targets: &[
        Entry::new(&owner_target::MODULE, 1, &owner_target::Params { never: NEVER, spare_when_tame: SPARED_TAME, spare_same_owner: true, ..owner_target::Params::HURT_BY }),
        Entry::new(&owner_target::MODULE, 2, &owner_target::Params { never: NEVER, spare_when_tame: SPARED_TAME, spare_same_owner: true, ..owner_target::Params::HURT }),
        Entry::new(&hurt_by::MODULE, 3, &hurt_by::Params { alert: true, spare_owner: true, ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 5, &nearest_target::Params { types: &["minecraft:sheep", "minecraft:rabbit", "minecraft:fox"], must_see: false, ..nearest_target::Params::DEFAULT }).when(&WILD),
    ],
    goals: &[
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 1.5, ..panic::Params::DEFAULT }),
        Entry::new(&sit_when_ordered::MODULE, 2, &()),
        Entry::new(&leap_at_target::MODULE, 4, &leap_at_target::Params { yd: 0.4 }),
        Entry::new(&melee::MODULE, 5, &melee::Params::DEFAULT),
        Entry::new(&follow_owner::MODULE, 6, &follow_owner::Params::DEFAULT),
        Entry::new(&stroll::MODULE, 8, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 10, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 10, &random_look::Params::DEFAULT),
    ],
    changes: &[],
    rules: &[RuleEntry::new(&TAME, &TameParams::WOLF)],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Tamable]),
    brain: crate::brain::BrainDef::EMPTY,
    locomotion: &[],
};
