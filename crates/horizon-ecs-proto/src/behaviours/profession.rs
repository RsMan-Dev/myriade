//! Le métier d'un villageois, par son point d'intérêt : posé quand le poste est pris (`on_acquired`), retiré quand il est
//! perdu (`on_lost`). Écrit par ces deux événements seulement ; aucun parcours.
//! Le choix d'un poste et l'ordre de la table des métiers (`villager_professions.rs`) restent à la charge du tronc.
use crate::brain::*;
use crate::poi::PoiRegistry;
use horizon_world::poi::PoiType;

/// Le métier : le type du poste qui le donne ; absent = sans métier.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[component(storage = "SparseSet")]
pub struct Profession(pub PoiType);

/// Le poste est pris : le métier de son type est posé (le type se lit dans le registre au même point).
pub fn on_acquired(w: &mut World, e: Entity, poi: [i32; 3]) {
    let Some(kind) = w.resource::<PoiRegistry>().get(poi).map(|p| p.kind) else { return };
    if kind.is_job() {
        w.entity_mut(e).insert(Profession(kind));
    }
}

/// Le poste est perdu : le métier part.
pub fn on_lost(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<Profession>();
}
