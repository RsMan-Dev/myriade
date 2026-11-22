//! `makes_piglins_neutral` answered by item Parts on vanilla items: leather boots and a leather
//! helmet made to count as gold, golden boots made not to. The answer is what the view of the
//! player the piglins read carries (`gold_armor`, `PiglinAi.isWearingSafeArmor`), for the host
//! and for each LAN guest, and a calm piglin targets only a player whose view has none
//! (`NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD`).
use super::native_entity_refs::tests::{
    bind_local_player, with_game,
};
use super::native_guest_turn_tests::with_guests;
use horizon_client::items::ItemStack;
use horizon_client::motion::PlayerMotion;
use horizon_core::geometry::Vec3;
use mod_api::{Hooks, Host, ItemPart, MakesPiglinsNeutralEvent, Registrar, RegistrationContext};
use std::sync::Mutex;

/// Each question a part was asked: the item it is attached to and the wearer's entity id.
static ASKED: Mutex<Vec<(&'static str, u32)>> = Mutex::new(Vec::new());

/// Answers `makes_piglins_neutral` for its item.
#[derive(Clone)]
struct Neutral(&'static str, bool);

#[mod_api::horizon_part]
impl ItemPart for Neutral {
    fn makes_piglins_neutral(&self, _: &dyn Host, event: &mut MakesPiglinsNeutralEvent<'_>) {
        ASKED.lock().unwrap().push((self.0, event.wearer.id()));
        event.neutral = self.1;
    }
}

/// Leather boots and a leather helmet count as gold; golden boots don't.
struct NeutralMod;

#[mod_api::__subscriptions]
impl Hooks for NeutralMod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for (item, neutral) in [
            ("minecraft:leather_boots", true),
            ("minecraft:leather_helmet", true),
            ("minecraft:golden_boots", false),
        ] {
            let key = r.item_key(item);
            r.item_part(key, Neutral(item, neutral));
        }
    }
}

/// The host player's entity id.
const PLAYER: u32 = 42;
/// The LAN guest `with_guests` places as connection and entity 12.
const GUEST: u32 = 12;

/// A serving game with a survival host player, entity `PLAYER`, at `at`.
fn serving(game: &mut super::Game<'_>, at: Vec3) {
    game.options.dedicated = Some(Vec::new());
    game.player = Some(PlayerMotion::new(game.motion, at).unwrap());
    bind_local_player(game, PLAYER, u128::from(PLAYER));
    game.creative = false;
}

fn armor(head: Option<&str>, feet: Option<&str>) -> [Option<ItemStack>; 4] {
    [
        head.map(|i| ItemStack::new(i, 1)),
        None,
        None,
        feet.map(|i| ItemStack::new(i, 1)),
    ]
}

/// The host wears `head` and `feet` and nothing else; whether calm piglins leave them alone.
fn wearing(game: &mut super::Game<'_>, head: Option<&str>, feet: Option<&str>) -> bool {
    game.armor = armor(head, feet);
    ASKED.lock().unwrap().clear();
    game.player_view().unwrap().gold_armor
}

fn asked() -> Vec<(&'static str, u32)> {
    ASKED.lock().unwrap().clone()
}

#[test]
#[ignore = "daily"]
fn a_part_decides_whether_a_worn_piece_counts_as_gold() {
    crate::game::in_own_process(
        "game::native_makes_piglins_neutral_tests::a_part_decides_whether_a_worn_piece_counts_as_gold",
        a_part_decides_whether_a_worn_piece_counts_as_gold_isolated,
    );
}

fn a_part_decides_whether_a_worn_piece_counts_as_gold_isolated() {
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new("neutral", Box::new(NeutralMod))].into();
    mod_platform::testing::with_mods(mods.clone(), || {
        crate::game::api_values::install_parts(&mods).unwrap();
        with_game(|game| {
            serving(game, Vec3::new(0.5, 64.0, 0.5));
            assert!(wearing(game, Some("minecraft:leather_helmet"), None));
            assert_eq!(
                asked(),
                [("minecraft:leather_helmet", PLAYER)],
                "the part sees the wearer"
            );
            // Golden boots the part refuses don't count, though the tag lists them.
            assert!(!wearing(game, None, Some("minecraft:golden_boots")));
            assert_eq!(asked(), [("minecraft:golden_boots", PLAYER)]);
            // A tagged piece no part is attached to keeps vanilla's answer.
            assert!(wearing(game, Some("minecraft:golden_helmet"), None));
            assert!(asked().is_empty());
            assert!(!wearing(game, Some("minecraft:iron_helmet"), None));
            // Feet come first (`EquipmentSlotGroup.ARMOR`): boots that count end the question
            // before the helmet is asked; refused boots go on to the helmet.
            assert!(wearing(
                game,
                Some("minecraft:leather_helmet"),
                Some("minecraft:leather_boots")
            ));
            assert_eq!(asked(), [("minecraft:leather_boots", PLAYER)]);
            assert!(wearing(
                game,
                Some("minecraft:leather_helmet"),
                Some("minecraft:golden_boots")
            ));
            assert_eq!(
                asked(),
                [
                    ("minecraft:golden_boots", PLAYER),
                    ("minecraft:leather_helmet", PLAYER)
                ]
            );
        });
        // A LAN guest's view asks with the guest as the wearer.
        with_guests(|game| {
            game.options.dedicated = Some(Vec::new());
            let lan = game.lan.as_mut().unwrap();
            lan.hub.guests.update_all(|guest| {
                guest.loaded = true;
                if guest.conn == GUEST {
                    guest.armor = armor(Some("minecraft:leather_helmet"), None);
                }
            });
            ASKED.lock().unwrap().clear();
            let views = game.lan_guest_views();
            let gold = |conn: u32| views.iter().find(|(c, _)| *c == conn).unwrap().1.gold_armor;
            assert!(gold(GUEST));
            assert!(!gold(13));
            assert_eq!(asked(), [("minecraft:leather_helmet", GUEST)]);
        });
    });
}

#[test]
#[ignore = "daily"]
fn without_parts_the_piglin_safe_armor_tag_decides() {
    crate::game::in_own_process(
        "game::native_makes_piglins_neutral_tests::without_parts_the_piglin_safe_armor_tag_decides",
        without_parts_the_piglin_safe_armor_tag_decides_isolated,
    );
}

fn without_parts_the_piglin_safe_armor_tag_decides_isolated() {
    with_game(|game| {
        serving(game, Vec3::new(0.5, 64.0, 0.5));
        assert!(!wearing(game, Some("minecraft:leather_helmet"), None));
        assert!(wearing(game, None, Some("minecraft:golden_boots")));
        assert!(!wearing(game, None, None));
    });
}
