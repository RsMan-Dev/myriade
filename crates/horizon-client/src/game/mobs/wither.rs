//! The wither on the game side: building it (`WitherSkullBlock.checkSpawn`),
//! its boss bar, and its skulls hitting and exploding.
use super::Game;
use horizon_client::block_pattern::{BlockPattern, PatternMatch, WITHER_FULL};
use horizon_client::items::ItemStack;
use horizon_core::{geometry::Direction, position::BlockPos};

impl Game<'_> {
    /// `LivingEntity.createWitherRose` for a dying player (`ServerPlayer.die`),
    /// whose kill credit is `killer`.
    pub(super) fn create_wither_rose_by(&mut self, position: [f64; 3], killer: Option<u32>) {
        let wither = killer.filter(|&id| self.is_wither(id));
        self.create_wither_rose_at(position, wither);
    }
    /// `getKillCredit` naming a wither that is still in the level.
    fn is_wither(&self, id: u32) -> bool {
        self.ecs.species(id) == Some("minecraft:wither") && self.ecs.alive(id)
    }
    fn create_wither_rose_at(&mut self, position: [f64; 3], wither: Option<u32>) {
        let Some(wither) = wither else {
            return;
        };
        let pos = [
            position[0].floor() as i32,
            position[1].floor() as i32,
            position[2].floor() as i32,
        ];
        let registry = self.pack.assets.registry;
        let rose = registry.default_state("minecraft:wither_rose");
        // NeoForge: `canEntityGrief` for the wither.
        if self.can_entity_grief(Some(wither))
            && let Some(rose) = rose
            && self
                .res
                .facts()
                .is_air(self.world.block(pos[0], pos[1], pos[2]))
            && self.with_level(|b, level, ticks| {
                b.can_survive(level, ticks, BlockPos::new(pos[0], pos[1], pos[2]), rose)
            })
        {
            let _ = self.level_set(pos, rose);
            return;
        }
        self.spawn_item_own(ItemStack::new("minecraft:wither_rose", 1), position, 0);
    }
    fn is_wither_skull(&self, p: [i32; 3]) -> bool {
        let name = &*self
            .res
            .facts()
            .get(self.world.block(p[0], p[1], p[2]))
            .name;
        matches!(
            name,
            "minecraft:wither_skeleton_skull" | "minecraft:wither_skeleton_wall_skull"
        )
    }
    /// `#minecraft:wither_summon_base_blocks` (soul sand and soul soil).
    pub(super) fn is_soul_base(&self, p: [i32; 3]) -> bool {
        let name = &*self
            .res
            .facts()
            .get(self.world.block(p[0], p[1], p[2]))
            .name;
        self.res
            .items()
            .block_tags
            .get("minecraft:wither_summon_base_blocks")
            .is_some_and(|t| t.contains(name))
    }
    /// `pattern.find(level, pos)` for the wither patterns: '^' a wither
    /// skeleton skull, '#' a summon base block, '~' air.
    pub(super) fn find_wither_pattern(
        &self,
        pattern: BlockPattern,
        pos: [i32; 3],
    ) -> Option<PatternMatch> {
        pattern.find(pos, |key, p| match key {
            '^' => self.is_wither_skull(p),
            '#' => self.is_soul_base(p),
            _ => self.res.facts().is_air(self.world.block(p[0], p[1], p[2])),
        })
    }
    /// `WitherSkullBlock.checkSpawn`, after a wither skeleton skull is
    /// placed: the T of three skulls on four soul sand or soul soil, built in
    /// any orientation, becomes a wither.
    pub(super) fn check_wither_spawn(&mut self, placed: [i32; 3]) -> horizon_client::Result<()> {
        // Nothing is built in Peaceful (`level.getDifficulty() != PEACEFUL`).
        if self.session.difficulty == 0 || !self.is_wither_skull(placed) {
            return Ok(());
        }
        let Some(found) = self.find_wither_pattern(WITHER_FULL, placed) else {
            return Ok(());
        };
        // Clear the pattern, the wither appears on the middle of the bottom
        // row facing along the pattern's forwards axis, then neighbours update.
        let cells = found.front_cells();
        self.clear_pattern_blocks(&cells)?;
        let base = found.block(1, 2, 0);
        let at = [
            f64::from(base[0]) + 0.5,
            f64::from(base[1]) + 0.55,
            f64::from(base[2]) + 0.5,
        ];
        let yaw = if matches!(found.forwards, Direction::West | Direction::East) {
            0.0
        } else {
            90.0
        };
        let id = self.create_triggered("minecraft:wither", at, yaw);
        // WitherBoss.makeInvulnerable: a third of its health. RESTE-SANS-ECS: the
        // charging counter and the persistence flag have no ECS write yet.
        if let Some(id) = id {
            if let Some(max) = self.ecs.max_health(id) {
                self.ecs.set_health(id, max / 3.0);
            }
        }
        // SUMMONED_ENTITY for each player within 50 blocks of the wither's box.
        self.trigger_summoned(id, 50.0);
        self.update_pattern_blocks(&cells)
    }
}
/// `!target.isAlive()` after a hit that landed: a totem that fired keeps the
/// player alive.
pub(super) fn killed(hurt: bool, dead_or_dying: bool, totem_used: bool) -> bool {
    hurt && dead_or_dying && !totem_used
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_totem_that_fires_is_no_kill() {
        assert!(killed(true, true, false));
        // The totem restores health next tick: alive inside hurtServer.
        assert!(!killed(true, true, true));
        assert!(!killed(true, false, false));
        assert!(!killed(false, true, false));
    }
}
