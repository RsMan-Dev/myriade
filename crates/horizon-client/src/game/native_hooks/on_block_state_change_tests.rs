#![cfg(test)]
//! `on_block_state_change` heard by a block Part at the end of `Level.setBlock`: after the
//! neighbours' shapes settled, where `block_changed` hears the write before them, and on a
//! client for each block the server sends and each write of a piston's replay.
use crate::game::native_entity_refs::tests::with_game;
use horizon_core::{geometry::Direction, position::BlockPos};
use horizon_world::level::LevelAccess;
use mod_api::{
    BlockChangedEvent, BlockPart, BlockState, Hooks, LevelHost, LevelSide, OnBlockStateChangeEvent,
    Registrar,
};
use std::sync::{Arc, Mutex};

/// One notice: the side, the position, the state asked, old, new, and whether the fence west
/// of the position showed an east side then.
type Heard = (bool, [i32; 3], BlockState, BlockState, BlockState, bool);

/// Whether the block west of `pos` is a fence connected east.
fn fence_east(level: &dyn LevelHost, pos: [i32; 3]) -> bool {
    level
        .block_state([pos[0] - 1, pos[1], pos[2]])
        .is_some_and(|fence| level.state_with_property(fence, "east", "true") == Some(fence))
}

#[derive(Clone, Default)]
struct Listener(Arc<Mutex<Vec<Heard>>>);
#[mod_api::horizon_part]
impl BlockPart for Listener {
    fn on_block_state_change(&self, level: LevelSide<'_>, event: &mut OnBlockStateChangeEvent) {
        assert!(
            !event.predicted,
            "server writes and piston replays are not predictions"
        );
        let (client, level) = match level {
            LevelSide::Server(level) => (false, level),
            LevelSide::Client(level) => (true, level),
        };
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        // The write holds `new`.
        assert_eq!(level.block_state(pos), Some(event.new));
        self.0.lock().unwrap().push((
            client,
            pos,
            event.state,
            event.old,
            event.new,
            fence_east(level, pos),
        ));
    }
}

struct Mod {
    listener: Listener,
    /// What `block_changed` saw of the fence beside each change.
    changed: Arc<Mutex<Vec<bool>>>,
}
#[mod_api::__subscriptions]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
        for name in ["minecraft:stone", "minecraft:gold_block"] {
            let key = r.block_key(name);
            r.block_part(key, self.listener.clone());
        }
    }
    fn block_changed(&self, level: &dyn LevelHost, event: &mut BlockChangedEvent) {
        let pos = [event.pos.x, event.pos.y, event.pos.z];
        self.changed.lock().unwrap().push(fence_east(level, pos));
    }
}

fn mods(listener: &Listener, changed: &Arc<Mutex<Vec<bool>>>) -> mod_api::Mods {
    vec![mod_api::ModEntry::new(
        "state_change",
        Box::new(Mod {
            listener: listener.clone(),
            changed: changed.clone(),
        }),
    )]
    .into()
}

#[test]
#[ignore = "daily"]
fn a_part_hears_a_state_change_after_the_neighbours_shapes_settled() {
    let listener = Listener::default();
    let changed = Arc::new(Mutex::new(Vec::new()));
    let mods = mods(&listener, &changed);
    crate::game::api_values::install_parts(&mods).unwrap();
    mod_platform::testing::with_mods(mods, || {
        with_game(|game| {
            game.world
                .chunks
                .insert((0, 0), Arc::new(horizon_client::chunk::Chunk::empty(0, 0)));
            let registry = game.pack.assets.registry;
            let [fence, stone, dirt] = ["minecraft:oak_fence", "minecraft:stone", "minecraft:dirt"]
                .map(|name| registry.default_state(name).unwrap());
            game.world.set(1, 64, 0, fence).unwrap();
            game.with_level(|b, level, ticks| {
                b.set_block(level, ticks, BlockPos::new(2, 64, 0), stone, 3);
            });
            // `block_changed` saw the stone beside an unconnected fence (then the fence's own
            // change); the part hears the stone beside the connected fence.
            assert_eq!(changed.lock().unwrap().first(), Some(&false));
            let heard = listener.0.lock().unwrap().clone();
            assert_eq!(heard.len(), 1);
            let (client, pos, asked, _, new, east) = heard[0];
            assert_eq!((client, pos, east), (false, [2, 64, 0], true));
            assert_eq!((asked, new), (api(stone), api(stone)));
            // Stone to dirt: stone's part hears it, dirt has none.
            game.with_level(|b, level, ticks| {
                b.set_block(level, ticks, BlockPos::new(2, 64, 0), dirt, 3);
            });
            let heard = listener.0.lock().unwrap().clone();
            assert_eq!(heard.len(), 2);
            assert_eq!((heard[1].2, heard[1].4), (api(stone), api(dirt)));
        })
    });
}

/// The API state of the engine state `state`.
fn api(state: horizon_world::registry::BlockStateId) -> BlockState {
    use crate::game::api_values::TryIntoApi;
    state.try_into_api().unwrap()
}
