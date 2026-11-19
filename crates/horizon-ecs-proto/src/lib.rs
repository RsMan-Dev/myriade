//! Prototype herds as `bevy_ecs` worlds (suivi/ecs-prototype.md sections 11 and 12).
//! `flat`: the first throwaway herd (ground = the plane y = 0). `cow`: a cow with Horizon's real block
//! collision kernel and the stroll / move-control / travel formulas of the real cow.
pub mod behaviours;
pub mod bench_support;
pub mod blocks;
pub mod brain;
pub mod cow;
pub mod farm;
pub mod flat;
pub mod goals;
pub mod instr;
pub mod item;
pub mod item_reference;
pub mod loot;
pub mod mount;
pub mod orb;
pub mod hostile;
pub mod kernel_lab;
pub mod locomotion;
pub mod move_to_block;
pub mod poi;
pub mod profile;
pub mod projectile;
pub mod rules;
pub mod size;
pub mod spots;
pub mod species;
pub mod tame;
pub mod targeting;
pub mod zone;
