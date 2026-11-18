//! Prototype herds as `bevy_ecs` worlds (suivi/ecs-prototype.md sections 11 and 12).
//! `flat`: the first throwaway herd (ground = the plane y = 0). `cow`: a cow with Horizon's real block
//! collision kernel and the stroll / move-control / travel formulas of the real cow.
pub mod cow;
pub mod flat;
pub mod goals;
