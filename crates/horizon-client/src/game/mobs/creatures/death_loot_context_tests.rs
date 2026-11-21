use super::*;

struct BlockTool(mod_api::ItemStack);
#[mod_api::__subscriptions]
impl mod_api::Hooks for BlockTool {
    fn loot_drops(&self, _: &mut dyn mod_api::ServerHost, event: &mut mod_api::LootDropsEvent<'_>) {
        assert!(event.context.damage_source.is_none());
        assert!(event.context.block.is_some());
        assert_eq!(event.context.tool, Some(&self.0));
    }
}
#[test]
#[ignore = "daily"]
fn death_loot_change_preserves_block_tool() {
    crate::game::native_entity_refs::tests::with_game(|game| {
        game.build_registries();
        let mut tool = ItemStack::new("minecraft:diamond_pickaxe", 1);
        tool.damage = 17;
        tool.enchant("minecraft:fortune", 3);
        let expected = tool.try_into_api().unwrap();
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap();
        let mods = vec![mod_api::ModEntry::new(
            "block_tool",
            Box::new(BlockTool(expected)),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            game.native_block_loot([0, 64, 0], stone, None, Some(&tool), None, &mut Vec::new());
        });
    });
}
