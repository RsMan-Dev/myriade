//! `Drowned` (`mobs/undead.rs:96-147`): the zombie's configuration (`templates::zombie_base`) with the water goals: `DrownedGoToWater`
//! (1), the attack of priority 2 only at night or with its target in water (`drowned:ok_target`, asked again on each tick of the pursuit: `melee::Params.keep_while`), `DrownedGoToBeach` (5), `DrownedSwimUp` (6), the player as a target
//! only at night or in water (`drowned:player_ok`), the iron golem, the wandering trader and the axolotl (3), a baby turtle (5).
//! The trident attack (priority 2, `ranged_attack::TRIDENT`) and `BreakDoor` come from the base and the shots' port.
//! Its swim: amphibious navigation, `DrownedMoveControl` (the marker `WantsToSwim` makes it swim, else the ground's control), the push
//! 0.01 only under water when it wants to.
use super::{templates::zombie_base, Change, Def, Section};
use crate::behaviours::{drowned_go_to_beach, drowned_go_to_water, drowned_swim_up, melee, nearest_target, ranged_attack, spear_use, try_find_water, Cap, Cond, Entry, ambient_sound};
use crate::locomotion::{swim, Medium};

static OK_TARGET: Cond = Cond::Is("drowned:ok_target");
static PLAYER_OK: Cond = Cond::Is("drowned:player_ok");

fn drowned_ambient(w: &mut bevy_ecs::prelude::World, e: bevy_ecs::prelude::Entity) -> &'static str {
    if w.get::<crate::cow::FluidState>(e).is_some_and(|f| f.in_water) { "entity.drowned.ambient_water" } else { "entity.drowned.ambient" }
}

pub static DEF: Def = Def {
    load_attrs: zombie_base::LOAD_ATTRS,
    name: "minecraft:drowned",
    reference: Some(&zombie_base::DEF),
    changes: &[
        Change::Add(Section::Goals, Entry::new(&ambient_sound::MODULE, 14, &ambient_sound::Params { interval: Some(80), choose: Some(drowned_ambient), ..ambient_sound::Params::DEFAULT })),
        Change::Remove { module: &melee::MODULE, nth: 0 },
        // Drowned.addBehaviourGoals does not call Zombie's: no SpearUseGoal (checked in the jar).
        Change::Remove { module: &spear_use::MODULE, nth: 0 },
        // Nor Zombie.addBehaviourGoals' MoveThroughVillage (the Mob's drowned has none).
        Change::Remove { module: &crate::behaviours::move_through_village::MODULE, nth: 0 },
        // Drowned.addBehaviourGoals re-lists the player, villager and golem targets itself: the base's three go (else each is asked twice,
        // twice as often as the Mob's goal).
        Change::Remove { module: &nearest_target::MODULE, nth: 0 },
        Change::Remove { module: &nearest_target::MODULE, nth: 0 },
        Change::Remove { module: &nearest_target::MODULE, nth: 0 },
        Change::Add(Section::Goals, Entry::new(&try_find_water::MODULE, 1, &try_find_water::DROWNED)),
        Change::Add(Section::Goals, Entry::new(&drowned_go_to_water::MODULE, 1, &drowned_go_to_water::Params::DEFAULT)),
        Change::Add(Section::Goals, Entry::new(&melee::MODULE, 2, &melee::Params { keep_while: Some(&OK_TARGET), ..melee::Params::DEFAULT }).when(&OK_TARGET)),
        Change::Add(Section::Goals, Entry::new(&ranged_attack::MODULE, 2, &ranged_attack::Params::TRIDENT)),
        Change::Add(Section::Goals, Entry::new(&drowned_go_to_beach::MODULE, 5, &drowned_go_to_beach::Params::DEFAULT)),
        Change::Add(Section::Goals, Entry::new(&drowned_swim_up::MODULE, 6, &drowned_swim_up::Params::DEFAULT)),
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER).when(&PLAYER_OK)),
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:villager", "minecraft:wandering_trader"], must_see: false, ..nearest_target::Params::DEFAULT })),
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:iron_golem"], ..nearest_target::Params::DEFAULT })),
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 3, &nearest_target::Params { types: &["minecraft:axolotl"], ..nearest_target::Params::DEFAULT })),
        Change::Add(Section::Targets, Entry::new(&nearest_target::MODULE, 5, &nearest_target::Params { types: &["minecraft:turtle"], predicate: Some("baby"), interval: 10, ..nearest_target::Params::DEFAULT })),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion]),
    locomotion: &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, amphibious: Some(false), control: swim::Control::Drowned, sinks: false, ..swim::Params::DEFAULT }))],
    spawn: Some(crate::spawning::SpawnDef { category: crate::spawning::Category::Monster, place: crate::spawning::Place::InWater, rule: crate::spawning::Rule::Drowned, cluster: 4, far: false, peaceful: false, liquid: true, despawn: crate::despawn::DespawnDef::MOB }), despawn: Some(crate::despawn::DespawnDef::MOB),
    rules: &[
        crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 5, max: 5 }),
        crate::rules::RuleEntry::new(&crate::loot::BABY_DROPS, &crate::loot::BabyDrops::ZOMBIE),
        crate::rules::RuleEntry::new(&crate::reinforcements::RULE, &crate::reinforcements::Params::ZOMBIE),
        crate::rules::RuleEntry::new(&crate::spawn_gear::RULE, &crate::spawn_gear::DROWNED),
    ],
    not_in_peaceful: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
