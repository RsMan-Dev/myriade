//! The seven minecart genres (Horizon's `CartKind`): configuration of `minecart`, whose physics knows no genre.
use super::Def;
use crate::behaviours::Cap;
use crate::minecart::Kind;

/// The cart's capabilities: a position and a box. Its rail motion is driven by the host, so no brain, body or goal cap.
const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid];

/// The seven genres (Horizon's `CartKind`). The genre is configuration: its `Kind` is set when the cart is spawned.
pub const RIDEABLE: Def = Def { name: "minecraft:minecart", caps: Some(CAPS), ..Def::EMPTY };
pub const CHEST: Def = Def { name: "minecraft:chest_minecart", caps: Some(CAPS), ..Def::EMPTY };
pub const HOPPER: Def = Def { name: "minecraft:hopper_minecart", caps: Some(CAPS), ..Def::EMPTY };
pub const FURNACE: Def = Def { name: "minecraft:furnace_minecart", caps: Some(CAPS), ..Def::EMPTY };
pub const TNT: Def = Def { name: "minecraft:tnt_minecart", caps: Some(CAPS), ..Def::EMPTY };
pub const COMMAND_BLOCK: Def = Def { name: "minecraft:command_block_minecart", caps: Some(CAPS), ..Def::EMPTY };
pub const SPAWNER: Def = Def { name: "minecraft:spawner_minecart", caps: Some(CAPS), ..Def::EMPTY };

/// Each genre's `Def` with the `Kind` it spawns as.
pub const GENRES: [(&Def, Kind); 7] = [
    (&RIDEABLE, Kind::Rideable),
    (&CHEST, Kind::Chest),
    (&HOPPER, Kind::Hopper),
    (&FURNACE, Kind::Furnace),
    (&TNT, Kind::Tnt),
    (&COMMAND_BLOCK, Kind::CommandBlock),
    (&SPAWNER, Kind::Spawner),
];
