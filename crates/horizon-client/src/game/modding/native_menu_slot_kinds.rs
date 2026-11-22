//! Client layout roles; never inspect stacks or server container identities.
use mod_api::MenuSlotKind;

#[cfg(test)]
mod tests {
    use super::*;
    use MenuSlotKind::*;
    use mod_api::__private::game::Fields;

    const PLAYER: [MenuSlotKind; 36] = [
        Player(9),
        Player(10),
        Player(11),
        Player(12),
        Player(13),
        Player(14),
        Player(15),
        Player(16),
        Player(17),
        Player(18),
        Player(19),
        Player(20),
        Player(21),
        Player(22),
        Player(23),
        Player(24),
        Player(25),
        Player(26),
        Player(27),
        Player(28),
        Player(29),
        Player(30),
        Player(31),
        Player(32),
        Player(33),
        Player(34),
        Player(35),
        Player(0),
        Player(1),
        Player(2),
        Player(3),
        Player(4),
        Player(5),
        Player(6),
        Player(7),
        Player(8),
    ];

    #[test]
    fn layout_roles_in_menu_order() {
        for (name, own) in [
            (
                "inventory",
                vec![
                    Result,
                    CraftingGrid,
                    CraftingGrid,
                    CraftingGrid,
                    CraftingGrid,
                    Player(39),
                    Player(38),
                    Player(37),
                    Player(36),
                ],
            ),
            (
                "minecraft:crafting",
                vec![
                    Result,
                    CraftingGrid,
                    CraftingGrid,
                    CraftingGrid,
                    CraftingGrid,
                    CraftingGrid,
                    CraftingGrid,
                    CraftingGrid,
                    CraftingGrid,
                    CraftingGrid,
                ],
            ),
            ("minecraft:furnace", vec![Storage, Storage, Result]),
            (
                "minecraft:generic_9x3",
                vec![
                    Storage, Storage, Storage, Storage, Storage, Storage, Storage, Storage,
                    Storage, Storage, Storage, Storage, Storage, Storage, Storage, Storage,
                    Storage, Storage, Storage, Storage, Storage, Storage, Storage, Storage,
                    Storage, Storage, Storage,
                ],
            ),
            (
                "horizon:mount1",
                vec![Equipment, Equipment, Storage, Storage, Storage],
            ),
            ("minecraft:anvil", vec![Other, Other, Other]),
        ] {
            let mut expected = own;
            expected.extend(PLAYER);
            if name == "inventory" {
                expected.push(Player(40));
            }
            if name == "minecraft:anvil" {
                expected = vec![Other; 39];
            }
            let got = layout_kinds(name, expected.len(), None, |s| {
                Game::inventory_screen_slot(name, s as i32)
            });
            assert_eq!(got, expected, "{name}");
        }
    }

    #[test]
    fn content_declaration_roles() {
        use horizon_client::containers::{ModCrafting, ModLinks};
        let content = ModMenuType {
            id: "test:roles",
            slots: 8,
            data_slots: 0,
            results: vec![7],
            crafting: Some(ModCrafting {
                grid: 0,
                size: 2,
                result: 4,
            }),
            links: Some(ModLinks { from: 5, count: 2 }),
        };
        let mut expected = vec![
            CraftingGrid,
            CraftingGrid,
            CraftingGrid,
            CraftingGrid,
            Result,
            Storage,
            Storage,
            Result,
        ];
        expected.extend(PLAYER);
        assert_eq!(
            layout_kinds("test:roles", 44, Some(&content), |s| Some(if s < 8 {
                300 + s
            } else if s < 35 {
                s + 1
            } else {
                s - 35
            })),
            expected
        );
    }

}
