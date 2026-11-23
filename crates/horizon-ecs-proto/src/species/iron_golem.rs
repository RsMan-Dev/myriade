//! `IronGolem.registerGoals` as far as the frame has the modules: the blow, closing in on the target, going back to the village and
//! strolling in it (`village_walk`, on the registry of the points of interest), looking; the offer of the flower to a child villager
//! (`offer_flower`), `DefendVillageTargetGoal` (event-driven, the reputation read from the villager's gossip), the target of what hurt it.
//! The anger at a player (`neutral_anger`, at 255, and the player instance that only searches while it runs). Missing: the walk toward a villager who wants a golem, the cracks.
use super::Def;
use crate::behaviours::{hurt_by, look_at_player, melee, move_towards_target, nearest_target, neutral_anger, random_look, village_walk, Entry};
use crate::targeting::Who;
use bevy_ecs::prelude::{Entity, World};

/// `IronGolem.angry_at_player`: the golem is angry at a player (the anger holds it or counts down for one).
fn angry_at_player(w: &World, e: Entity) -> bool {
    matches!(neutral_anger::anger_at(w, e), Some(Who::Player(_)))
}

pub static DEF: Def = Def {
    name: "minecraft:iron_golem",
    targets: &[
        Entry::new(&crate::behaviours::defend_village_target::MODULE, 1, &crate::behaviours::defend_village_target::Params::DEFAULT),
        // `IronGolem.canAttack` refuses the creeper: a creeper that hurts it is never fought back.
        Entry::new(&hurt_by::MODULE, 2, &hurt_by::Params { ignoring: &["minecraft:creeper"], ..hurt_by::Params::DEFAULT }),
        // `golem_prey`: the monsters but the creeper, not seen needed, one ask in 5.
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { hostile: true, except: &["minecraft:creeper"], must_see: false, interval: 5, ..nearest_target::Params::DEFAULT }),
        // `NearestAttackableTargetGoal(Player)` with `angry_at_player`: only while the golem is angry at a player.
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { players: true, may_search: Some(angry_at_player), ..nearest_target::Params::DEFAULT }),
        Entry::new(&crate::behaviours::reset_universal_anger::MODULE, 4, &crate::behaviours::reset_universal_anger::Params::DEFAULT),
    ],
    goals: &[
        Entry::new(&neutral_anger::MODULE, 255, &neutral_anger::Params::DEFAULT),
        Entry::new(&crate::behaviours::offer_flower::MODULE, 5, &crate::behaviours::offer_flower::Params { candidates: &["minecraft:villager", "minecraft:copper_golem"], ..crate::behaviours::offer_flower::Params::DEFAULT }),
        Entry::new(&melee::MODULE, 1, &melee::Params::DEFAULT),
        Entry::new(&crate::behaviours::target_toss::MODULE, 1, &crate::behaviours::target_toss::Params::DEFAULT),
        Entry::new(&move_towards_target::MODULE, 2, &move_towards_target::Params::DEFAULT),
        Entry::new(&village_walk::MODULE, 2, &village_walk::Params::BACK),
        Entry::new(&village_walk::MODULE, 4, &village_walk::Params::IN_VILLAGE),
        Entry::new(&look_at_player::MODULE, 7, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::give_item::GIVE, &crate::give_item::GiveParams::IRON_REPAIR), crate::rules::RuleEntry::new(&crate::behaviours::contact_damage::VICTIM, &()), crate::rules::RuleEntry::new(&crate::raids::BAIT, &()), crate::rules::RuleEntry::new(&crate::behaviours::crack_sound::RULE, &crate::behaviours::crack_sound::Params { sound: "entity.iron_golem.damage", levels: [0.75, 0.5, 0.25] })],
    ..Def::EMPTY
};
