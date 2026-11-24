//! Un lit pour un dormeur : la mémoire du lit le plus proche (`NEAREST_BED`), le composant `Sleeping` (rare) et la place
//! occupée (`bed_taken`, lu sur les autres dormeurs). Position du couché : `pos_to_bed` (`bed_geometry`) n'existe pas dans l'ECS.
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::{Level, Pos};
use crate::nbt::{accept, owner, Field};
use crate::rules::bed::lie_spot;
use horizon_core::nbt::{Compound, Tag};
use std::borrow::Cow;

pub static NEAREST_BED: MemKey = MemKey::new("minecraft:nearest_bed", Shape::Block);

/// Le dormeur est couché dans ce lit.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet", on_add = box_changed, on_remove = box_changed)]
pub struct Sleeping(pub [i32; 3]);

/// `Villager.addAdditionalSaveData` writes `sleeping_pos` only while asleep; the position is the bed's block, read back as is.
pub const FIELDS: &[Field] = &[Field { key: Cow::Borrowed("sleeping_pos"), write: write_sleeping, read: read_sleeping, accepts: is_pos, owner: Some(owner::<Sleeping>) }];

fn is_pos(t: &Tag) -> bool {
    matches!(t, Tag::IntArray(v) if v.len() == 3)
}

fn write_sleeping(w: &World, e: Entity, c: &mut Compound) {
    if let Some(s) = w.get::<Sleeping>(e) {
        c.insert("sleeping_pos".into(), Tag::IntArray(s.0.to_vec()));
    }
}

/// Reads the bed back without moving the body: its position is saved with it.
fn read_sleeping(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    let [x, y, z] = v.as_slice() else { return false };
    w.entity_mut(e).insert(Sleeping([*x, *y, *z]));
    true
}

/// The sleeper's box changes with the bed (`refresh_box`): queued, so that the removal is over when it runs.
fn box_changed(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    let e = ctx.entity;
    world.commands().queue(move |w: &mut World| crate::cow::refresh_box(w, e));
    crate::bridge::view::touched(world, ctx);
}

/// Un autre dormeur occupe déjà le lit `p`.
pub fn bed_taken(w: &mut World, me: Entity, p: [i32; 3]) -> bool {
    w.query::<(Entity, &Sleeping)>().iter(w).any(|(e, s)| e != me && s.0 == p)
}
/// Pose le corps au-dessus du lit (`setPosToBed`, hauteur 0,5625 ; approximé faute de `pos_to_bed`).
pub fn lie_down(w: &mut World, e: Entity, p: [i32; 3]) {
    if let Some(mut pos) = w.get_mut::<Pos>(e) {
        pos.0 = lie_spot(p);
    }
    w.entity_mut(e).insert(Sleeping(p));
}

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(wake_without_bed.after(crate::cow::reindex));
}

/// `LivingEntity.checkBedExists`: a sleeper whose block is no longer a bed wakes, whatever its brain does.
fn wake_without_bed(level: Res<Level>, mut commands: Commands, sleepers: Query<(Entity, &Sleeping)>) {
    let mw = level.mw.0;
    for (e, s) in &sleepers {
        let [x, y, z] = s.0;
        if !mw.facts.get(mw.world.block(x, y, z)).name.ends_with("_bed") {
            commands.entity(e).remove::<Sleeping>();
        }
    }
}
