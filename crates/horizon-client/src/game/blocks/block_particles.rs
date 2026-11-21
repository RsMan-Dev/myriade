//! The sprite and tint of a block's faces and particle material.
use super::*;

impl Game<'_> {
    /// `BlockStateModelSet.getParticleMaterial`: the sprite of the model's
    /// `particle` texture, else its first face (entity-rendered blocks may
    /// have a particle texture without faces, and faces without one).
    pub(super) fn block_particle_sprite(
        pack: &PackResources,
        state: BlockStateId,
    ) -> Option<String> {
        let catalog = &pack.render.model_catalog;
        let layer = catalog.particle(state).or_else(|| {
            let models = catalog
                .collect(state, horizon_core::position::BlockPos::new(0, 0, 0))
                .ok()?;
            models
                .iter()
                .flat_map(|(_, m)| m.faces.iter())
                .next()
                .map(|f| f.layer)
        })?;
        pack.layer_names.get(&layer).cloned()
    }

    pub(super) fn particle_shape(&self, pos: [i32; 3], state: BlockStateId) -> Vec<[f64; 6]> {
        let f = self.res.facts().get(state);
        let offset = if f.outline_offset {
            self.pack
                .context
                .tables
                .offset(state, pos[0], pos[2])
                .unwrap_or([0.0; 3])
        } else {
            [0.0; 3]
        };
        f.outline
            .iter()
            .map(|b| std::array::from_fn(|i| b[i] + offset[i % 3]))
            .collect()
    }
}

impl Game<'_> {
    /// `TerrainParticle` at `pos`: the model's particle sprite and tint source
    /// 0's `colorAsTerrainParticle` (white when the block has none).
    pub(super) fn particle_sprite(
        &self,
        state: BlockStateId,
        pos: [i32; 3],
    ) -> Option<(String, u32)> {
        let sprite = Self::block_particle_sprite(&self.pack, state)?;
        let chunks = &self.world.chunks;
        let (cx, cz) = (pos[0] >> 4, pos[2] >> 4);
        let region: [[Option<&horizon_client::chunk::Chunk>; 3]; 3] = std::array::from_fn(|dz| {
            std::array::from_fn(|dx| {
                chunks
                    .get(&(cx + dx as i32 - 1, cz + dz as i32 - 1))
                    .map(|c| &**c)
            })
        });
        let tint = self
            .pack
            .context
            .tables
            .terrain_particle_tint(
                state,
                pos,
                &region,
                world_biomes!(self),
                self.streaming.biome_zoom,
                self.streaming.biome_blend,
                self.streaming.nether_shading,
            )
            .unwrap_or(0xFFFFFF);
        Some((sprite, tint))
    }
    /// `BlockTransformer.TransformParticle.send`: its level event.
    pub(super) fn transform_particle(&mut self, particle: &str, pos: [i32; 3]) {
        let source = self
            .lan_acting_uuid
            .map(horizon_world::level::LevelEventPlayer);
        match particle {
            "scrape" => self.level_event_from(source, 3005, pos, 0),
            "wax_on" => self.level_event_from(source, 3003, pos, 0),
            "wax_off" => self.level_event_from(source, 3004, pos, 0),
            _ => {}
        }
    }
}

impl Game<'_> {
    /// The block's first up face (or any face) and its fixed tint, for block
    /// models drawn as entities (falling blocks, carried blocks).
    pub(super) fn falling_block_face_sprite(&self, state: BlockStateId) -> Option<(String, u32)> {
        let pack = &self.pack;
        let models = pack
            .render
            .model_catalog
            .collect(state, horizon_core::position::BlockPos::new(0, 0, 0))
            .ok()?;
        let face = models
            .iter()
            .flat_map(|(_, m)| m.faces.iter())
            .find(|f| f.quad.facing == horizon_assets::model::Direction::Up)
            .or_else(|| models.iter().flat_map(|(_, m)| m.faces.iter()).next())?;
        let name = pack.layer_names.get(&face.layer)?.clone();
        let tint = if face.quad.tint_index >= 0 {
            let n = &self.res.facts().get(state).name;
            if n.contains("leaves") || n.contains("vine") {
                0x48B518
            } else {
                0x91BD59
            }
        } else {
            0xFFFFFF
        };
        Some((name, tint))
    }
}
