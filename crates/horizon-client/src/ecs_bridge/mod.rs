//! The server's side of the ECS bridge (greffe L0): an ECS entity read as the tracker's `EntityState` (`state`), and the host
//! events the ECS writes turned into the `MobEvent`s the server applies (`events`). Nothing calls it yet: the next lots do.
pub mod events;
pub mod state;
