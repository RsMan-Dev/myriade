//! Who a creature trusts (`Fox.getTrustedEntities`): two slots, the first one filled first; a trusted player is indexed by its id.
//! A trusted entity is not avoided, and what hurts it is defended against (`defend_trusted`).
use super::*;
use crate::{tame, targeting::Who};

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Trust(pub [Option<Who>; 2]);

impl Trust {
    /// `addTrustedEntity`: the first free slot, the second one overwritten when both are used.
    pub fn add(&mut self, who: Who) {
        let slot = usize::from(self.0[0].is_some());
        self.0[slot] = Some(who);
    }
    pub fn trusts(&self, who: Who) -> bool {
        self.0.contains(&Some(who))
    }
    pub fn trusts_player(&self, id: u32) -> bool {
        self.trusts(Who::Player(id))
    }
}

/// The player `id` is trusted by `e` (`player_when` of an avoidance: a trusted player is spared). Read with the world.
pub fn spares_player(w: &World, e: Entity, id: u32) -> bool {
    w.get::<Trust>(e).is_some_and(|t| t.trusts_player(id))
}

/// The target of an avoidance is not tamed (`seen_when`: `!tame`).
pub fn untamed(w: &World, t: Entity) -> bool {
    w.get::<tame::Owner>(t).is_none_or(|o| o.0.is_none())
}
