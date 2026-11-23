//! The track under a minecart: `BaseRailBlock.isRail` and the shape a cart reads,
//! which the rail's parts may answer (`rail_direction`).
use super::Game;
use super::minecarts::{Rail, Shape};
use horizon_world::level::classes::Class;

impl Game<'_> {
    /// The rail at (x, y, z): a block
    /// in `#minecraft:rails` built on `BaseRailBlock` (`BaseRailBlock.isRail`), with the shape
    /// its parts answer (`IBaseRailBlockExtension.getRailDirection`).
    pub(super) fn rail(&self, x: i32, y: i32, z: i32) -> Option<Rail> {
        let state = self.world.block(x, y, z);
        let name = self.res.facts().get(state).name.clone();
        let tagged = self
            .items()
            .block_tags
            .get("minecraft:rails")
            .is_some_and(|rails| rails.contains(&*name));
        if !tagged || !self.res.behaviours().data(state).is(Class::BaseRail) {
            return None;
        }
        let s = self.pack.assets.registry.state(state)?;
        let text = s.properties.get("shape").map_or("", String::as_str);
        let powered = s.properties.get("powered").is_some_and(|v| v == "true");
        let shape = Shape::parse(text);
        Some(Rail {
            name,
            shape,
            powered,
        })
    }
}
