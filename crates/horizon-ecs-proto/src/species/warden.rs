//! `minecraft:warden`: the values of the Mob's constants (`mobs/warden.rs`, `mobs/warden_brain.rs`), given to the modules. The
//! modules hold the mechanics only; their parameters have no default. Priorities: the lower number runs first (the brain's order).
use super::{templates::monster, Def};
use crate::behaviours::{
    darkness_pulse, dig, emerge, float, go_to_disturbance, melee, roar_at_target, sniff, sonic_boom,
    suspect_anger, vibration_listen, Cap, Entry,
};

pub static DEF: Def = Def {
    name: "minecraft:warden",
    reference: Some(&monster::DEF),
    goals: &[
        Entry::new(&emerge::MODULE, 0, &emerge::Params { duration: 134, sound: "entity.warden.emerge" }),
        Entry::new(&dig::MODULE, 1, &dig::Params { duration: 100, sound_ground: "entity.warden.dig", sound_liquid: "entity.warden.agitated" }),
        Entry::new(
            &suspect_anger::MODULE,
            2,
            &suspect_anger::Params {
                max: 150,
                agitated: 40,
                angry: 80,
                step: 20,
                memory: u16::MAX,
                hurt_boost: 20,
                dig_cooldown: 1200,
                hurt_target_range_sqr: 25.0,
            },
        ),
        Entry::new(&vibration_listen::MODULE, 2, &vibration_listen::Params {
            cooldown: 40,
            anger: 35,
            projectile_anger: 10,
            projectile_window: 100,
            owner_range_sqr: 900.0,
            disturbance_expiry: 100,
        }),
        Entry::new(&sonic_boom::MODULE, 3, &sonic_boom::Params {
            duration: 60,
            sound_delay: 34,
            cooldown: 60,
            range_h: 15.0,
            range_v: 20.0,
            chest: 1.6,
            charge_sound: "entity.warden.sonic_charge",
            boom_sound: "entity.warden.sonic_boom",
        }),
        Entry::new(&roar_at_target::MODULE, 4, &roar_at_target::Params { duration: 84, anger: 20, sound: "entity.warden.roar" }),
        Entry::new(&melee::MODULE, 5, &melee::Params { interval: 18, ..melee::Params::DEFAULT }),
        Entry::new(&go_to_disturbance::MODULE, 6, &go_to_disturbance::Params { speed: 0.7 }),
        Entry::new(&sniff::MODULE, 7, &sniff::Params { duration: 84, cooldown: 150, sound: "entity.warden.sniff", radius: 24.0 }),
        Entry::new(&darkness_pulse::MODULE, 8, &darkness_pulse::Params { radius: 20.0, interval: 120 }),
        Entry::new(&float::MODULE, 9, &()),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion]),
    ..Def::EMPTY
};
