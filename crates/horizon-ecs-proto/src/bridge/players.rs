//! The players of the host, as facts and not as entities: `Online` holds their place and items (the ECS reads them by bin), `PlayerFacts`
//! holds what `Online` does not (team, life, targetability). A fact is written only when it differs from the last one.
use crate::goals::{Online, Player};
use bevy_ecs::prelude::*;
use std::collections::HashMap;

/// What the ECS reads of a player beyond `Online`: whether it is alive, whether a creature may target it. Its team is in `bridge::teams`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerFact {
    pub alive: bool,
    pub targetable: bool,
}

#[derive(Resource, Default)]
pub struct PlayerFacts(pub HashMap<u32, PlayerFact>);

/// The same place and items (`Online` keeps the old one when they are equal).
fn same(a: &Player, b: &Player) -> bool {
    (a.x, a.y, a.z, a.main, a.off, a.head, a.chest, a.legs, a.feet) == (b.x, b.y, b.z, b.main, b.off, b.head, b.chest, b.legs, b.feet)
}

/// Sets the player `p` and its facts. Nothing is written for a fact or a place that did not change.
pub fn set_player(world: &mut World, p: Player, state: PlayerFact) {
    world.init_resource::<Online>();
    world.init_resource::<PlayerFacts>();
    if world.resource::<PlayerFacts>().0.get(&p.id) != Some(&state) {
        world.resource_mut::<PlayerFacts>().0.insert(p.id, state);
    }
    let moved = world.resource::<Online>().get(p.id).is_none_or(|o| !same(&o, &p));
    if moved {
        world.resource_mut::<Online>().set(p);
    }
    crate::hook_wait::owner_changed(world, p.id, Some(p));
}

/// Removes the player `id` and its facts: nothing of it stays.
pub fn remove_player(world: &mut World, id: u32) {
    world.init_resource::<Online>();
    world.init_resource::<PlayerFacts>();
    crate::hook_wait::owner_changed(world, id, None);
    world.resource_mut::<Online>().remove(id);
    world.resource_mut::<PlayerFacts>().0.remove(&id);
}
