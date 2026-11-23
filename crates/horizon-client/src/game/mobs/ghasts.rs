//! Happy ghasts on the game side: putting a harness on (and shearing it
//! off), riding, getting off on top, and the ghastling a dried ghast block
//! hatches.
use super::Game;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_world::registry::BlockStateId;

impl Game<'_> {
    /// Client `EntityGetter.getEntityCollisions` for the player: the
    /// entities that `canBeCollidedWith` it, solid happy ghasts, living
    /// shulkers (`Shulker.canBeCollidedWith`) and boats
    /// (`AbstractBoat.canBeCollidedWith`), except the player's own vehicle.
    pub(super) fn player_entity_collisions(&self) -> Vec<Aabb> {
        let Some(p) = &self.player else {
            return Vec::new();
        };
        let sweep = Aabb::new(p.position.subtract(Vec3::new(3.0, 1.0, 3.0)), p.position.add_vector(Vec3::new(3.0, 4.0, 3.0)));
        // RESTE-SANS-ECS: the solid happy ghasts and living shulkers are assumed to come from `solid_boxes` (to check in horizon-ecs-proto).
        self.ecs.solid_boxes(sweep, Some(p.position.y), self.riding)
    }
    /// `EntityGetter.getEntityCollisions` for a mover that is neither a player nor a
    /// happy ghast (a dropped item, an armor stand, primed TNT, a falling block): the
    /// entities whose `canBeCollidedWith(mover)` is true, the same for all of them. A
    /// boat always, a living shulker, and a happy ghast only while it is held still
    /// (`isOnStillTimeout`): a player on its back keeps it still, so an item dropped there
    /// stays on top. A minecart keeps `Entity`'s false (`AbstractMinecart` overrides only
    /// `canCollideWith`), so an item falls through it.
    /// Read from the ECS index cells over each of the `sweeps` (P `solid_boxes`), without repeats.
    pub(super) fn item_entity_collisions(&self, sweeps: impl IntoIterator<Item = Aabb>) -> Vec<Aabb> {
        let mut out: Vec<Aabb> = Vec::new();
        for sweep in sweeps {
            for b in self.ecs.solid_boxes(sweep, None, None) {
                if !out.contains(&b) {
                    out.push(b);
                }
            }
        }
        out
    }
    /// `DriedGhastBlock.spawnGhastling`: a ghastling where the block was. RESTE-SANS-ECS: its baby age and
    /// facing yaw are not set (no ECS setter), so it hatches as an adult facing the default way.
    pub(super) fn spawn_ghastling(&mut self, pos: [i32; 3], _state: BlockStateId) {
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]),
            f64::from(pos[2]) + 0.5,
        ];
        if self.spawn_mob_raw("minecraft:happy_ghast", at, true).is_some() {
            // DriedGhastBlock.spawnGhastling: level.playSound(null, ghastling, GHASTLING_SPAWN, BLOCKS, 1, 1).
            self.audio
                .play("minecraft:entity.ghastling.spawn", Some(at), 1.0, 1.0);
        }
    }
}

#[cfg(test)]
mod item_collision_tests {
    use crate::game::{Game, Options, Progress, Settings, load_resources};
    use horizon_client::chunk::Chunk;
    use horizon_core::position::ChunkPos;
    use std::sync::Arc;

    /// An item dropped onto a minecart falls through it to the floor, and one
    /// dropped onto a boat rests on the boat (`canBeCollidedWith`).
    #[test]
    #[ignore = "daily"]
    fn a_dropped_item_rests_on_a_boat_but_falls_through_a_minecart() {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
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
        options.game_dir = Some(
            std::env::temp_dir().join(format!("horizon-item-cart-unit-{}", std::process::id())),
        );
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);
        let stone = game
            .pack
            .assets
            .registry
            .default_state("minecraft:stone")
            .unwrap()
            .0;
        let mut chunk = Chunk::empty(0, 0);
        for x in 0..16 {
            for z in 0..16 {
                chunk.set(x, 199, z, stone);
            }
        }
        game.world.chunks.insert((0, 0), Arc::new(chunk));
        game.block_ticks
            .add_empty_chunk(ChunkPos::new(0, 0))
            .unwrap();
        game.load_ticking_neighbourhood(0, 0);
        assert!(game.run_command("forceload add 0 0"));
        assert!(game.run_command("summon minecart 4.5 200 4.5"));
        assert!(game.run_command("summon oak_boat 10.5 200 10.5"));
        for (x, z) in [(4.5, 4.5), (10.5, 10.5)] {
            assert!(game.run_command(&format!(
                "summon item {x} 201.5 {z} {{Item:{{id:\"minecraft:stick\",count:1}},Motion:[0.0d,0.0d,0.0d]}}"
            )));
        }
        for _ in 0..60 {
            game.tick_world(true).unwrap();
        }
        let rest = |x: f64| {
            game.ecs.item_views()
                .iter()
                .find(|d| (d.position.x - x).abs() < 0.5)
                .map(|d| d.position.y)
                .unwrap()
        };
        assert_eq!(rest(4.5), 200.0, "through the minecart to the stone");
        let boat_top = game.ecs.bounds(game.ecs.boat_ids()[0]).unwrap().max_position().y;
        assert_eq!(rest(10.5), boat_top, "on the boat");
    }
}
