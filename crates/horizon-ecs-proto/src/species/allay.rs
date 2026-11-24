//! `Allay`: a flier that wanders in the air, keeps close to its owner, takes the item it is given and flies it back, looks at players,
//! runs when hurt, fetches the item of its hand from the ground into its pocket (`go_to_wanted` with `hand`), likes the note block it
//! heard for 600 ticks and dances to a jukebox within 10 blocks (`event_listen`, fed by `EcsWorld::allays_hear`). Left out, outside this
//! species: the throw at arrival (a brain step), the duplication, the deposit at the liked block (`event_listen::liked_deposit` is not
//! called by `give_to_owner` yet), the wool occlusion of vibrations, and the `interact` entry point (not called yet: the owner is set
//! by `tame::set_owner`).
//! DOUBT: the glide damping of the allay's flight is not checked against vanilla (the default value is used).
use super::Def;
use crate::behaviours::{animal_panic, event_listen, fly_stroll, give_to_owner, go_to_wanted, look_at_player, random_look, stay_close_to_owner, Cap, Entry};
use crate::locomotion::{fly, Medium};

pub static DEF: Def = Def {
    name: "minecraft:allay",
    goals: &[
        Entry::new(&crate::behaviours::move_emission::MODULE, 0, &crate::behaviours::move_emission::Params { flap: crate::behaviours::move_emission::Flap::Airborne, ..crate::behaviours::move_emission::Params::DEFAULT }),
        Entry::new(&animal_panic::MODULE, 0, &animal_panic::Params { speed: 2.5, ..animal_panic::Params::DEFAULT }),
        Entry::new(&go_to_wanted::MODULE, 1, &go_to_wanted::Params { hand: true, range: 32.0, speed: 1.75, ..go_to_wanted::Params::DEFAULT }),
        Entry::new(&give_to_owner::MODULE, 2, &give_to_owner::Params::DEFAULT),
        Entry::new(&stay_close_to_owner::MODULE, 3, &stay_close_to_owner::Params::DEFAULT),
        Entry::new(&fly_stroll::MODULE, 4, &fly_stroll::Params { reach: Some((10, 7)), ..fly_stroll::Params::DEFAULT }),
        Entry::new(&look_at_player::MODULE, 5, &look_at_player::Params { range: 6.0, species: look_at_player::Params::ALL, ..look_at_player::Params::DEFAULT }),
        Entry::new(&random_look::MODULE, 6, &random_look::Params { interval: Some((150, 250)), ..random_look::Params::DEFAULT }),
        Entry::new(&event_listen::MODULE, 7, &event_listen::Params {
            note_event: "minecraft:note_block_play",
            note_radius: 16,
            note_block: "minecraft:note_block",
            forget: 600,
            liked_reach: 1024,
            play_event: "minecraft:jukebox_play",
            stop_event: "minecraft:jukebox_stop_play",
            dance_radius: 10,
            dance_block: "minecraft:jukebox",
        }),
    ],
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Head, Cap::Tamable, Cap::Locomotion]),
    locomotion: &[(Medium::Air, Entry::new(&fly::MODULE, 0, &fly::Params { max_turn: 20.0, hovers: true, free: true, drive_air: true, ..fly::Params::DEFAULT }))],
    rules: &[crate::rules::RuleEntry::new(&crate::rules::NO_FALL, &()), crate::rules::RuleEntry::new(&crate::duplicate_item::DUPLICATE_ITEM, &crate::duplicate_item::DuplicateParams::ALLAY), crate::rules::RuleEntry::new(&crate::hand_over::HAND_OVER, &crate::hand_over::HandOverParams::ALLAY), crate::rules::RuleEntry::new(&crate::allay_heal::HEAL, &())],
    ..Def::EMPTY
};
