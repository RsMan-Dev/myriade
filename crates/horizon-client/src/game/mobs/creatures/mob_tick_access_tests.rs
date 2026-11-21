use super::*;
use crate::game::Dim;
use crate::game::native_entity_refs::tests::with_game;
use horizon_client::chunk::Chunk;
use mod_api::{ModEntry, ServerHost};
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

/// Extends the lifetime of the dropped item it names, through `extra_life`.
struct ExtendLife {
    id: Arc<AtomicU32>,
}

#[mod_api::__subscriptions]
impl mod_api::Hooks for ExtendLife {
    fn item_expire(&self, _: &mut dyn ServerHost, event: &mut mod_api::ItemExpireEvent<'_>) {
        if event.entity == self.id.load(Ordering::Relaxed) {
            event.extra_life += 100;
        }
    }
}

/// The item family keeps its own contract while the mob slice changes: the
/// item tick still lifts its list, and an `item_expire` listener still edits
/// the item's lifetime, which the engine applies when the tick ends.
#[test]
#[ignore = "daily"]
fn item_lifetime_edit_in_the_item_tick_still_lands() {
    let named = Arc::new(AtomicU32::new(0));
    let mods: mod_api::Mods = vec![ModEntry::new(
        "item-lifetime-access",
        Box::new(ExtendLife { id: named.clone() }),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
        let root = std::env::temp_dir().join(format!(
            "horizon-item-lifetime-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        with_game(|game| {
            game.world_root = root.clone();
            game.enter_level(Dim::Overworld).unwrap();
            game.options.dedicated = Some(Vec::new());
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            game.push_item_entity(
                ItemStack::new("minecraft:apple", 1),
                [1.5, 64.0, 1.5],
                Some([0.0, 0.0, 0.0]),
                0,
                None,
            );
            let id = game.dropped.last().unwrap().id;
            named.store(id, Ordering::Relaxed);
            game.ticked_entities_through = id;
            game.dropped.last_mut().unwrap().age = 6000;
            let mut round = game.native_tick_round();
            game.tick_dropped_items(&mut round);
            let item = game
                .dropped
                .iter()
                .find(|item| item.id == id)
                .expect("the extended item stayed");
            assert_eq!(
                item.age, 5900,
                "the listener's 100 ticks landed on the item's age"
            );
        });
        let _ = std::fs::remove_dir_all(&root);
    });
}
