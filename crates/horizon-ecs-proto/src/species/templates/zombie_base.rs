//! The zombie's configuration without its sun burn (`Zombie.addBehaviourGoals` and `registerGoals`, `mobs/undead.rs:36-70`): the base
//! of the husk and of the zombie, which adds the burn. The villagers, the wandering trader, the iron golem and the baby turtle (targets)
//! wait for their species to be in the registry.
use crate::behaviours::{pick_up_gear, spear_use, attack_turtle_egg, break_door, hurt_by, look_at_player, melee, nearest_target, random_look, stroll, Entry};
use crate::species::Def;

pub static DEF: Def = Def {
    name: "template:zombie_base",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params { alert: true, alert_except: &["minecraft:zombified_piglin"], ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:villager", "minecraft:wandering_trader"], must_see: false, ..nearest_target::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&break_door::MODULE, 1, &break_door::Params::ZOMBIE),
        Entry::new(&spear_use::MODULE, 2, &spear_use::Params::DEFAULT),
        Entry::new(&melee::MODULE, 3, &melee::Params::DEFAULT),
        Entry::new(&attack_turtle_egg::MODULE, 4, &attack_turtle_egg::Params::DEFAULT),
        Entry::new(&crate::behaviours::move_through_village::MODULE, 6, &crate::behaviours::move_through_village::Params::ZOMBIE),
        Entry::new(&stroll::MODULE, 7, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 8, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 8, &random_look::Params::DEFAULT),
        // The looting block of `Mob.aiStep` (a rule): a zombie leaves glow ink alone (`Zombie.wantsToPickUp`).
        Entry::new(&pick_up_gear::MODULE, 255, &pick_up_gear::Params { refuses: &["minecraft:glow_ink_sac"], ..pick_up_gear::Params::DEFAULT }),
    ],
    spawn: Some(crate::spawning::SpawnDef::MONSTER), despawn: Some(crate::despawn::DespawnDef::MOB),
    // `Zombie.doHurtTarget`: a burning zombie with an empty hand sets what it hits on fire.
    rules: &[
        crate::rules::RuleEntry::new(&crate::hostile::IGNITE_ON_HIT, &crate::hostile::ZOMBIE_FIRE),
        crate::rules::RuleEntry::new(&crate::spawn_gear::RULE, &crate::spawn_gear::ZOMBIE),
    ],
    ..Def::EMPTY
};
