//! `EnderDragon` (`boss/enderdragon/EnderDragon`: constructor, `createAttributes`; Horizon `game/mobs/dragon.rs`, `Dragon::new` D:611-664).
//! Configuration only: the phase machine (`dragon::phases`) and the flight (`dragon::flight`) are the species' two goals. Not here: the
//! boss bar (the fight's, L5: Horizon shows it for the fight's dragon only, `bossbars.rs:622`), the ambient sound (`Mob.baseTick` rolls
//! `nextInt(1000)` each tick against `ambientTime`, not a delay drawn once: `ambient_sound` does not reproduce it), the hurt filter (`dragon::hurt`).
use crate::behaviours::{Cap, Entry};
use crate::dragon::{flight, parts::PartDef, phases};
use crate::locomotion::Medium;
use crate::species::Def;

/// The eight boxes in the order of `subEntities`: head, neck, body, tail x3, wing x2 (sizes: jar constructor; `PART_SIZES` D:596).
pub const PARTS: [PartDef; 8] = [
    PartDef { name: "head", w: 1.0, h: 1.0 },
    PartDef { name: "neck", w: 3.0, h: 3.0 },
    PartDef { name: "body", w: 5.0, h: 3.0 },
    PartDef { name: "tail", w: 2.0, h: 2.0 },
    PartDef { name: "tail", w: 2.0, h: 2.0 },
    PartDef { name: "tail", w: 2.0, h: 2.0 },
    PartDef { name: "wing", w: 4.0, h: 2.0 },
    PartDef { name: "wing", w: 4.0, h: 2.0 },
];
/// The phase a dragon is born in (`EnderDragonPhase.HOVERING`, id 10): a `/summon`, an egg, a save without `DragonPhase`. Only the
/// fight's dragon starts in the holding pattern (L5).
pub const START_PHASE: u8 = 10;

pub static DEF: Def = Def {
    name: "minecraft:ender_dragon",
    goals: &[Entry::new(&phases::MODULE, 0, &phases::Params { start: START_PHASE, ..phases::Params::DEFAULT })],
    locomotion: &[(Medium::Air, Entry::new(&flight::MODULE, 0, &flight::Params::DEFAULT))],
    // Health is not here: `SpeciesTable` takes it from the type's data (200, `createAttributes`); the camera distance (16) is client-side.
    caps: Some(&[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion]),
    ..Def::EMPTY
};
