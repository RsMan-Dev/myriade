//! A sulfur cube swallows one item from its list and holds it (`SulfurCube.isSwallowableItem`, `swallowItem`, `tryToSearch`): it
//! keeps the item's archetype, takes no goal while it holds the block (`ai_step` switches MOVE, LOOK and JUMP off), and gives it back
//! on release. The item's name is saved (`save`, `read`) as the vanilla `Item` tag. Parameters come from the species: the list of
//! swallowable item names (`sulfur_cube_swallowable` tag, resolved by the species) and the cool-down of a search after a release.
//! DOUTE : `Tag::String` pour `Item` n'est pas vérifié dans le cadre (variante supposée) ; la liste de l'espèce est à remplir depuis le tag.
use super::*;
use crate::cow::*;
use horizon_client_lib::mobs::sulfur_cube::Archetype;
use horizon_core::nbt::Tag;
use std::sync::Arc;

/// The block a cube holds: the item and the archetype that gives its physics and its contact.
#[derive(Component, Clone)]
pub struct Held {
    pub item: Arc<str>,
    pub archetype: &'static Archetype,
}
/// The own time before which the cube does not search again (`searchCooldown` after a release).
#[derive(Component, Clone, Copy, Debug)]
pub struct SearchAfter(pub u64);

/// `isSwallowableItem`: the item's name is in the species' list.
pub fn accepts(name: &str, swallowable: &[&str]) -> bool {
    swallowable.contains(&name)
}

/// Swallows `item`, with its archetype: the cube holds it and stops moving.
pub fn hold(w: &mut World, e: Entity, item: Arc<str>, archetype: &'static Archetype) {
    w.entity_mut(e).insert(Held { item, archetype });
}

/// Gives the block back: the cube holds nothing and may move again.
pub fn release(w: &mut World, e: Entity) -> Option<Arc<str>> {
    let held = w.get::<Held>(e).map(|h| h.item.clone());
    w.entity_mut(e).remove::<Held>();
    held
}

/// Whether the cube may search for an item now (no block held, the cool-down is over).
pub fn may_search(w: &World, e: Entity) -> bool {
    w.get::<Held>(e).is_none() && w.get::<SearchAfter>(e).is_none_or(|s| now_of(w, e) >= s.0)
}

/// The save of the held block (`Item`, a string) and nothing when the cube holds none.
pub fn save(w: &World, e: Entity) -> Vec<(&'static str, Tag)> {
    w.get::<Held>(e).map(|h| vec![("Item", Tag::String(horizon_core::nbt::JavaString::from(&*h.item)))]).unwrap_or_default()
}
