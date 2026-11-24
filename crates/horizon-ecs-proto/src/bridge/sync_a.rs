//! The synched entries of the species whose state has a component of P (greffe L0, lot 05): one row per registry id, its slots
//! read those components. A slot gives an entry only when its value differs from the default. The state without a component
//! (guardian target, horse and cat variants...) has no row yet: see the lot's report.
pub use crate::species::sync_a_table::TABLE;
use super::sync::SyncValue;
use super::uuids::EntityUuid;
use crate::behaviours::{beg::Begging, convert_timer::Converting, sit_when_ordered::SitPose, variant::Variant};
use crate::farm::Wool;
use crate::size::Size;
use crate::tame::Owner;
use crate::targeting::Who;
use bevy_ecs::prelude::*;

/// Sheep.DATA_WOOL_ID: the colour in the low nibble, 16 when sheared; sent when not 0.
pub(crate) fn wool(w: &World, e: Entity) -> Option<SyncValue> {
    let wool = w.get::<Wool>(e)?;
    let bits = (wool.color & 15) | if wool.sheared { 16 } else { 0 };
    (bits != 0).then_some(SyncValue::Byte(bits as i8))
}

/// SnowGolem.DATA_PUMPKIN_ID: 16 while the pumpkin is on, the default; sent as 0 once it is cut.
pub(crate) fn pumpkin(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::species::snow_golem::Pumpkin>(e).filter(|p| !p.0).map(|_| SyncValue::Byte(0))
}

/// Bogged.DATA_SHEARED (a bool): sent once the mushrooms are off.
pub(crate) fn sheared(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::dispense::Sheared>(e).map(|_| SyncValue::Bool(true))
}

/// Allay.DATA_DANCING (index 16, a bool): sent while the allay dances.
pub(crate) fn dancing(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::behaviours::event_listen::Dancing>(e).map(|_| SyncValue::Bool(true))
}

/// AbstractCubeMob.ID_SIZE: sent above 1, the default.
pub(crate) fn size(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Size>(e).filter(|s| s.0 > 1).map(|s| SyncValue::Int(s.0))
}

/// Goat.DATA_IS_SCREAMING_GOAT (index 18, default false): sent as true on the screaming goat.
pub(crate) fn screaming_goat(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::species::goat::Screaming>(e).map(|_| SyncValue::Bool(true))
}

/// Goat.DATA_HAS_LEFT_HORN (index 19) and DATA_HAS_RIGHT_HORN (index 20), default true: sent as false once the horn is lost.
pub(crate) fn left_horn(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::behaviours::ram::Shed>(e).filter(|s| s.0 & 1 != 0).map(|_| SyncValue::Bool(false))
}

pub(crate) fn right_horn(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::behaviours::ram::Shed>(e).filter(|s| s.0 & 2 != 0).map(|_| SyncValue::Bool(false))
}

/// Phantom.DATA_ID_SIZE (index 16, default 0): sent when not 0.
pub(crate) fn phantom_size(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Size>(e).filter(|s| s.0 != 0).map(|s| SyncValue::Int(s.0))
}

/// Camel.DATA_LAST_POSE_TICK (index 20, a long, default 0): the game time of the last change of pose, sent when not 0.
pub(crate) fn last_pose(w: &World, e: Entity) -> Option<SyncValue> {
    crate::behaviours::posture::last_pose_tick(w, e).filter(|&t| t != 0).map(SyncValue::Long)
}

/// Fox.DATA_TYPE_ID, Rabbit.DATA_TYPE_ID (the default variant is id 0) and MushroomCow.DATA_TYPE: the variant, sent when not 0.
pub(crate) fn variant(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Variant>(e).filter(|v| v.0 != 0).map(|v| SyncValue::Int(i32::from(v.0)))
}

/// Salmon's variant (index 17, an int): sent when not 1, the default (medium).
pub(crate) fn salmon_type(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Variant>(e).filter(|v| v.0 != 1).map(|v| SyncValue::Int(i32::from(v.0)))
}

/// AbstractVillager.DATA_UNHAPPY_COUNTER: the ticks left of the shake (40 when it starts or restarts, `setUnhappyCounter(40)`), 0 by default.
pub(crate) fn unhappy(w: &World, e: Entity) -> Option<SyncValue> {
    let u = w.get::<crate::behaviours::unhappy::Unhappy>(e)?;
    let left = u.until.saturating_sub(crate::cow::now_of(w, e));
    (left > 0).then_some(SyncValue::Int(left as i32))
}

/// ZombieVillager.DATA_CONVERTING_ID: the cure's shaking, while the conversion timer runs.
pub(crate) fn converting(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Converting>(e).map(|_| SyncValue::Bool(true))
}

/// Wolf.DATA_INTERESTED_ID: BegGoal's flag, the head tilt while it begs.
pub(crate) fn interested(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<Begging>(e).map(|_| SyncValue::Bool(true))
}

/// Wolf.DATA_COLLAR_COLOR (21) and Cat.DATA_COLLAR_COLOR (23), an int: sent once a dye was used (`Collar`).
pub(crate) fn collar(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::dye::Collar>(e).map(|c| SyncValue::Int(i32::from(c.0)))
}

/// TamableAnimal.DATA_FLAGS_ID (index 18 after AgeableMob's 17): sitting 0x01 (the pose, `SitPose`), tame 0x04 (the tame state is `Owner`).
pub(crate) fn tame_flags(w: &World, e: Entity) -> Option<SyncValue> {
    let bits = i8::from(w.get::<SitPose>(e).is_some()) | i8::from(w.get::<Owner>(e).is_some_and(|o| o.0.is_some())) << 2;
    (bits != 0).then_some(SyncValue::Byte(bits))
}

/// TamableAnimal.DATA_OWNERUUID_ID (index 19): the owner's uuid. Only an owner entity's uuid is in P (`EntityUuid`); a player
/// owner's uuid is the host's, so it sends nothing yet (see the lot's report).
pub(crate) fn tame_owner(w: &World, e: Entity) -> Option<SyncValue> {
    let Who::Entity(owner) = w.get::<Owner>(e)?.0? else {
        return None;
    };
    w.get::<EntityUuid>(owner).map(|u| SyncValue::OptUuid(Some(u.0)))
}

/// The species' rows, by registry id: the slots of the components they own (the core's are in `sync_core`).
/// Creaking.IS_TEARING_DOWN (index 18, a bool): sent while its death runs (`tear_down`).
pub(crate) fn tearing_down(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::tear_down::Crumbling>(e).filter(|c| c.sound.is_some()).map(|_| SyncValue::Bool(true))
}

/// Creaking.CAN_MOVE (index 16, a bool, true by default): sent as false while it stands still under a stare (`Stilled`).
pub(crate) fn can_move(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::cow::Stilled>(e).map(|_| SyncValue::Bool(false))
}

/// Creaking.IS_ACTIVE (index 17, a bool): sent while a stare has roused it (`Roused`).
pub(crate) fn is_active(w: &World, e: Entity) -> Option<SyncValue> {
    w.get::<crate::behaviours::stare_freeze::Roused>(e).map(|_| SyncValue::Bool(true))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::sit_when_ordered::SitPose;

    /// `TamableAnimal.DATA_FLAGS_ID`: a tame animal standing sends 0x04, and 0x05 once it is in the sitting pose (not the order).
    #[test]
    fn the_sitting_pose_sets_the_first_flag_bit() {
        let mut w = World::new();
        let e = w.spawn(Owner(Some(Who::Player(1)))).id();
        assert!(matches!(tame_flags(&w, e), Some(SyncValue::Byte(4))));
        w.entity_mut(e).insert(SitPose);
        assert!(matches!(tame_flags(&w, e), Some(SyncValue::Byte(5))));
    }
}

/// The villager types and professions in the order of the built-in registries (the id the entry carries is the holder's index).
const VILLAGER_TYPE_IDS: [&str; 7] = ["desert", "jungle", "plains", "savanna", "snow", "swamp", "taiga"];
const PROFESSION_IDS: [&str; 15] = [
    "none", "armorer", "butcher", "cartographer", "cleric", "farmer", "fisherman", "fletcher", "leatherworker", "librarian", "mason", "nitwit", "shepherd", "toolsmith",
    "weaponsmith",
];

/// Villager.DATA_VILLAGER_DATA (index 19): the type, the profession and the level, in registry ids. None for a profession or a type of
/// a mod (no vanilla id) and for a villager without a `Merchant`; the default (plains, none, level 1) is not sent.
pub(crate) fn villager_data(w: &World, e: Entity) -> Option<SyncValue> {
    let m = w.get::<crate::behaviours::trade_offers::Merchant>(e).filter(|m| !m.wandering)?;
    let by = |list: &[&str], name: &str| list.iter().position(|n| name.strip_prefix("minecraft:") == Some(n)).map(|i| i as i32);
    let kind = by(&VILLAGER_TYPE_IDS, &crate::behaviours::trade_offers::variant_name(m.variant))?;
    let job = if m.profession == crate::behaviours::trade_offers::UNRESOLVED { return None } else { by(&PROFESSION_IDS, &crate::behaviours::trade_offers::profession_name(w, m.profession))? };
    let level = i32::from(m.level);
    (kind != 2 || job != 0 || level != 1).then_some(SyncValue::Ids(vec![kind, job, level]))
}
