//! A server's mobs as a client animates them. The server thread, a LAN host
//! and a dedicated server run each mob's animation state (`ai::tick`); a
//! client of a server only gets synched data and entity events, and runs the
//! animation steps vanilla runs on the client (`Entity.onSyncedDataUpdated`,
//! `handleEntityEvent` and the per tick animation states). The server's half
//! is `game/network/lan/mod.rs` (`mob_meta`, with `pose_id` here for index 6) and
//! `MobEvent::EntityEvent`; the client's half is `game/network/remote_mob_data.rs`,
//! `game/network/remote/mod.rs` (`C::EntityEvent`) and `client_tick` here.
//!
//! To animate another species for a client:
//! - send its synched data in `mob_meta` and read it in
//!   `remote_mob_data::apply`;
//! - send its poses in `pose_id` and read them in `set_pose`;
//! - broadcast its entity events with `MobEvent::EntityEvent` and handle
//!   them in `entity_event`;
//! - run its client side animation in `client_tick`, sharing the function
//!   with the server's `ai_step` where the server computes the same.
use super::{
    MobEvent, Species, allay, armadillo, axolotl, breeze, camel, copper_golem, creaking,
    equine, frog, goat, golem, guardian, piglin, ravager, shulker, sniffer, tame, undead, villager,
    warden,
};

// `Pose.id` of the poses mobs take.
pub const STANDING: u8 = 0;
pub const SLEEPING: u8 = 2;
pub const CROUCHING: u8 = 5;
pub const LONG_JUMPING: u8 = 6;
pub const CROAKING: u8 = 8;
pub const USING_TONGUE: u8 = 9;
pub const SITTING: u8 = 10;
pub const ROARING: u8 = 11;
pub const SNIFFING: u8 = 12;
pub const EMERGING: u8 = 13;
pub const DIGGING: u8 = 14;
pub const SLIDING: u8 = 15;
pub const SHOOTING: u8 = 16;
pub const INHALING: u8 = 17;




