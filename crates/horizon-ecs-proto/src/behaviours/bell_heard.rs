//! Une cloche sonnée : chaque entité avec un cerveau dans 32 blocs garde l'heure de la sonnerie (`HEARD_BELL`).
//! Système événementiel : ne tourne que si `BellRings` n'est pas vide ; l'événement vient du bloc cloche (à câbler).
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::{now_of, Index};

pub static HEARD_BELL: MemKey = MemKey::new("minecraft:heard_bell_time", Shape::Int);

/// Les cloches sonnées depuis le dernier tick (remplies par la boucle de blocs, vidées ici).
#[derive(Default)]
pub struct BellRings(pub Vec<[i32; 3]>);

const RANGE: f64 = 32.0;

/// La cloche du bloc `pos` vient d'être sonnée : le bloc (boucle de blocs) l'annonce ici.
pub fn ring(w: &mut World, pos: [i32; 3]) {
    w.resource_mut::<BellRings>().0.push(pos);
}

/// Système : ne tourne que si une cloche a sonné ce tick ; touche seulement les entités proches de chaque sonnerie.
pub fn hear_bells(w: &mut World) {
    if w.resource::<BellRings>().0.is_empty() {
        return;
    }
    let rings = std::mem::take(&mut w.resource_mut::<BellRings>().0);
    for ring in rings {
        let mut near = Vec::new();
        w.resource::<Index>().for_radius(f64::from(ring[0]) + 0.5, f64::from(ring[2]) + 0.5, RANGE, |o| near.push(o));
        for o in near {
            if w.get::<Brain>(o).is_some() {
                let now = now_of(w, o) as i64;
                set(w, o, &HEARD_BELL, MemValue::Int(now), None);
            }
        }
    }
}
