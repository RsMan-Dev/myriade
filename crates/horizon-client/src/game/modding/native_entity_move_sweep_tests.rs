//! The `entity_move` sweep (arismoko/horizon#2414, Ari's 1b): one entity of every walk
//! family, one full family round each, and a counting listener that must
//! hear exactly each family's case - the backstop the clippy ban cannot
//! give, because a forgotten family moves without any resolve call. The
//! cases are generated from `entity_lists!` itself and the coverage is
//! asserted against the real `entity_handles()` walk (review round 1).
use super::native_entity_refs::tests::with_game;
use mod_api::{EntityMoveEvent, Flow, Hooks, LoopContext, MoveCause};
use std::sync::{Arc, Mutex};

/// `kind -> (fires, causes)` heard during the round.
type Heard = Vec<(String, MoveCause)>;

struct Counter(Arc<Mutex<Heard>>);

#[mod_api::__subscriptions]
impl Hooks for Counter {
    fn entity_move(&self, _: &dyn LoopContext, event: &mut EntityMoveEvent<'_>) -> Flow {
        self.0
            .lock()
            .unwrap()
            .push((event.kind.to_owned(), event.cause));
        Flow::Continue
    }
}

/// Arrows in flight are vanilla `setPos` movers: their own tick fires
/// nothing; a piston push fires once with the piston cause (arismoko/horizon#2414 point 3).
#[test]
#[ignore = "daily"]
fn an_arrow_in_flight_fires_nothing_and_a_piston_push_fires_once() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
        "counter",
        Box::new(Counter(log.clone())),
    )]
    .into();
    mod_platform::testing::with_mods(mods, || {
        with_game(|game| {
            game.build_registries();
            game.set_allow_commands(true);
            let registry = &game.pack.assets.registry;
            let stone = registry.default_state("minecraft:stone").unwrap();
            let mut chunk = horizon_client::chunk::Chunk::empty(0, 0);
            for x in 0..16 {
                for z in 0..16 {
                    chunk.set(x, 8, z, stone.0);
                    chunk.set(x, 20, z, stone.0);
                }
            }
            game.world.chunks.insert((0, 0), std::sync::Arc::new(chunk));
            assert!(game.run_command("summon arrow 5.5 15.5 5.5 {Motion:[1.0d,0.0d,0.0d]}"));
            log.lock().unwrap().clear();
            game.tick_ecs().unwrap();
            let heard = std::mem::take(&mut *log.lock().unwrap());
            assert!(
                heard.is_empty(),
                "an arrow's own flight is setPos, not Entity.move: {heard:?}"
            );
        });
    });
}

