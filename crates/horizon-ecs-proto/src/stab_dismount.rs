//! `LivingEntity.stabAttack` with the `dismount` condition of a kinetic weapon: the target `stopRiding()`. A creature is found by its `Riding`
//! component; a player by the ridden mounts (only those are visited, and only when a stab passes the condition).
use crate::mount::{self, Mount, Ridden, Riding};
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// The mount `who` rides, when it rides one.
pub fn mount_of(w: &mut World, who: Who) -> Option<Entity> {
    if let Who::Entity(e) = who {
        return w.get::<Riding>(e).map(|r| r.0);
    }
    let mut q = w.query_filtered::<(Entity, &Mount), With<Ridden>>();
    q.iter(w).find(|(_, m)| m.rider == Some(who) || m.passenger == Some(who)).map(|(e, _)| e)
}

/// `Entity.stopRiding`: `who` gets off its mount (a rider of the first seat through `dismount`, the passenger of the second through
/// `dismount_passenger`). True when it rode one.
pub fn stop_riding(w: &mut World, who: Who) -> bool {
    // `stabAttack`: a rider of a type in `cannot_be_dismounted_by_item_usage` stays.
    if let Who::Entity(e) = who {
        if crate::type_tags::named(w, "cannot_be_dismounted_by_item_usage", e) {
            return false;
        }
    }
    let Some(m) = mount_of(w, who) else { return false };
    if w.get::<Mount>(m).is_some_and(|x| x.rider == Some(who)) {
        mount::dismount(w, m);
    } else {
        mount::dismount_passenger(w, m);
    }
    true
}
