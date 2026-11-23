#[test]
#[ignore = "daily"]
fn held_durability_retains_the_stack_hand_and_explicit_break_slot() {
    use crate::game::{Game, Hand, Options, Progress, Settings, load_resources};
    use horizon_client::{items::ItemStack, motion::PlayerMotion};
    use horizon_core::equipment_slot::EquipmentSlot;
    use horizon_core::geometry::Vec3;
    let root = std::path::PathBuf::from(
        std::env::var_os("HORIZON_TEST_RESOURCES").expect("prepared resources required"),
    );
    let settings = Settings::default();
    let loaded = load_resources(
        horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
        Vec::new(),
        None,
        &root,
        &settings,
        std::time::Instant::now(),
        crate::game::execution_role::ExecutionRole::DedicatedServer,
        &Progress::new(),
    )
    .unwrap();
    let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
    options.resources = root.clone();
    options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
    let mut game = Game::new(
        &loaded.resources,
        loaded.pack,
        options,
        settings,
    )
    .unwrap();
    game.player = Some(PlayerMotion::new(game.motion, Vec3::new(0.0, 70.0, 0.0)).unwrap());
    game.set_mode_id(0);
    // Stage an acting guest: break events are queued instead of rendering local crumbs.
    game.lan_acting = Some(1);
    game.hand = Hand::Off;
    let main = ItemStack::new("minecraft:stick", 3);
    game.inventory[game.selected] = Some(main.clone());
    let max = game
        .items()
        .get("minecraft:flint_and_steel")
        .unwrap()
        .max_damage;
    game.offhand = Some(ItemStack::new("minecraft:flint_and_steel", 1));
    game.damage_held_in_slot(1, EquipmentSlot::OffHand);
    assert_eq!(game.offhand.as_ref().unwrap().damage, 1);
    assert!(game.guest_equipment_breaks.is_empty());
    for (explicit, event) in [(true, 48), (false, 47)] {
        let mut tool = ItemStack::new("minecraft:flint_and_steel", 1);
        tool.damage = max - 1;
        game.offhand = Some(tool);
        if explicit {
            game.damage_held_in_slot(1, EquipmentSlot::OffHand);
        } else {
            game.damage_held(1);
        }
        assert!(game.offhand.is_none());
        assert_eq!(game.inventory[game.selected], Some(main.clone()));
        assert_eq!(game.guest_equipment_breaks.last().unwrap(), &event);
    }
    game.set_mode_id(1);
    game.offhand = Some(ItemStack::new("minecraft:flint_and_steel", 1));
    game.damage_held_in_slot(1, EquipmentSlot::OffHand);
    assert_eq!(game.offhand.as_ref().unwrap().damage, 0);
}

