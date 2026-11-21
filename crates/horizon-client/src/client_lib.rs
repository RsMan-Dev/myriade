//! Horizon native client: resources, world streaming, rendering and the
//! bindings between the validated simulation pieces and the game loop.
#![forbid(unsafe_code)]
extern crate self as horizon_client;
#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: horizon_threads::allocation_probe::Allocator =
    horizon_threads::allocation_probe::Allocator;
pub mod advancements;
pub mod api_text;
pub mod arrows;
pub mod audio;
pub mod bed_geometry;
pub mod block_contact;
pub mod block_entity_render;
pub mod block_sounds;
pub mod item_damage;
pub mod packet_source;
pub mod parallel;
pub mod particles;
pub mod powder_snow;
#[cfg(any(test, feature = "proto"))]
mod test_log;
#[cfg(any(test, feature = "proto"))]
mod test_resources;
pub use horizon_client_render::{
    assets, auth, biome, block_entities, block_entity_anim, block_states, brewing, chunk,
    chunk_map, collisions, command_block, containers, crash, datapack, effects, environment,
    fast_hash, jigsaw_block, level_loading, light, lod, logging, mod_chunk_tickets, mod_list,
    pack_meta, poi_sections,
    potent_sulfur, providers, random_source, sculk, section_mask, signs, skins, sound_events,
    spans, streaming, structure_block, test_block, test_instance_block, testgen, vault, weather,
    world, worldborder,
};
pub mod attack_strength;
pub mod attribute_updates;
pub mod auto_jump;
pub mod banner;
pub mod beehive;
pub mod block_entity_item;
pub mod block_pattern;
pub mod block_transform;
pub mod breathing;
pub mod build_mismatch;
pub mod cauldron;
pub mod chunk_io;
pub mod clock;
pub mod command_storage;
pub mod content_identity;
pub mod crafting;
pub mod cushion;
pub mod data_cmd;
pub mod datapack_net;
pub mod debugworld;
pub mod desktop;
pub mod dialog;
pub mod enchantment_levels;
pub mod end_crystal;
pub mod end_gateway;
pub mod entities;
pub mod entity_nbt;
pub mod entity_rules;
pub mod falling_block;
pub mod flatgen;
pub mod furnace;
pub mod game_mode_switcher;
pub mod gossip;
pub mod gui_layers;
pub mod health;
pub mod hotbar;
pub mod hunger;
pub mod inv_sort;
pub mod item_activation;
pub mod item_attributes;
pub mod item_names;
pub mod item_sync;
pub mod join_seq;
pub mod move_gate;
/// Game-owned JSON metadata conversion, shared by recipe and API adapters.
pub use horizon_core::json_nbt;
pub mod lan;
pub mod legacy_combat;
pub mod living_attributes;
pub mod map_decoration_types;
pub mod menu;
pub mod menu_extra_net;
pub mod merchant;
pub mod mobs;
pub mod mod_content;
pub mod modgen;
pub mod motion;
pub mod native_stack_hooks;
pub mod neoforge_mob_maps;
pub mod nether_portal;
pub mod offline_name;
pub mod old_world;
pub mod patrol;
pub mod perf_net;
pub mod pick_item;
pub mod player_sync;
pub mod portal_sections;
pub mod pose;
pub mod pregen;
pub mod projectiles;
pub mod raid;
pub mod random_tick_sections;
pub mod raw_payloads;
pub mod recipe_book;
pub mod save;
// The carrier ownership store: an internal storage layer of this
// library, used by the game crate's transfers (game/world/), like
// save.rs below it. Not part of the mods' API surface.
#[cfg(test)]
mod carrier_recover_tests;
pub mod carrier_store;
#[cfg(test)]
mod carrier_store_tests;
pub mod scoreboard_cmd;
pub mod screen_layout;
pub use horizon_server_files as server;
pub mod server_packs;
pub mod server_perf;
pub mod spear;
pub mod splashes;
pub mod sprint;
pub mod stand_up;
pub mod stations;
pub mod stopwatches;
pub mod structure_blocks;
#[cfg(any(test, feature = "proto"))]
pub mod test_fixtures;
pub mod text_display;
pub mod tick_chunk_membership;
pub mod ticket_levels;
pub mod vanillagen;
pub mod viewed_levels_net;
pub mod world_primitives;
pub mod world_spawn;
pub use horizon_client_core::Result;
pub use horizon_client_core::block_entity_nbt;
pub use horizon_client_core::block_entity_types;
pub use horizon_client_core::bossbar;
pub use horizon_client_core::component;
pub use horizon_client_core::component_codec;
pub use horizon_client_core::fastutil_order;
pub use horizon_client_core::gamerules;
pub use horizon_client_core::lang;
pub use horizon_client_core::nbt_path;
pub use horizon_client_core::scoreboard;
pub use horizon_client_core::snbt;
pub use horizon_client_core::text;
pub use horizon_command as command;
pub use horizon_items::blocks;
pub use horizon_items::bundle;
pub use horizon_items::cooldowns;
pub use horizon_items::enchant;
pub use horizon_items::flowers;
pub use horizon_items::item_data;
pub use horizon_items::items;
pub use horizon_items::random_sequences;

#[cfg(test)]
mod brewing_speed_load_tests;

pub mod control;
pub mod landing;
pub mod native_client_hooks;
pub mod net;
pub mod lod_net;
