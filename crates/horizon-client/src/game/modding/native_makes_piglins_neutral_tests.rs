//! `makes_piglins_neutral` answered by item Parts on vanilla items: leather boots and a leather
//! helmet made to count as gold, golden boots made not to. The answer is what the view of the
//! player the piglins read carries (`gold_armor`, `PiglinAi.isWearingSafeArmor`), for the host
//! and for each LAN guest, and a calm piglin targets only a player whose view has none
//! (`NEAREST_TARGETABLE_PLAYER_NOT_WEARING_GOLD`).
use super::native_entity_refs::tests::{
    bind_local_player, with_game,
};
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
