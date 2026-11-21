//! Every horizon-client parity probe in one binary: `probe <name> [arguments]` runs `<name>.rs` as its own
//! example did (tools/test_*_parity.py, tools/oracle_harness.py, tools/audio_harness.py). Linking ~80 examples,
//! each with the whole client in it, was most of the parity job's probe build; one binary builds it once.
//!
//! This file only lists the probes, so that adding one reruns no harness: tools/parity.py leaves it out of
//! their dependencies. How a probe runs is in dispatch.rs.
extern crate horizon_client_lib as horizon_client;
#[path = "../../src/test_resources.rs"]
mod test_resources;
#[macro_use]
mod dispatch;

use dispatch::{args, args_os};

mod block_entity_tickers_fixture;
mod block_transform_fixture;
mod brewing_fixture;
mod cauldron_fixture;
mod chest_load_fixture;
mod chest_pair_fixture;
mod command_parse_fixture;
mod container_title_fixture;
mod conversion_tags_fixture;
mod creative_mining_probe;
mod cushion_placement_probe;
mod enchant_durability_fixture;
mod enchanting_fixture;
mod entity_components_probe;
mod entity_death_loot_fixture;
mod entity_sound_packet_fixture;
mod environment_attribute_modifier_fixture;
mod environment_fixture;
mod environment_system_fixture;
mod environment_timeline_fixture;
mod falling_block_fixture;
mod finalize_spawn_fixture;
mod furnace_fixture;
mod hotbar_fixture;
mod lan_probe;
mod late_entity_pair_fixture;
mod lava_cauldron_item_fixture;
mod light_parity;
mod mob_ai_probe;
mod mob_attribute_nbt_fixture;
mod mob_attributes_fixture;
mod natural_spawn_fixture;
mod pick_block_data_fixture;
mod player_sync_fixture;
mod portal_tickets_fixture;
mod raid_fixture;
mod random_tick_sections_probe;
mod recipe_fixture;
mod remote_attack_probe;
mod scene_collision_probe;
mod scene_motion_probe;
mod scoreboard_fixture;
mod server_files_probe;
mod sheep_dye_fixture;
mod shulker_bullet_probe;
mod spawn_rules_fixture;
mod squid_client_probe;
mod stack_mining_probe;
mod storage_menu_fixture;
mod team_rules_fixture;
mod text_display_fixture;
mod trade_components_probe;
mod trading_fixture;
mod vehicle_container_fixture;
mod world_options_fixture;

probes! {
    block_entity_tickers_fixture,
    block_transform_fixture,
    furnace_fixture,
    brewing_fixture,
    squid_client_probe,
    creative_mining_probe,
    stack_mining_probe,
    cushion_placement_probe,
    cauldron_fixture,
    lava_cauldron_item_fixture,
    chest_pair_fixture,
    chest_load_fixture,
    command_parse_fixture,
    container_title_fixture,
    conversion_tags_fixture,
    enchant_durability_fixture,
    enchanting_fixture,
    falling_block_fixture,
    entity_components_probe,
    entity_death_loot_fixture,
    environment_attribute_modifier_fixture,
    environment_fixture,
    environment_system_fixture,
    environment_timeline_fixture,
    finalize_spawn_fixture,
    hotbar_fixture,
    lan_probe,
    light_parity,
    mob_ai_probe,
    mob_attribute_nbt_fixture,
    mob_attributes_fixture,
    natural_spawn_fixture,
    pick_block_data_fixture,
    player_sync_fixture,
    portal_tickets_fixture,
    raid_fixture,
    random_tick_sections_probe,
    recipe_fixture,
    remote_attack_probe,
    scene_collision_probe,
    scene_motion_probe,
    scoreboard_fixture,
    server_files_probe,
    late_entity_pair_fixture,
    sheep_dye_fixture,
    shulker_bullet_probe,
    entity_sound_packet_fixture,
    spawn_rules_fixture,
    storage_menu_fixture,
    team_rules_fixture,
    text_display_fixture,
    trading_fixture,
    trade_components_probe,
    vehicle_container_fixture,
    world_options_fixture,
}
