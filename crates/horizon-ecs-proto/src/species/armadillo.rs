//! `Armadillo` goals (`mobs/armadillo.rs:56-78`): the panic of the environment only (`ArmadilloPanic`, 1), the rolling (2), and the
//! breed (3), the tempt (4, spider eyes) and the baby following an adult (4) WHEN IDLE (`armadillo:idle`: the `Idle` wrapper).
use crate::behaviours::{ambient_sound, armadillo_state, baby_follow_adult, breed, float, look_at_player, panic, random_look, rolled, stroll, tempt, walk_to_look, Cond, Entry};
use crate::gift_drop::{GiftDrop, GiftSound, Periodic, PERIODIC_GIFT};
use crate::rules::RuleEntry;
use crate::species::Def;

static IDLE: Cond = Cond::Is("armadillo:idle");
const FOOD: &[&str] = &["minecraft:spider_eye"];

/// `Armadillo.customServerAiStep`: a scute every `6000 + nextInt(6000)` ticks (`pickNextScuteDropTime`), at any age.
/// `Armadillo.isScared` (`!= IDLE` in the jar: rolling, scared and unrolling all count): silent.
fn armadillo_ambient(w: &mut bevy_ecs::prelude::World, e: bevy_ecs::prelude::Entity) -> &'static str {
    if armadillo_state::of(w, e) != armadillo_state::IDLE { "" } else { "entity.armadillo.ambient" }
}

/// `Armadillo.getHurtSound`: `hurt_reduced` while not IDLE (`isScared`), else the declared hurt voice.
fn armadillo_hurt(w: &bevy_ecs::prelude::World, e: bevy_ecs::prelude::Entity) -> Option<&'static str> {
    (armadillo_state::of(w, e) != armadillo_state::IDLE).then_some("entity.armadillo.hurt_reduced")
}

static SHED: GiftDrop = GiftDrop { table: "minecraft:gameplay/armadillo_shed", sound: Some(GiftSound { name: "minecraft:entity.armadillo.scute_drop", volume: 1.0 }), game_event: Some("minecraft:entity_place"), needs_mob_drops: true };

pub static DEF: Def = Def {
    spawn: Some(crate::spawning::SpawnDef { rule: crate::spawning::Rule::Bright { on: "minecraft:armadillo_spawnable_on" }, ..crate::spawning::SpawnDef::ANIMAL }),
    name: "minecraft:armadillo",
    hurt: Some(armadillo_hurt),
    goals: &[
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&panic::MODULE, 1, &panic::Params { environmental: true, ..panic::Params::DEFAULT }),
        Entry::new(&rolled::MODULE, 2, &rolled::Params::DEFAULT),
        Entry::new(&breed::MODULE, 3, &breed::Params { speed: 1.0, foods: FOOD, ..breed::Params::DEFAULT }).when(&IDLE),
        Entry::new(&tempt::MODULE, 4, &tempt::Params { speed: 1.25, items: FOOD, ..tempt::Params::DEFAULT }).when(&IDLE),
        Entry::new(&baby_follow_adult::MODULE, 4, &baby_follow_adult::Params::DEFAULT).when(&IDLE),
        Entry::new(&walk_to_look::MODULE, 4, &walk_to_look::Params { speed: 1.0, weight: 1, total: 3 }).when(&IDLE),
        Entry::new(&stroll::MODULE, 5, &stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 6.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 7, &random_look::Params { interval: Some((150, 250)), ..random_look::Params::DEFAULT }),
        Entry::new(&ambient_sound::MODULE, 8, &ambient_sound::Params { interval: Some(120), choose: Some(armadillo_ambient), ..ambient_sound::Params::DEFAULT }),
    ],
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 1, max: 3 }), RuleEntry::new(&PERIODIC_GIFT, &Periodic { base: 6000, range: 6000, gift: &SHED }), RuleEntry::new(&crate::behaviours::brush::RULE, &())],
    ..Def::EMPTY
};
