//! Breeze ground block: the block under a breeze's feet, fence and wall included.
use super::{BlockStateId, Game};
use horizon_client::mobs::Mob;

impl Game<'_> {
    /// `Breeze.emitGroundParticles`'s block: `getInBlockState`, else
    /// `getBlockStateOn`, which is `Entity.getOnPos(1.0E-5F)` — the supporting
    /// block the feet rest on, or the cell just below them. A fence, a wall and
    /// a fence gate have a 1.5-block collision, so a breeze on one has its feet
    /// half a block above that block's own cell, where the cell below them is
    /// air and only the supporting block has the material.
    pub(super) fn breeze_ground(&self, m: &Mob) -> ([i32; 3], BlockStateId) {
        let feet = [
            m.position.x.floor() as i32,
            m.position.y.floor() as i32,
            m.position.z.floor() as i32,
        ];
        let in_block = self.world.block(feet[0], feet[1], feet[2]);
        if !self.res.facts().is_air(in_block) {
            return (feet, in_block);
        }
        let on_ground = m
            .on_ground
            .then(|| self.supporting_block(m.position, m.bounds()));
        let at = on_ground.flatten().map_or(
            [
                feet[0],
                (m.position.y - f64::from(1.0e-5_f32)).floor() as i32,
                feet[2],
            ],
            |p| [p.x, p.y, p.z],
        );
        (at, self.world.block(at[0], at[1], at[2]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Audio, Options, Progress, Settings, load_resources};
    use horizon_client::chunk::Chunk;
    use horizon_client::mobs::Mob;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;

    /// A game on the prepared resources with an empty chunk at (0, 0), the
    /// window's player (the camera the dust needs) and one breeze standing
    /// with its feet at `feet`.
    fn game_with_breeze(feet: Vec3) -> Game<'static> {
        let root = std::path::PathBuf::from(
            std::env::var_os("HORIZON_TEST_RESOURCES")
                .expect("set HORIZON_TEST_RESOURCES to prepared resources"),
        );
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            false,
            crate::game::execution_role::ExecutionRole::Client,
            &Progress::new(),
            &mut |_, _| unreachable!(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::Client;
        let res: &'static _ = Box::leak(Box::new(loaded.resources));
        let mut game = Game::new(
            res,
            loaded.pack,
            loaded.gui.unwrap(),
            options,
            settings,
            Audio::silent(&horizon_assets::resource_source::ResourceSource::directory(
                root.clone(),
            )),
            None,
        )
        .unwrap();
        game.world
            .chunks
            .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        // Eight blocks south of the breeze: inside the 32 blocks the dust needs.
        game.player = Some(
            horizon_client::motion::PlayerMotion::new(game.motion, Vec3::new(8.5, 66.0, 16.5))
                .unwrap(),
        );
        let kind = game.res.mob_types()["minecraft:breeze"].clone();
        game.mobs.push(Mob::new(13001, kind, feet, 0.0));
        game
    }

    /// `Entity.getOnPos(1.0E-5F)` keeps a fence's, a wall's or a fence gate's
    /// own cell while the feet rest on its 1.5-block collision; on a full block
    /// it is the block below the feet. Reading 0.2 below the feet, as the dust
    /// used to, lands in air on all three.
    #[test]
    #[ignore = "daily"]
    fn the_ground_of_a_breeze_is_the_fence_wall_or_gate_it_stands_on() {
        let mut game = game_with_breeze(Vec3::new(8.5, 65.5, 8.5));
        let registry = &game.pack.assets.registry;
        let stone = registry.default_state("minecraft:stone").unwrap();
        game.world.set(8, 63, 8, stone).unwrap();
        game.mobs[0].on_ground = true;
        for name in [
            "minecraft:oak_fence",
            "minecraft:cobblestone_wall",
            "minecraft:oak_fence_gate",
        ] {
            let state = registry.default_state(name).unwrap();
            game.world.set(8, 64, 8, state).unwrap();
            assert_eq!(
                game.breeze_ground(&game.mobs[0]),
                ([8, 64, 8], state),
                "{name}"
            );
        }
        // The same breeze on the full block's top reads that block.
        game.world.set(8, 64, 8, stone).unwrap();
        game.mobs[0].position = Vec3::new(8.5, 65.0, 8.5);
        assert_eq!(game.breeze_ground(&game.mobs[0]), ([8, 64, 8], stone));
    }
}
