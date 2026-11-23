//! Left out, for the selector holds 16 entries (goals and targets together) and the wolf has 20 (BESOIN DU CADRE): the avoidance of llamas
//! (`avoid_entity` 3 + `forget_target`: no llama in the registry yet), the baby turtle and the skeletons as prey.
//! `Wolf.registerGoals` as far as the frame has the modules: float, sit when ordered, the leap and the bite, follow the owner, stroll and
//! look; the targets of a tame wolf (what hurt its owner, what its owner hit, what hurt it) and the wild wolf's prey. Taming is the rule
//! `TAME` (a bone, one chance in three, 40 health). Missing, to be complete: the anger (`NeutralMob`: it bites the player that hit it and
//! the ones the anger names), begging, the fear of llamas, the panic that teleports a far pet to its owner, breeding with meat, the
//! shake, the collar colour and its dyes, the healing by food, the variants, the howl, armour.
use super::Def;
use crate::behaviours::avoid_entity::{self, Avoid};
use crate::behaviours::{neutral_anger, forget_target, beg, breed, float, follow_owner, hurt_by, leap_at_target, look_at_player, melee, nearest_target, owner_target, panic, random_look, sit_when_ordered, stroll, Cap, Cond, Entry};
use crate::rules::RuleEntry;
use crate::tame::{Owner, TameParams, TAME};
use crate::cow::{Health, Rng};
use crate::behaviours::ambient_sound;
use bevy_ecs::prelude::*;

/// `wolf_food`: the `meat` tag, the fish, the rabbit stew.
const FOOD: &[&str] = &["minecraft:beef", "minecraft:chicken", "minecraft:cooked_beef", "minecraft:cooked_chicken", "minecraft:cooked_mutton", "minecraft:cooked_porkchop", "minecraft:cooked_rabbit", "minecraft:mutton", "minecraft:porkchop", "minecraft:rabbit", "minecraft:rotten_flesh", "minecraft:cod", "minecraft:cooked_cod", "minecraft:salmon", "minecraft:cooked_salmon", "minecraft:tropical_fish", "minecraft:pufferfish", "minecraft:rabbit_stew"];
static AVOID_LLAMAS: avoid_entity::Params = avoid_entity::Params { what: Avoid::Named(&["minecraft:llama", "minecraft:trader_llama"]), max_dist: 24.0, walk: 1.5, sprint: 1.5, target_roll: Some(forget_target::strength_roll), ..avoid_entity::Params::DEFAULT };
const NEVER: &[&str] = &["minecraft:creeper", "minecraft:ghast"];
const SPARED_TAME: &[&str] = &["minecraft:cat", "minecraft:parrot", "minecraft:horse", "minecraft:donkey", "minecraft:mule", "minecraft:llama"];
static ANGRY: Cond = Cond::Is("neutral:angry");
static WILD: Cond = Cond::Not(&Cond::Is("horizon:is_tame"));

/// `Wolf.getAmbientSound`: a growl while angry; else one time in three the whine (tame and under 20 health) or the pant; else the bark.
/// The variant's own sound set is not in the ECS, so the default names stand.
fn ambient(w: &mut World, e: Entity) -> &'static str {
    if w.get::<neutral_anger::Angry>(e).is_some() {
        return "entity.wolf.growl";
    }
    if w.get_mut::<Rng>(e).unwrap().next_u32() % 3 != 0 {
        return "entity.wolf.ambient";
    }
    let tame = w.get::<Owner>(e).is_some_and(|o| o.0.is_some());
    if tame && w.get::<Health>(e).is_some_and(|h| h.0 < 20.0) {
        "entity.wolf.whine"
    } else {
        "entity.wolf.pant"
    }
}

pub static DEF: Def = Def {
    load_attrs: &[crate::attr_mod::LoadAttr { name: "max_health", base: None, when: crate::attr_mod::LoadWhen::Unowned }],
    name: "minecraft:wolf",
    sound_on_breed: true,
    sounds: Some(&["minecraft:angry", "minecraft:big", "minecraft:classic", "minecraft:cute", "minecraft:grumpy", "minecraft:puglin", "minecraft:sad"]),
    variant: Some(&super::variants::WOLF),
    reference: None,
    targets: &[
        Entry::new(&neutral_anger::MODULE, 255, &neutral_anger::Params::DEFAULT),
        Entry::new(&owner_target::MODULE, 1, &owner_target::Params { never: NEVER, spare_when_tame: SPARED_TAME, spare_same_owner: true, ..owner_target::Params::HURT_BY }),
        Entry::new(&owner_target::MODULE, 2, &owner_target::Params { never: NEVER, spare_when_tame: SPARED_TAME, spare_same_owner: true, ..owner_target::Params::HURT }),
        Entry::new(&hurt_by::MODULE, 3, &hurt_by::Params { alert: true, spare_owner: true, ..hurt_by::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 4, &nearest_target::Params::PLAYER).when(&ANGRY),
        Entry::new(&nearest_target::MODULE, 5, &nearest_target::Params { types: &["minecraft:sheep", "minecraft:rabbit", "minecraft:fox"], must_see: false, ..nearest_target::Params::DEFAULT }).when(&WILD),
        Entry::new(&nearest_target::MODULE, 6, &nearest_target::Params { types: &["minecraft:turtle"], predicate: Some("baby_on_land"), must_see: false, ..nearest_target::Params::DEFAULT }).when(&WILD),
        Entry::new(&nearest_target::MODULE, 7, &nearest_target::Params { types: &["minecraft:skeleton", "minecraft:stray", "minecraft:bogged", "minecraft:wither_skeleton"], must_see: false, ..nearest_target::Params::DEFAULT }),
    ],
    goals: &[
        Entry::new(&float::MODULE, 1, &float::Params::DEFAULT),
        Entry::new(&panic::MODULE, 1, &panic::Params { speed: 1.5, environmental: true, ..panic::Params::DEFAULT }),
        Entry::new(&sit_when_ordered::MODULE, 2, &sit_when_ordered::Params { head_x: 20.0, refuses_mate: true, stands_when_hurt: true }),
        Entry::new(&avoid_entity::MODULE, 3, &AVOID_LLAMAS).when(&WILD),
        Entry::new(&forget_target::MODULE, 3, &forget_target::Params::DEFAULT),
        Entry::new(&leap_at_target::MODULE, 4, &leap_at_target::Params { yd: 0.4 }),
        Entry::new(&melee::MODULE, 5, &melee::Params { follow_unseen: true, ..melee::Params::DEFAULT }),
        Entry::new(&follow_owner::MODULE, 6, &follow_owner::Params::DEFAULT),
        Entry::new(&breed::MODULE, 7, &breed::Params { speed: 1.0, foods: FOOD, ..breed::Params::DEFAULT }),
        Entry::new(&stroll::MODULE, 8, &stroll::Params::DEFAULT),
        Entry::new(&beg::MODULE, 9, &beg::Params { foods: FOOD, ..beg::Params::DEFAULT }),
        Entry::new(&crate::behaviours::shake_off_water::MODULE, 12, &crate::behaviours::shake_off_water::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 10, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 10, &random_look::Params::DEFAULT),
        Entry::new(&ambient_sound::MODULE, 11, &ambient_sound::Params { interval: Some(80), choose: Some(ambient), ..ambient_sound::Params::DEFAULT }),
    ],
    changes: &[],
    rules: &[RuleEntry::new(&crate::dye::DYE,&crate::dye::DyeParams::WOLF), RuleEntry::new(&crate::body_armor::WEAR, &crate::body_armor::WearParams::WOLF), RuleEntry::new(&TAME, &TameParams::WOLF),RuleEntry::new(&crate::credit::CREDITS_OWNER, &())],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::Tamable]),
    brain: crate::brain::BrainDef::EMPTY,
    locomotion: &[],
    body_wear: true,
    ..Def::EMPTY
};
