//! `Wither` (`mobs/wither.rs`, `register`): a flying boss. Targets: the hurt-by, then the nearest non-undead creature, then the
//! nearest player. Goals: the charge at spawn, the centre head shooting at range (`ranged_attack`, skull), the two side heads
//! (`side_heads`, `nth` 0 and 1), the regeneration, the block breaking, the boss bar, the stroll and the looks. Flight is the `FlyingMoveControl` (10, no hover) of the parrot (`fly` `Control::Path`), the stroll `WaterAvoidingRandomFlyingGoal`. The hurt filter is `behaviours::phase_shield::accepts`.
use crate::behaviours::{block_breaker, boss_bar, do_nothing, hurt_by, invulnerable_charge,look_at_player, nearest_target, random_look, ranged_attack, regen, side_heads, fly_stroll, Cap, Entry};
use crate::locomotion::{fly, Medium};
use crate::projectile::Flight;
use crate::species::Def;

/// The undead the wither never picks (`undead(s)`, `#wither_friends`).
pub const UNDEAD: &[&str] = &[
    "minecraft:zombie", "minecraft:husk", "minecraft:drowned", "minecraft:zombie_villager", "minecraft:skeleton",
    "minecraft:stray", "minecraft:bogged", "minecraft:parched", "minecraft:wither_skeleton", "minecraft:zombie_horse",
    "minecraft:skeleton_horse", "minecraft:phantom", "minecraft:zombified_piglin", "minecraft:zoglin", "minecraft:wither",
];

pub static DEF: Def = Def {
    rules: &[crate::rules::RuleEntry::new(&crate::loot::XP, &crate::loot::XpReward { min: 50, max: 50 })],
    name: "minecraft:wither",
    targets: &[
        Entry::new(&hurt_by::MODULE, 1, &hurt_by::Params::DEFAULT),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params { any: true, except: UNDEAD, ..nearest_target::Params::DEFAULT }),
        Entry::new(&nearest_target::MODULE, 2, &nearest_target::Params::PLAYER),
    ],
    goals: &[
        Entry::new(&invulnerable_charge::MODULE, 0, &invulnerable_charge::Params::DEFAULT),
        Entry::new(&do_nothing::MODULE, 0, &do_nothing::Params::DEFAULT),
        Entry::new(&ranged_attack::MODULE, 2, &ranged_attack::Params { speed: 1.0, min: 40, max: 40, radius: 20.0, flight: Flight::wither_skull(false), blue: Some((0.001, Flight::wither_skull(true))), sound: Some("entity.wither.shoot"), ..ranged_attack::Params::DEFAULT }),
        Entry::new(&side_heads::MODULE, 2, &side_heads::Params::WITHER),
        Entry::new(&side_heads::MODULE, 2, &side_heads::Params::WITHER),
        Entry::new(&regen::MODULE, 0, &regen::Params::WITHER),
        Entry::new(&block_breaker::MODULE, 0, &block_breaker::Params::WITHER),
        Entry::new(&boss_bar::MODULE, 0, &boss_bar::Params::WITHER),
        Entry::new(&fly_stroll::MODULE, 5, &fly_stroll::Params::DEFAULT),
        Entry::new(&look_at_player::MODULE, 6, &look_at_player::Params { range: 8.0, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 7, &random_look::Params::DEFAULT),
    ],
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { control: fly::Control::Path, max_turn: 10.0, hovers: false, damp_y: 0.6, ..fly::Params::DEFAULT }))],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion, Cap::FireImmune, Cap::WitherImmune, Cap::FreezeImmune, Cap::NeverStuck]),
    not_in_peaceful: true,
    no_portal: true,
    sound_source: super::SoundCategory::Hostile,
    ..Def::EMPTY
};
