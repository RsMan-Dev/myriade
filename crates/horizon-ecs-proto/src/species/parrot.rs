//! `Parrot` (`mobs/tame.rs:109-119`, `register_parrot`): a flier (`FlyingMoveControl(10, false)`, `FlyingPathNavigation`) that wanders in
//! the air, follows its owner once tame, sits when ordered, panics when hurt, looks at players, and follows a mob (`FollowMobGoal`).
//! The landing on the owner's shoulder: `land_on_shoulder` (host output `HostOut::shoulder`). Left out: the taming by seeds (the `TAME` rule: the Mob does it
//! elsewhere, not read here). DOUBT: `TamePanic` does not panic while the tame parrot sits: the `panic` module has no such option.
use super::Def;
use crate::behaviours::{ambient_sound, fly_stroll, float, follow_mob, follow_owner, look_at_player, panic, sit_when_ordered, land_on_shoulder, Cap, Entry};
use crate::locomotion::{fly, Medium};

/// The imitations of `Parrot.MOB_SOUND_MAP` (a `HashMap` keyed by `EntityType`, whose order is not fixed in vanilla), one per key of the
/// `<clinit>` lambda, in its order; happy ghast gives `SoundEvents.EMPTY`, a sound with no file, so `""`.
const IMITATED: [&str; 42] = [
    "entity.parrot.imitate.blaze", "entity.parrot.imitate.bogged", "entity.parrot.imitate.breeze", "entity.parrot.imitate.camel_husk",
    "entity.parrot.imitate.spider", "entity.parrot.imitate.creaking", "entity.parrot.imitate.creeper", "entity.parrot.imitate.drowned",
    "entity.parrot.imitate.elder_guardian", "entity.parrot.imitate.ender_dragon", "entity.parrot.imitate.endermite", "entity.parrot.imitate.evoker",
    "entity.parrot.imitate.ghast", "", "entity.parrot.imitate.guardian", "entity.parrot.imitate.hoglin",
    "entity.parrot.imitate.husk", "entity.parrot.imitate.illusioner", "entity.parrot.imitate.magma_cube", "entity.parrot.imitate.parched",
    "entity.parrot.imitate.phantom", "entity.parrot.imitate.piglin", "entity.parrot.imitate.piglin_brute", "entity.parrot.imitate.pillager",
    "entity.parrot.imitate.ravager", "entity.parrot.imitate.shulker", "entity.parrot.imitate.silverfish", "entity.parrot.imitate.skeleton",
    "entity.parrot.imitate.slime", "entity.parrot.imitate.spider", "entity.parrot.imitate.stray", "entity.parrot.imitate.vex",
    "entity.parrot.imitate.vindicator", "entity.parrot.imitate.warden", "entity.parrot.imitate.witch", "entity.parrot.imitate.wither",
    "entity.parrot.imitate.wither_skeleton", "entity.parrot.imitate.zoglin", "entity.parrot.imitate.zombie", "entity.parrot.imitate.zombie_horse",
    "entity.parrot.imitate.zombie_nautilus", "entity.parrot.imitate.zombie_villager",
];

/// `Parrot.getAmbient`: not Peaceful, one time in a thousand an imitation of one of the 42 keys; else the ambient. The draws come from the
/// animal's generator, not the level's.
fn parrot_ambient(w: &mut bevy_ecs::prelude::World, e: bevy_ecs::prelude::Entity) -> &'static str {
    if crate::effects::difficulty(w) == 0 {
        return "entity.parrot.ambient";
    }
    crate::goals::with_rng(w, e, |r| if r.next_int(1000) == 0 { IMITATED[r.next_int(IMITATED.len() as i32) as usize] } else { "entity.parrot.ambient" })
}

pub static DEF: Def = Def {
    name: "minecraft:parrot",
    variant: Some(&super::variants::PARROT),
    // `register_parrot`: the panic and the float (0), the look (1), the sit and the owner (2), the wander (2), the mob (3).
    goals: &[
        Entry::new(&panic::MODULE, 0, &panic::Params { speed: 1.25, ..panic::Params::DEFAULT }),
        Entry::new(&float::MODULE, 0, &float::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 1, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&sit_when_ordered::MODULE, 2, &sit_when_ordered::Params { stands_when_hurt: true, ..sit_when_ordered::Params::DEFAULT }),
        Entry::new(&follow_owner::MODULE, 2, &follow_owner::Params { speed: 1.0, start: 5.0, stop: 1.0 }),
        Entry::new(&fly_stroll::MODULE, 2, &fly_stroll::Params::DEFAULT),
        Entry::new(&land_on_shoulder::MODULE, 3, &land_on_shoulder::Params::DEFAULT),
        Entry::new(&follow_mob::MODULE, 3, &follow_mob::Params { speed: 1.0, stop: 3.0, area: 7.0, ..follow_mob::Params::DEFAULT }),
        Entry::new(&ambient_sound::MODULE, 4, &ambient_sound::Params { interval: Some(120), choose: Some(parrot_ambient), ..ambient_sound::Params::DEFAULT }),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Tamable, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { max_turn: 10.0, hovers: false, glide: 0.6, ..fly::Params::DEFAULT }))],
    spawn: Some(crate::spawning::SpawnDef { rule: crate::spawning::Rule::Bright { on: "minecraft:parrots_spawnable_on" }, ..crate::spawning::SpawnDef::ANIMAL }),
    despawn: Some(crate::despawn::DespawnDef::KEPT),
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 1, max: 3 }), crate::rules::RuleEntry::new(&crate::rules::NO_FALL, &())],
    ..Def::EMPTY
};
