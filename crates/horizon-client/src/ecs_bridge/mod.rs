//! The server's side of the ECS bridge (greffe L0): an ECS entity read as the tracker's `EntityState` (`state`), and the host
//! events the ECS writes turned into the `MobEvent`s the server applies (`events`). The level of a dimension (`level`) and its
//! reads by network id (`access`) wrap the ECS world for the game.
pub mod access;
pub mod events;
pub mod level;
pub mod state;
#[cfg(test)]
mod tests_level;

mod level_goals;
mod level_read;
mod level_raids;
mod level_read_ai;
mod level_read_living;
mod level_creaking;
mod level_write;
mod level_write_ai;
mod level_acts;
mod level_vehicle;
mod level_leash;
mod items;
mod dragon;
pub use items::ItemView;
mod orbs;
pub use orbs::OrbView;

pub use level::EcsLevel;
pub use level_raids::RaiderNow;
mod boat_effects;
mod boat_hurt;
mod boat_input;
mod boats;
mod cart_more;
mod cart_kill;
mod cart_refs;
mod cart_move;
pub(crate) mod cart_variants;
mod carts;
mod falling;
mod firework;
mod general;
mod hanging_more;
mod hook;
mod level_attr;
mod level_counts;
mod level_dispense;
mod level_equipment;
mod level_combat;
mod level_listen;
mod level_mount;
mod level_nav_more;
mod level_pickup;
mod level_pilot;
mod level_player_uuids;
mod level_portal;
mod level_state;
mod level_vitals;
mod level_using;
mod misc;
mod projectile_more;
mod seat;
mod tnt;
mod trident;
mod vehicle_push;
pub mod inside_zone;
