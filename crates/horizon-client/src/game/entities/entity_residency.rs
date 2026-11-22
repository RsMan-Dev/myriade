//! One storage-family dispatch for the server's residency journal.
use super::{Game, commands::Target};
use horizon_client::join_seq::{
    Joined,
    residency::{
        Journal, Kind, Owner,
        table::{Location, SourceEntity},
    },
};
use mod_api::DimensionKey;

/// Assign a stored zero-id entity through its residency boundary exactly once.
macro_rules! assign_resident_id {
    ($list:expr, $index:expr, $entity:ident, $next:expr, $lifetime:expr) => {{
        let index = $index;
        if $list[index].id == 0 {
            let id = $list.update(index, |$entity| {
                $crate::game::entity::assign_stored_id($next, &mut $entity.id, $lifetime)
            });
            $list.id_assigned(index);
            id
        } else {
            $list[index].id
        }
    }};
}
pub(crate) use assign_resident_id;

macro_rules! assign_resident_ids {
    ($list:expr, $entity:ident, $next:expr, $lifetime:expr) => {
        for index in 0..$list.len() {
            $crate::game::entity_residency::assign_resident_id!(
                $list, index, $entity, $next, $lifetime
            );
        }
    };
}
pub(crate) use assign_resident_ids;

// Keep binding and resolution on this one family list.
macro_rules! resident_owners {
    ($then:ident, $this:ident) => {
        $then! {
            ($this)
            Mob => Mob: remote_entities,
        }
    };
}
pub(crate) use resident_owners;

macro_rules! resident_visible {
    (Mob, $entity:expr) => {
        !$entity.removed
    };
    ($kind:ident, $entity:expr) => {
        true
    };
}
pub(crate) use resident_visible;

macro_rules! bind_resident_list {
    (Mob, $list:expr, $journal:expr, $owner:expr) => {
        $list.bind_residency_with_subtype(
            $journal,
            $owner,
            |entity| (entity.id, entity.join_seq()),
            |entity| {
                if matches!(
                    entity.species(),
                    horizon_client::mobs::Species::Villager
                        | horizon_client::mobs::Species::WanderingTrader
                ) {
                    horizon_client::join_seq::residency::EntitySubtype::Merchant
                } else {
                    horizon_client::join_seq::residency::EntitySubtype::Ordinary
                }
            },
        )
    };
    ($kind:ident, $list:expr, $journal:expr, $owner:expr) => {
        $list.bind_residency($journal, $owner, |entity| (entity.id, entity.join_seq()))
    };
}
pub(crate) use bind_resident_list;

/// Physical lifetime access, not a cached publication or a delivery promise.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SlotAccess<R> {
    Done(R),
    Taken,
    Missing,
    UnsupportedKind(Kind),
}

macro_rules! resident_lifetime_slot {
    ($kind:ident, $list:expr, $join:expr, $visit:expr) => {{
        let Some(index) = $list.locate($join) else {
            return SlotAccess::Missing;
        };
        SlotAccess::Done($list.update(index, |entity| {
            $visit(
                horizon_mod_runtime::attachments::HasAttachmentLifetime::attachment_lifetime(
                    entity,
                ),
            )
        }))
    }};
}
pub(crate) use resident_lifetime_slot;

macro_rules! define_resident_access {
    (($this:ident) $($kind:ident => $target:ident: $($field:ident).+,)*) => {
        pub(super) fn bind_resident_owners(&mut $this, journal: &Journal, dimension: DimensionKey, track_saved_uuids: bool) {
            if track_saved_uuids {
                $this.persist.uuids.bind(journal, dimension);
            }
            $($crate::game::entity_residency::bind_resident_list!($kind, $this$(.$field)+, journal, Owner { dimension, kind: Kind::$kind });)*
            $this.dragon.bind_residency_specialized(journal, Owner { dimension, kind: Kind::Dragon }, |entity| (entity.id, entity.join_seq()), horizon_client::join_seq::residency::Source::Ordinary, Some((Kind::DragonPart, 8, |dragon, offset| dragon.parts.get(usize::from(offset)).map(|part| part.3))), None);
        }
        pub(super) fn with_resident_owner_slot<R>(
            &mut $this,
            at: Location,
            visit: impl FnOnce(&mut horizon_mod_runtime::attachments::AttachmentLifetime) -> R,
        ) -> $crate::game::entity_residency::SlotAccess<R> {
            use $crate::game::entity_residency::SlotAccess;
            match at.owner.kind {
                $(Kind::$kind => $crate::game::entity_residency::resident_lifetime_slot!($kind, $this$(.$field)+, at.join, visit),)*
                Kind::Dragon => {
                    $this.dragon.locate(at.join).map_or(SlotAccess::Missing, |_| {
                        use horizon_mod_runtime::attachments::HasAttachmentLifetime;
                        SlotAccess::Done(visit($this.dragon.as_mut().unwrap().attachment_lifetime()))
                    })
                },
                Kind::DragonPart => {
                    use horizon_client::join_seq::residency::Source;
                    use horizon_mod_runtime::attachments::HasAttachmentLifetime;
                    let Source::DragonPart(offset) = at.source else { return SlotAccess::Missing; };
                    if $this.dragon.locate(at.join).is_none() { return SlotAccess::Missing; }
                    let Some(part) = $this.dragon.as_mut().and_then(|dragon| dragon.parts.get_mut(usize::from(offset))) else { return SlotAccess::Missing; };
                    SlotAccess::Done(visit(part.attachment_lifetime()))
                },
                Kind::Player | Kind::Item | Kind::Orb | Kind::Hook | Kind::Projectile | Kind::Other | Kind::General | Kind::Fangs | Kind::Knot | Kind::Crystal | Kind::Cloud | Kind::Cushion | Kind::Spawner | Kind::Stand => SlotAccess::UnsupportedKind(at.owner.kind),
            }
        }
        pub(super) fn locate_resident_attachment_owner(&$this, at: Location) -> Option<SourceEntity<(Target, bool)>> {
            let source = match at.owner.kind {
                $(Kind::$kind => {
                    let index = $this$(.$field)+.locate(at.join)?;
                    let entity = &$this$(.$field)+[index];
                    let visible = $crate::game::entity_residency::resident_visible!($kind, entity);
                    let id = entity.id;
                    Some(SourceEntity { id, value: (Target::$target(id), visible) })
                })*
                Kind::Fangs | Kind::Knot | Kind::Crystal | Kind::Cloud => None,
                Kind::Dragon => {
                    $this.dragon.locate(at.join)?;
                    let id = $this.dragon.as_ref()?.id;
                    Some(SourceEntity { id, value: (Target::Dragon(id), true) })
                }
                Kind::DragonPart => {
                    let horizon_client::join_seq::residency::Source::DragonPart(offset) = at.source else { return None; };
                    $this.dragon.locate(at.join)?;
                    let dragon = $this.dragon.as_ref()?;
                    if dragon.id == 0 { return None; }
                    dragon.parts.get(usize::from(offset))?;
                    let id = dragon.id.checked_add(u32::from(offset) + 1)?;
                    let visible = true;
                    Some(SourceEntity { id, value: (Target::DragonPart(id), visible) })
                }
                Kind::Player | Kind::Item | Kind::Orb | Kind::Hook | Kind::Projectile | Kind::Other | Kind::General | Kind::Cushion | Kind::Spawner | Kind::Stand => None,
            };
            source
        }
        pub(super) fn locate_resident_owner(&$this, at: Location) -> Option<SourceEntity<(Target, bool)>> {
            let source = $this.locate_resident_attachment_owner(at).map(|mut source| {
                if let Target::DragonPart(_) = source.value.0 {
                    source.value.1 &= $this.dragon.as_ref().is_some_and(|dragon| !$this.persist.removals.contains(&Target::Dragon(dragon.id)));
                }
                source
            });
            // Logical visibility is not physical absence: filtering here would
            // incorrectly evict reversible conversion rows or report Taken.
            source.map(|mut source| {
                source.value.1 &= !$this.persist.removals.contains(&source.value.0);
                source.value.1 &= !$this.persist.destroying_vehicles.contains(&source.value.0);
                source
            })
        }
    };
}
pub(crate) use define_resident_access;

impl Game<'_> {
    resident_owners!(define_resident_access, self);
}

#[cfg(test)]
#[path = "entity_residency/lifetime_tests.rs"]
mod lifetime_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_client::join_seq::residency::table::EntityLookup;
    use horizon_core::position::BlockPos;


}
