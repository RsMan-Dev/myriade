//! The test instance blocks of the level in place: where `/test` finds them
//! and where a new test goes.
use super::Game;

type Pos = (i32, i32, i32);

impl Game<'_> {
    /// `StructureUtils.findTestBlocks`: the test instance blocks within
    /// `radius` of `center` (the POI search is a sphere).
    pub(super) fn find_test_instance_blocks(&self, center: Pos, radius: i32) -> Vec<Pos> {
        let limit = i64::from(radius) * i64::from(radius);
        let mut found: Vec<Pos> = self
            .world
            .block_entities
            .iter()
            .filter(|(_, e)| {
                matches!(
                    e,
                    myriade_engine::containers::BlockEntity::TestInstanceBlock(_)
                )
            })
            .map(|(p, _)| *p)
            .filter(|p| {
                let d = [p.0 - center.0, p.1 - center.1, p.2 - center.2].map(i64::from);
                d[0] * d[0] + d[1] * d[1] + d[2] * d[2] <= limit
            })
            .collect();
        found.sort_unstable();
        found
    }

    /// The block of the source's position (`BlockPos.containing`).
    pub(super) fn test_source_block(&self) -> Pos {
        let p = self.source_position();
        (
            p[0].floor() as i32,
            p[1].floor() as i32,
            p[2].floor() as i32,
        )
    }

    /// The column `createTestPositionAround` starts from: the source's.
    pub(super) fn test_around(&self) -> (i32, i32) {
        let p = self.source_position();
        (p[0].floor() as i32, p[2].floor() as i32)
    }

    /// `createTestPositionAround`: three blocks south of the source, on the
    /// surface of the level in place .
    pub(in crate::game) fn test_position_in_level(&mut self, (x, z): (i32, i32)) -> Pos {
        let facts = self.res.facts();
        let surface = self.world.world_surface(x, z, |s| facts.is_air(s));
        (x, surface, z + 3)
    }

    /// `createTestPositionAround` in the level in place.
    pub(super) fn test_position_around(&mut self) -> Pos {
        let around = self.test_around();
        self.test_position_in_level(around)
    }
}
