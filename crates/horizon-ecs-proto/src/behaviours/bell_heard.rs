//! Une cloche sonnée : chaque entité avec un cerveau dans 32 blocs garde l'heure de la sonnerie (`HEARD_BELL`).
//! Aucune file : `ring` fait le travail à l'appel (par la boucle de blocs ou par `ring_bell`).
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::{now_of, Index};
use crate::rules::bell_heard::{centre, RANGE};

pub static HEARD_BELL: MemKey = MemKey::new("minecraft:heard_bell_time", Shape::Int);

/// La cloche du bloc `pos` vient d'être sonnée : seules les entités proches écrivent l'heure.
pub fn ring(w: &mut World, pos: [i32; 3]) {
    let mut near = Vec::new();
    let (x, z) = centre(pos);
    w.resource::<Index>().for_radius(x, z, RANGE, |o| near.push(o));
    for o in near {
        if w.get::<Brain>(o).is_some() {
            let now = now_of(w, o) as i64;
            set(w, o, &HEARD_BELL, MemValue::Int(now), None);
        }
    }
}
