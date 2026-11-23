//! `EntityReference`s a mob's compound holds: a UUID, resolved against what is in
//! the level.
use super::Who;
use crate::mobs::Mob;
use std::collections::HashMap;

/// The entity a stored UUID names, while it is in the level: `player` (the
/// player's own UUID, when one is in the level), or the mob `uuids` gives that
/// UUID to. `uuids` is the game layer's id -> UUID map, so this is the read half
/// of the pair [`who_uuid`] and the save's `uuid_of` form. A UUID nothing
/// loaded has, or one naming a kind `Who` cannot carry (an item, a projectile, a
/// guest), is `None` - never some other entity as a stand-in.
pub fn who_by_uuid(
    uuids: &HashMap<u32, [i32; 4]>,
    mobs: &[Mob],
    player: Option<[i32; 4]>,
    uuid: [i32; 4],
) -> Option<Who> {
    if player == Some(uuid) {
        return Some(Who::Player);
    }
    // Only a loaded mob: the map also holds projectiles, vehicles and items,
    // whose ids mean nothing to `Who`.
    uuids
        .iter()
        .find(|(id, u)| **u == uuid && mobs.iter().any(|m| m.id == **id))
        .map(|(id, _)| Who::Mob(*id))
}

#[cfg(test)]
mod tests {
    /// `LivingEntity.readAdditionalSaveData`: `lastHurtByMob` is the UUID,
    /// `lastHurtByMobTimestamp = tickCount - ticks_since_last_hurt_by_mob`, and a
    /// freshly loaded entity has `tickCount` 0, so 30 saved ticks make -30.
    #[test]
    fn a_saved_hurt_by_mob_is_read_back_as_a_reference_and_a_timestamp() {
        todo!("phase 2 : réécrire sur l'ECS")
    }

    /// A save with no hurt-by key has no reference and a timestamp stamped at
    /// `tickCount`; a key that is not a UUID int array is no reference.
    #[test]
    fn a_save_without_a_hurt_by_mob_has_none() {
        todo!("phase 2 : réécrire sur l'ECS")
    }
}
