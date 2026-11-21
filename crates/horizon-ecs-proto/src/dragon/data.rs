//! Constants of the dragon's body, hurt rules and contacts, copied from Horizon's `game/mobs/dragon.rs` (`D:line`), which keeps them
//! private: they live in `rules/dragon_data.rs`, with the flight graph's `ADJACENCY` (`D:606`, read by the flight nodes, L2).
pub use crate::rules::dragon_data::*;
pub use crate::rules::dragon_nodes::ADJACENCY;
