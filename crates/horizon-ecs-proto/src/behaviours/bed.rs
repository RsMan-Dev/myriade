//! Un lit pour un dormeur : la mémoire du lit le plus proche (`NEAREST_BED`), le composant `Sleeping` (rare) et la place
//! occupée (`bed_taken`, lu sur les autres dormeurs). Position du couché : `pos_to_bed` (`bed_geometry`) n'existe pas dans l'ECS.
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::Pos;
use horizon_core::geometry::Vec3;

pub static NEAREST_BED: MemKey = MemKey::new("minecraft:nearest_bed", Shape::Block);

/// Le dormeur est couché dans ce lit.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Sleeping(pub [i32; 3]);

/// Un autre dormeur occupe déjà le lit `p`.
pub fn bed_taken(w: &mut World, me: Entity, p: [i32; 3]) -> bool {
    w.query::<(Entity, &Sleeping)>().iter(w).any(|(e, s)| e != me && s.0 == p)
}
/// Pose le corps au-dessus du lit (`setPosToBed`, hauteur 0,5625 ; approximé faute de `pos_to_bed`).
pub fn lie_down(w: &mut World, e: Entity, p: [i32; 3]) {
    if let Some(mut pos) = w.get_mut::<Pos>(e) {
        pos.0 = Vec3::new(f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5625, f64::from(p[2]) + 0.5);
    }
    w.entity_mut(e).insert(Sleeping(p));
}
