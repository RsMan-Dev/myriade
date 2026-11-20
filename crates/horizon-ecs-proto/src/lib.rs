//! Prototype herds as `bevy_ecs` worlds (suivi/ecs-prototype.md sections 11 and 12).
//! `flat`: the first throwaway herd (ground = the plane y = 0). `cow`: a cow with Horizon's real block
//! collision kernel and the stroll / move-control / travel formulas of the real cow.
pub mod behaviours;
pub mod bench_support;
pub mod blocks;
pub mod brain;
pub mod conversion;
pub mod cow;
pub use cow::helpers as cow_helpers;
pub mod despawn;
pub mod effects;
pub mod equipment;
pub mod explosion;
pub mod jump_arc;
pub mod farm;
pub mod flat;
pub mod goals;
pub mod health;
pub mod instr;
pub mod item;
pub mod items;
pub mod item_reference;
pub mod leash;
pub mod level;
pub mod loot;
pub mod mount;
pub mod orb;
pub mod area;
pub mod attributes;
pub mod pathsearch;
pub mod host_out;
pub mod hostile;
pub mod kernel_lab;
pub mod locomotion;
pub mod move_to_block;
pub mod occupancy;
pub mod poi;
pub mod block_watch;
pub mod profile;
pub mod projectile;
pub mod raids;
pub mod rules;
pub mod size;
pub mod small;
pub mod spawn_reference;
pub mod spawning;
pub mod spots;
pub mod species;
pub mod tame;
pub mod vehicle;
pub mod targeting;
pub mod touch;
pub mod zone;
pub mod spawn_finalize;
pub mod spawn_spawner;
pub mod spawn_trial;
pub mod equipment_spawn;
pub mod equipment_enchant;
pub mod equipment_items;
pub mod equipment_wear;
pub mod equipment_drop;
pub mod equipment_break;
pub mod goals_near;
pub mod pocket;

/// `f64::floor` as an `i32` without the libm call (the baseline target has no `roundsd`): the truncation, one less when it rounded up.
/// Same result as `x.floor() as i32` for every `x` (saturating at the ends, 0 for NaN).
pub trait IFloor {
    fn ifloor(self) -> i32;
}
impl IFloor for f64 {
    #[inline(always)]
    fn ifloor(self) -> i32 {
        let i = self as i32;
        i.saturating_sub((self < f64::from(i)) as i32)
    }
}
impl IFloor for f32 {
    #[inline(always)]
    fn ifloor(self) -> i32 {
        let i = self as i32;
        i.saturating_sub((self < i as f32) as i32)
    }
}

pub mod block_events;
pub mod fangs;
pub mod eye;
pub mod breath_cloud;
pub mod end_crystal;
pub mod dragon;
pub mod lightning;
pub mod falling_block;
pub mod firework;
pub mod minecart;
pub mod trident;
pub mod ominous_spawner;
pub mod passive;
pub mod hanging;
pub mod stand;
pub mod tnt;
pub mod boat;
pub mod projectile_more;
