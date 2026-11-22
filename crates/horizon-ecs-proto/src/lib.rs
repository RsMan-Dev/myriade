//! Prototype herds as `bevy_ecs` worlds (suivi/ecs-prototype.md sections 11 and 12).
//! `flat`: the first throwaway herd (ground = the plane y = 0). `cow`: a cow with Horizon's real block
//! collision kernel and the stroll / move-control / travel formulas of the real cow.
//! Bench probe scope (feature `probes`, env `BRAINPROF`): `bp!("kind", name)` counts the instructions of the rest of the block (`brain::bprobe`).
#[cfg(feature = "probes")]
macro_rules! bp { ($k:expr, $n:expr) => { let _bp = $crate::brain::bprobe::scope($k, $n); }; }
#[cfg(not(feature = "probes"))]
macro_rules! bp { ($k:expr, $n:expr) => {}; }
pub mod behaviours;
pub mod bench_support;
pub mod blocks;
pub mod brain;
pub mod conversion;
pub mod cow;
pub use cow::helpers as cow_helpers;
pub mod despawn;
pub mod spring_trap;
pub mod effects;
pub mod equipment;
pub mod explosion;
pub mod jump_arc;
pub mod farm;
pub mod give_item;
pub mod gift_drop;
pub mod held_wear;
pub mod ignite;
pub mod dye;
pub mod spawn_offspring;
pub mod used_stack;
pub mod age_lock;
pub mod body_armor;
pub mod fit_chest;
pub mod coating;
pub mod hand_feed;
pub mod duplicate_item;
pub mod heal_by_food;
pub mod hand_over;
pub mod hand_swallow;
pub mod flat;
pub mod goal_edit;
pub mod goals;
pub mod health;
pub mod instr;
pub mod item;
pub mod items;
pub mod item_reference;
pub mod keep_on_use;
pub mod leash;
pub mod credit;
pub mod damage_link;
pub mod guard_alert;
pub mod reinforcements;
pub mod hurt_spawn;
pub mod hiss;
pub mod fall_passengers;
pub mod make_mad;
pub mod stab_dismount;
pub mod remove_killed;
pub mod level;
pub mod loot;
pub mod mount;
pub mod orb;
pub mod area;
pub mod attributes;
pub mod pathsearch;
pub mod host_out;
pub mod bridge;
pub mod hostile;
pub mod melee_gear;
pub mod kernel_lab;
pub mod locomotion;
pub mod move_to_block;
pub mod occupancy;
pub mod poi;
pub mod block_watch;
pub mod chest_index;
pub mod profile;
pub mod projectile;
pub mod raid_buffs;
pub mod raids;
pub mod rules;
pub mod size;
pub mod spawn_reference;
pub mod spawning;
pub mod spawn_structures;
pub mod spots;
pub mod species;
pub mod tame;
pub mod type_tags;
pub mod nbt_tag;
pub mod nbt_fields;
pub mod vehicle;
pub mod targeting;
pub mod touch;
pub mod zone;
pub mod world_push;
pub mod spawn_finalize;
pub mod spawn_gear;
pub mod spawn_reason;
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
pub mod fishing_hook;
pub mod breath_cloud;
pub mod end_crystal;
pub mod dragon;
pub mod swing;
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
pub mod shulker_bullet;
pub mod display;
pub mod interaction;
pub mod effect_cloud;
pub mod nbt;
pub mod absorption;
pub mod arrow_more;
pub mod attr_mod;
pub mod block_factor;
pub mod boat_effects;
pub mod boat_hurt;
pub mod boat_leash;
pub mod boat_state;
pub mod boat_tick;
pub mod cart_move;
pub mod cart_state;
pub mod cart_variants;
pub mod cloud_tick;
pub mod colony_register;
pub mod custom_name;
pub mod dispense;
pub mod display_tick;
pub mod end_crystal_tick;
pub mod equip_stack;
pub mod equip_attrs;
pub mod falling_land;
pub mod fangs_tick;
pub mod firework_burst;
pub mod frame_item;
pub mod general;
pub mod gliding;
pub mod hook_extract;
pub mod hook_wait;
pub mod host_out_arrows;
pub mod host_out_boat_effects;
pub mod host_out_boat_hurt;
pub mod host_out_boats;
pub mod host_out_cart_move;
pub mod host_out_cart_variants;
pub mod host_out_equipment;
pub mod host_out_falling;
pub mod host_out_firework;
pub mod host_out_frames;
pub mod host_out_general;
pub mod host_out_hook;
pub mod host_out_inside;
pub mod host_out_items;
pub mod host_out_misc;
pub mod host_out_seat;
pub mod host_out_tnt;
pub mod host_out_fight;
pub mod host_out_trident;
pub mod host_out_using;
pub mod host_out_vehicle_push;
pub mod left_handed;
pub mod tear_down;
pub mod lightning_tick;
pub mod mount_inventory;
pub mod mount_jump;
pub mod nav_supplied;
pub mod no_gravity;
pub mod painting;
pub mod pilot;
pub mod player_uuids;
pub mod portal;
pub mod pose_state;
pub mod seat;
pub mod stuck;
pub mod tnt_tick;
pub mod trident_hit;
pub mod using;
pub mod vehicle_push;
pub mod inside;
pub mod inside_effects;
pub mod inside_more;
pub mod move_events;
