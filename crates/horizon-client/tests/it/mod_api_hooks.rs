//! A mod crate's view of the mod API: `Hooks` is implemented with the names `mod_api` exports.
//! The test is that this file builds with the exported event types.
use mod_api::{Hooks, ModEntry};

/// A login profile reaches the mods before the login success sends it, and a mod replaces the
/// skin there.
#[test]
fn a_mod_changes_a_login_profile_before_it_is_sent() {
    use mod_api::{ProfileProperty, dispatch_player_login_profile};
    use std::sync::Mutex;

    static SEEN: Mutex<Vec<(u128, String)>> = Mutex::new(Vec::new());

    #[derive(Default)]
    struct Skin;

    #[mod_api::horizon_mod]
    impl Hooks for Skin {
        fn player_login_profile(&self, event: &mut mod_api::PlayerLoginProfileEvent<'_>) {
            SEEN.lock()
                .unwrap()
                .push((event.player, event.name.to_owned()));
            event
                .properties
                .retain(|property| property.name != "textures");
            event.properties.push(ProfileProperty::new(
                "textures".into(),
                "new".into(),
                Some("signed".into()),
            ));
        }
    }

    let mut properties = vec![ProfileProperty::new("textures".into(), "old".into(), None)];
    mod_platform::testing::with_mods(vec![ModEntry::new("skin", Box::new(Skin))].into(), || {
        dispatch_player_login_profile(7, "Alex", &mut properties)
    });
    assert_eq!(*SEEN.lock().unwrap(), [(7, "Alex".to_owned())]);
    assert_eq!(
        properties,
        [ProfileProperty::new(
            "textures".into(),
            "new".into(),
            Some("signed".into())
        )]
    );
}

// Fluid acceptance is scoped to a FLUIDS provider, not a global Hooks method.
// Its default/refused/accepted writes are covered with a real Game backend in
// game/blocks/machines/native_fluids.rs::tests::a_scoped_tank_can_refuse_a_fluid.

/// Each mod changes the prior answer, vanilla's stays readable, and cancel
/// stops later mods. The fork's existing typed event owns this moment.
#[test]
fn an_anvil_update_is_changed_in_turn_and_cancel_stops_later_mods() {
    use mod_api::item::menu::AnvilUpdate;
    use mod_api::item::stations::AnvilResult;
    use mod_api::{AnvilUpdateEvent, Flow, Host, HostSide, ItemStack, ServerHost};
    fn key(id: &str) -> mod_api::ItemKey {
        mod_api::__private::item_key(id).unwrap()
    }
    struct TestHost;
    impl Host for TestHost {}
    mod_api::impl_host_areas!(TestHost);
    mod_api::impl_server_host_areas!(TestHost);
    impl ServerHost for TestHost {}
    #[derive(Default)]
    struct Recipe;
    #[mod_api::__subscriptions]
    impl Hooks for Recipe {
        fn anvil_update(&self, _: HostSide<'_>, event: &mut AnvilUpdateEvent<'_>) -> Flow {
            let update = &mut *event.update;
            if update.name.as_deref() == Some("void") {
                return Flow::Cancel;
            }
            if update.left.item() == key("minecraft:iron_ingot")
                && update
                    .right
                    .as_ref()
                    .is_some_and(|r| r.item() == key("minecraft:emerald"))
            {
                update.output = Some(ItemStack::of(key("minecraft:diamond")));
                update.xp_cost = 5;
                update.material_cost = 1;
            }
            Flow::Continue
        }
    }
    #[derive(Default)]
    struct Discount;
    #[mod_api::__subscriptions]
    impl Hooks for Discount {
        fn anvil_update(&self, _: HostSide<'_>, event: &mut AnvilUpdateEvent<'_>) -> Flow {
            assert_eq!(
                event.update.vanilla.xp_cost, 3,
                "vanilla's cost stays readable"
            );
            event.update.xp_cost -= 1;
            Flow::Continue
        }
    }
    let mods: mod_api::Mods = vec![
        ModEntry::new("recipe", Box::new(Recipe)),
        ModEntry::new("discount", Box::new(Discount)),
    ]
    .into();
    mod_platform::testing::with_mods(mods, || {
        mod_platform::__private::install_parts(
            &[],
            &mod_platform::__private::part_tests::rule_schema(),
            |_| false,
        )
        .unwrap();
        mod_api::__private::resolve_items([
            "minecraft:iron_sword",
            "minecraft:iron_ingot",
            "minecraft:emerald",
            "minecraft:diamond",
        ])
        .unwrap();
        let update = |left: &str, right: Option<&str>, name: Option<&str>| {
            let vanilla = AnvilResult {
                result: Some(ItemStack::of(key("minecraft:iron_sword"))),
                cost: 3,
                repair_item_count_cost: 0,
                only_renaming: false,
            };
            let mut update = AnvilUpdate::new(
                ItemStack::of(key(left)),
                right.map(|r| ItemStack::new(key(r), 4).unwrap()),
                name.map(str::to_owned),
                &vanilla,
            );
            let mut cancel = false;
            mod_api::dispatch_anvil_update(
                HostSide::Server(&mut TestHost),
                7,
                1,
                [0, 64, 0],
                &mut update,
                &mut cancel,
            );
            (
                update.output.map(|o| {
                    mod_api::__private::item_key_id(o.item())
                        .unwrap()
                        .to_owned()
                }),
                update.xp_cost,
                update.material_cost,
                cancel,
            )
        };
        let sword = Some("minecraft:iron_sword".to_owned());
        assert_eq!(
            update("minecraft:iron_ingot", Some("minecraft:emerald"), None),
            (Some("minecraft:diamond".to_owned()), 4, 1, false)
        );
        assert_eq!(
            update("minecraft:iron_sword", None, Some("")),
            (sword.clone(), 2, 0, false)
        );
        assert_eq!(
            update("minecraft:iron_sword", None, Some("void")),
            (sword, 3, 0, true)
        );
    });
}

#[test]
fn wind_burst_filter_preserves_order_and_context() {
    use mod_api::{Host, ServerHost, WindBurstEvent, dispatch_wind_burst};
    struct TestHost;
    impl Host for TestHost {}
    mod_api::impl_host_areas!(TestHost);
    mod_api::impl_server_host_areas!(TestHost);
    impl ServerHost for TestHost {}
    struct Claims;
    #[mod_api::__subscriptions]
    impl Hooks for Claims {
        fn wind_burst(&self, _host: &mut dyn ServerHost, event: &mut WindBurstEvent<'_>) {
            assert_eq!(event.dimension, "minecraft:the_nether");
            assert_eq!(event.center, [1.5, 70.0, -2.5]);
            event.blocks.retain(|p| p[0] != 1);
        }
    }
    let mods: mod_api::Mods = vec![ModEntry::new("claims", Box::new(Claims))].into();
    mod_platform::testing::with_mods(mods, || {
        let mut blocks = vec![[3, 70, -2], [1, 70, -2], [2, 70, -2]];
        dispatch_wind_burst(
            &mut TestHost,
            "minecraft:the_nether",
            [1.5, 70.0, -2.5],
            &mut blocks,
        );
        assert_eq!(blocks, vec![[3, 70, -2], [2, 70, -2]]);
    });
}
