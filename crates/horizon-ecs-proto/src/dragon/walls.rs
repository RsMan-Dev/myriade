//! `EnderDragon.checkWalls` (`D/walls.rs`) over the head, neck and body: a block that is neither air nor `#dragon_transparent` breaks
//! when the dragon may grief and may destroy it and it is not `#dragon_immune`; any other leaves the dragon in a wall (`in_wall`,
//! which slows its next move). One scan of three boxes per wake, run after the contacts.
use super::fight::is_air;
use super::flight::Flight;
use super::parts::{part_box, Parts};
use crate::blocks::set_state;
use crate::goals::Events;
use crate::level::{can_entity_destroy, can_grief, has_block_tag};
use bevy_ecs::prelude::*;

pub fn check(w: &mut World, e: Entity) {
    let Some(parts) = w.get::<Parts>(e) else { return };
    let boxes = [part_box(parts, 0), part_box(parts, 1), part_box(parts, 2)];
    let griefing = can_grief(w, e);
    let (mut in_wall, mut removed) = (false, Vec::new());
    for b in boxes {
        let (lo, hi) = (b.min_position(), b.max_position());
        for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
            for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
                for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                    if is_air(w, x, y, z) || has_block_tag(w, "minecraft:dragon_transparent", [x, y, z]) {
                        continue;
                    }
                    let immune = has_block_tag(w, "minecraft:dragon_immune", [x, y, z]);
                    if griefing && can_entity_destroy(w, e, [x, y, z], !immune) {
                        removed.push(([x, y, z], immune));
                    } else {
                        in_wall = true;
                    }
                }
            }
        }
    }
    removed.retain(|&(_, immune)| {
        in_wall |= immune;
        !immune
    });
    if !removed.is_empty() {
        for (p, _) in &removed {
            set_state(w, *p, "minecraft:air", &[]);
        }
        let head = w.get::<Parts>(e).map_or([0.0; 3], |p| p.0[0].pos);
        w.resource_mut::<Events>().host.particles("explosion", head, [0.0; 3], 0.0, 1);
    }
    if let Some(mut f) = w.get_mut::<Flight>(e) {
        f.in_wall = in_wall;
    }
}
