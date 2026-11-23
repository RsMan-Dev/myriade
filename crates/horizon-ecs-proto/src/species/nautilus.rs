//! `Nautilus` (`mobs/nautilus.rs:39-71`), the swimmer of the charge: `SmoothSwimming(85, 10, 0.011, 0, true)`; goals: the panic at 1.6
//! (0), the breed at 0.4 (1), the tempt at 1.3 (2), the charge at 0.6 (3), the swimming stroll every tick (4: `fish_swim`).
//! The foods are the data tag `nautilus_food` (the fishes and four buckets), listed by name. The tempt stops 3.5 blocks from the
//! player whatever the age (the `Mob` uses 2.5 for a baby: `suivi/ecs-ecarts.md`). The target is `nearest_target` with the attack
//! cooldown and the anger memory of `findNearestValidAttackTarget`: the wet `nautilus_hostiles` (`EntityTypeTags`) first, the
//! `ANGRY_AT` attacker (a player too) before them.
use super::Def;
use crate::behaviours::breed::Love;
use crate::behaviours::spawn_draw::{Draw, Draws, Kind, Store, When};
use crate::behaviours::{animal_panic, breed, charge_attack, fish_swim, nearest_target, tempt, Cap, Entry};
use crate::cow::{FluidState, GrowAt};
use crate::locomotion::{swim, Medium};
use crate::rules::spawn_draw::Roll;
use crate::tame::Owner;
use bevy_ecs::prelude::{Entity, World};

/// `tags/item/nautilus_food.json`: `#fishes` and `#nautilus_bucket_food`.
pub const FOOD: &[&str] = &[
    "minecraft:cod", "minecraft:cooked_cod", "minecraft:salmon", "minecraft:cooked_salmon", "minecraft:pufferfish", "minecraft:tropical_fish",
    "minecraft:pufferfish_bucket", "minecraft:cod_bucket", "minecraft:salmon_bucket", "minecraft:tropical_fish_bucket",
];
/// `SmoothSwimmingMoveControl(85, 10, 0.011, 0, true)`, shared with the zombie nautilus.
pub const LOCOMOTION: &[crate::locomotion::ModeEntry] = &[(Medium::Water, Entry::new(&swim::MODULE, 0, &swim::Params { needs_water: false, breach: false, control: swim::Control::Smooth { max_turn_x: 85.0, max_turn_y: 10.0, in_water: 0.011, outside: 0.0, gravity: true }, push: swim::Push::Speed, sinks: false, ..swim::Params::DEFAULT }))];

pub static DEF: Def = Def {
    spawn: Some(crate::spawning::SpawnDef { category: crate::spawning::Category::WaterCreature, place: crate::spawning::Place::InWater, rule: crate::spawning::Rule::Nautilus, peaceful: true, liquid: true, ..crate::spawning::SpawnDef::MONSTER }),
    name: "minecraft:nautilus",
    goals: &[
        Entry::new(&animal_panic::MODULE, 0, &animal_panic::Params { speed: 1.6, ..animal_panic::Params::DEFAULT }),
        Entry::new(&breed::MODULE, 1, &breed::Params { speed: 0.4, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&tempt::MODULE, 2, &tempt::Params { speed: 1.3, items: FOOD, stop: 3.5, ..tempt::Params::DEFAULT }),
        Entry::new(&charge_attack::MODULE, 3, &charge_attack::Params { speed: 0.6, ..charge_attack::Params::DEFAULT }),
        Entry::new(&fish_swim::MODULE, 4, &fish_swim::Params { speed: 1.0, interval: 1 }),
        Entry::new(&nearest_target::MODULE, 5, &nearest_target::Params { tag: Some("minecraft:nautilus_hostiles"), candidate_when: Some(in_water), attack_cooldown: Some((2400, 3600)), may_search: Some(may_search), anger_memory: true, ..nearest_target::Params::DEFAULT }),
    ],
    caps: Some(CAPS),
    locomotion: LOCOMOTION,
    rules: RULES,
    draws: Some(&DRAWS),
    ..Def::EMPTY
};

/// `NautilusAi.initMemories`: the attack target cooldown, 2400 to 3600 ticks, drawn at the spawn.
static DRAWS: Draws = Draws {
    group: None,
    list: &[Draw { when: When::Before, kind: Kind::Plain(Roll::Between { min: 2400, max: 3600 }), store: Store::Call(nearest_target::spawn_attack_cooldown) }],
};
/// `findNearestValidAttackTarget`'s first tests: not in love, in the water, not a baby, not tame.
fn may_search(w: &World, e: Entity) -> bool {
    w.get::<Love>(e).is_none() && in_water(w, e) && w.get::<GrowAt>(e).is_none() && w.get::<Owner>(e).is_none_or(|o| o.0.is_none())
}
/// A candidate is in the water (`isInWater` in the predicate of `findNearestValidAttackTarget`).
fn in_water(w: &World, c: Entity) -> bool {
    w.get::<FluidState>(c).is_some_and(|f| f.in_water)
}

/// The capabilities of `AbstractNautilus` (a `TamableAnimal` that carries a rider), shared with the zombie nautilus.
pub const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::PoisonImmune, Cap::Tamable, Cap::Rideable];
/// `AbstractNautilus.interact` (persistence) and `mobInteract`: taming by `#nautilus_taming_items`, the saddle and armor, riding, the lead.
/// The order is the order of the answers: taming and equipment before boarding.
pub const RULES: &[crate::rules::RuleEntry] = &[
    crate::rules::RuleEntry::new(&crate::keep_on_use::KEEP_ON_USE, &()),
    crate::rules::RuleEntry::new(&crate::tame::TAME, &crate::tame::TameParams::NAUTILUS),
    crate::rules::RuleEntry::new(&crate::heal_by_food::HEAL_BY_FOOD, &crate::heal_by_food::HealParams::NAUTILUS),
    crate::rules::RuleEntry::new(&crate::equipment::EQUIP, &crate::equipment::EquipParams::TAME_SADDLE_BODY),
    crate::rules::RuleEntry::new(&crate::mount::RIDE, &crate::mount::RideParams::NAUTILUS),
    crate::rules::RuleEntry::new(&crate::leash::LEASH, &crate::leash::LeashParams::DEFAULT),
    crate::rules::RuleEntry::new(&crate::leash::GATE, &crate::leash::GateParams(crate::leash::not_angry)),
    crate::rules::RuleEntry::new(&crate::cow::OWN_AIR, &crate::cow::OwnAirParams { max: 300, full_refill: false, nautilus: true }),
];
