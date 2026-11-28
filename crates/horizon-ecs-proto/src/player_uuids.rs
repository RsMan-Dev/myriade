//! The uuid of each player the host connected (`PlayerUuids`), and the owners and leads that name a player or an entity by uuid and wait
//! for it. The save names a holder by its uuid alone (`EntityReference`, online or not): read back, the owner is the player when it has
//! joined, the entity when it is loaded, else it waits here. Nothing is dropped: a player stays `Who::Player(id)` / `Holder::Player(id)`.
use crate::leash::{attach, Holder};
use crate::projectile::Projectile;
use crate::tame::Owner;
use crate::targeting::Who;
use crate::tnt::Primed;
use bevy_ecs::prelude::*;
use std::collections::HashMap;

/// What a waiting entity takes from its holder when the holder appears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// `Owner` of a tamed animal or a mount.
    Tame,
    /// `Primed.owner` of a TNT.
    Tnt,
    /// The lead, tied to the player.
    Leash,
    /// Nothing to set: a stack keeps its thrower as the uuid itself.
    Item,
    /// `Projectile.owner`.
    Projectile,
    /// `CloudSpec.owner` of a potion cloud.
    Cloud,
    /// `PlayerCredit.player`.
    Credit,
    /// `Anger.at`: the player or entity the anger is for.
    Anger,
    /// One of the `Trust` slots of a fox: the holder is added to the first free one.
    Trust,
}

/// The players by id and by uuid, and the waiting entities by uuid. A uuid is held as one `u128`.
#[derive(Resource, Default)]
pub struct PlayerUuids {
    by_id: HashMap<u32, u128>,
    by_uuid: HashMap<u128, u32>,
    pending: HashMap<u128, Vec<(Entity, Slot)>>,
}

/// The uuid of player `id`; None when it never joined.
pub fn uuid_of(w: &World, id: u32) -> Option<u128> {
    w.get_resource::<PlayerUuids>()?.by_id.get(&id).copied()
}

/// The id of the player of `uuid`, kept once it left; None when no such player joined.
pub fn id_of(w: &World, uuid: u128) -> Option<u32> {
    w.get_resource::<PlayerUuids>()?.by_uuid.get(&uuid).copied()
}

/// `e` waits for the holder of `uuid` (a player's or an entity's), then takes `slot` (see `join` and `resolve_entity`).
pub fn wait(w: &mut World, uuid: u128, e: Entity, slot: Slot) {
    w.get_resource_or_insert_with(PlayerUuids::default).pending.entry(uuid).or_default().push((e, slot));
}

/// The player `id` joined with `uuid`: its entries are set, and the entities waiting for `uuid` take it. A lead that the distance refuses
/// waits for the next connection of the player.
pub fn join(w: &mut World, id: u32, uuid: u128) {
    let mut table = w.get_resource_or_insert_with(PlayerUuids::default);
    if let Some(old) = table.by_id.insert(id, uuid) {
        if table.by_uuid.get(&old) == Some(&id) {
            table.by_uuid.remove(&old);
        }
    }
    table.by_uuid.insert(uuid, id);
    let waiting = table.pending.remove(&uuid).unwrap_or_default();
    let again: Vec<(Entity, Slot)> = waiting.into_iter().filter(|&(e, slot)| w.get_entity(e).is_ok() && !pose(w, e, Who::Player(id), slot)).collect();
    if !again.is_empty() {
        w.get_resource_or_insert_with(PlayerUuids::default).pending.entry(uuid).or_default().extend(again);
    }
}

/// The entity `h` has `uuid`: the entities waiting for it take it as their holder.
pub fn resolve_entity(w: &mut World, uuid: u128, h: Entity) {
    let waiting = w.get_resource_or_insert_with(PlayerUuids::default).pending.remove(&uuid).unwrap_or_default();
    for (e, slot) in waiting {
        if w.get_entity(e).is_ok() {
            pose(w, e, Who::Entity(h), slot);
        }
    }
}

/// The player `id` left. Its uuid stays: an owner offline is still written by it, since the save names an absent player by its uuid.
pub fn leave(_w: &mut World, _id: u32) {}

/// Sets what `slot` holds of `e` to `who`. False when a lead is refused (too far): the caller keeps it waiting.
fn pose(w: &mut World, e: Entity, who: Who, slot: Slot) -> bool {
    let some = Some(who);
    match slot {
        Slot::Tame => {
            w.entity_mut(e).insert(Owner(some));
        }
        Slot::Tnt => {
            if let Some(mut p) = w.get_mut::<Primed>(e) {
                p.owner = some;
            }
        }
        Slot::Projectile => {
            if let Some(mut p) = w.get_mut::<Projectile>(e) {
                p.owner = some;
            }
        }
        Slot::Cloud => {
            if let Some(mut k) = w.get_mut::<crate::effect_cloud::CloudSpec>(e) {
                k.owner = some;
            }
        }
        Slot::Leash => {
            if let Who::Player(id) = who {
                return attach(w, e, Holder::Player(id));
            }
        }
        Slot::Credit => {
            if let (Who::Player(id), Some(mut k)) = (who, w.get_mut::<crate::credit::PlayerCredit>(e)) {
                k.player = Some(id);
            }
        }
        Slot::Anger => {
            if let Some(mut a) = w.get_mut::<crate::behaviours::neutral_anger::Anger>(e) {
                a.at = some;
            }
        }
        Slot::Trust => {
            if let Some(mut t) = w.get_mut::<crate::behaviours::trust::Trust>(e) {
                t.add(who);
            }
        }
        Slot::Item => {}
    }
    true
}
