//! One storage-family dispatch for the server's residency journal.
use super::{Game, commands::Target};
use myriade_engine::join_seq::{
    Joined,
    residency::{
        Journal, Kind, Owner,
        table::{Location, SourceEntity},
    },
};
use myriade_types::DimensionKey;

/// Assign a stored zero-id entity through its residency boundary exactly once.
macro_rules! assign_resident_id {
    ($list:expr, $index:expr, $entity:ident, $next:expr) => {{
        let index = $index;
        if $list[index].id == 0 {
            let id = $list.update(index, |$entity| {
                $crate::game::entity::assign_stored_id($next, &mut $entity.id)
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
    ($list:expr, $entity:ident, $next:expr) => {
        for index in 0..$list.len() {
            $crate::game::entity_residency::assign_resident_id!(
                $list, index, $entity, $next
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
        }
    };
}
pub(crate) use resident_owners;

macro_rules! resident_visible {
    ($kind:ident, $entity:expr) => {
        true
    };
}
pub(crate) use resident_visible;

macro_rules! bind_resident_list {
    ($kind:ident, $list:expr, $journal:expr, $owner:expr) => {
        $list.bind_residency($journal, $owner, |entity| (entity.id, entity.join_seq()))
    };
}
pub(crate) use bind_resident_list;

macro_rules! define_resident_access {
    (($this:ident) $($kind:ident => $target:ident: $($field:ident).+,)*) => {
        pub(super) fn bind_resident_owners(&mut $this, journal: &Journal, dimension: DimensionKey, track_saved_uuids: bool) {
            if track_saved_uuids {
                $this.persist.uuids.bind(journal, dimension);
            }
            $($crate::game::entity_residency::bind_resident_list!($kind, $this$(.$field)+, journal, Owner { dimension, kind: Kind::$kind });)*
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
                Kind::Mob | Kind::Player | Kind::Item | Kind::Orb | Kind::Hook | Kind::Projectile | Kind::Other | Kind::General | Kind::Cushion | Kind::Spawner | Kind::Stand | Kind::Dragon | Kind::DragonPart => None,
            };
            source
        }
        pub(super) fn locate_resident_owner(&$this, at: Location) -> Option<SourceEntity<(Target, bool)>> {
            let source = $this.locate_resident_attachment_owner(at);
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
