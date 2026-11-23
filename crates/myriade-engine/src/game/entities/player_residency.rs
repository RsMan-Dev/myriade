//! Specialized authoritative owners that are not homogeneous entity lists.
use super::{Game, commands::Target};

pub(super) enum SpecializedSource<'a> {
    Player(&'a myriade_engine::motion::PlayerMotion),
    Guest(&'a myriade_engine::lan::guest::Guest),
}
use myriade_engine::join_seq::{
    residency::{
        Journal, Kind, Source,
        table::{Location, SourceEntity},
    },
};

impl SpecializedSource<'_> {
    fn id(&self) -> u32 {
        match self {
            Self::Player(player) => player.entity_id as u32,
            Self::Guest(guest) => guest.entity_id as u32,
        }
    }
}

impl Game<'_> {
    pub(super) fn current_player_resident_location(&self) -> Option<Location> {
        let conn = self.lan_acting?;
        self.command_guest(conn)?.residency.location()
    }

    /// Engine UUID consumers retain a specialized owner's immutable identity
    /// even through a real loan. Public I access applies its policy beforehand.
    pub(super) fn known_resident_entity_uuid_at(
        &self,
        dimension: myriade_types::DimensionKey,
        id: u32,
    ) -> Option<[i32; 4]> {
        let mut table = self.resident_entities.table.borrow_mut();
        let (at, _, details) =
            table.server_admitted_identity(id, |at| self.locate_server_resident_owner(at))?;
        let uuid = details.known_uuid;
        (at.owner.dimension == dimension).then_some(uuid).flatten()
    }

    pub(super) fn resident_target_id(&self, target: Target) -> Option<u32> {
        match target {
            Target::Player => {
                let conn = self.lan_acting?;
                self.command_guest(conn)?
                    .residency
                    .identity()
                    .map(|identity| identity.id.get())
            }
            Target::Guest(conn) => self
                .command_guest(conn)?
                .residency
                .identity()
                .map(|identity| identity.id.get()),
            _ => super::native_entities::target_id(target),
        }
    }

    pub(super) fn bind_specialized_resident_owners(&mut self, journal: &Journal) {
        self.resident_entities.local_player.rebind(journal);
        let body_id = self.player.as_ref().map(|player| player.entity_id as u32);
        if self.lan_acting.is_some() {
            self.resident_entities.local_player.extract();
        }
        if let Some(host) = &mut self.lan {
            host.hub
                .guests
                .bind_residency(journal, |id| super::Dim::from_id(id).key());
            for &conn in &self.lan_guest_swaps {
                if let Some(index) = host.hub.guests.connection_index(conn) {
                    let guest = &host.hub.guests[index];
                    if self.lan_acting != Some(conn) || body_id != Some(guest.entity_id as u32) {
                        guest.residency.extract();
                    }
                }
            }
        }
    }

    pub(super) fn specialized_resident_source(
        &self,
        at: Location,
    ) -> Option<SourceEntity<(SpecializedSource<'_>, Target)>> {
        let (source, id, target) = match (at.owner.kind, at.source) {
            (Kind::Player, Source::Guest(conn)) => {
                let host = self.lan.as_ref()?;
                let guest = &host.hub.guests[host.hub.guests.connection_index(conn)?];
                if guest.residency.location() != Some(at) || guest.released {
                    return None;
                }
                if self.lan_acting == Some(conn) {
                    let player = self.player.as_ref()?;
                    if player.entity_id != guest.entity_id {
                        return None;
                    }
                    (
                        SpecializedSource::Player(player),
                        guest.entity_id as u32,
                        Target::Player,
                    )
                } else {
                    if self.lan_guest_swaps.contains(&conn) {
                        return None;
                    }
                    (
                        SpecializedSource::Guest(guest),
                        guest.entity_id as u32,
                        Target::Guest(conn),
                    )
                }
            }
            _ => return None,
        };
        Some(SourceEntity {
            id,
            value: (source, target),
        })
    }

    pub(super) fn with_specialized_resident_slot<R>(
        &mut self,
        at: Location,
        visit: impl FnOnce(&mut horizon_mod_runtime::attachments::AttachmentLifetime) -> R,
    ) -> super::entity_residency::SlotAccess<R> {
        use super::entity_residency::SlotAccess;
        use horizon_mod_runtime::attachments::HasAttachmentLifetime;
        match at.source {
            Source::LocalPlayer => {
                SlotAccess::Done(visit(self.player_state.attachments.attachment_lifetime()))
            }
            Source::Guest(conn) => {
                if self.lan_acting == Some(conn) {
                    return SlotAccess::Done(visit(
                        self.player_state.attachments.attachment_lifetime(),
                    ));
                }
                let Some(attachments) = self
                    .lan
                    .as_mut()
                    .and_then(|host| host.hub.guests.attachments_mut(conn))
                else {
                    return SlotAccess::Missing;
                };
                SlotAccess::Done(visit(attachments.attachment_lifetime()))
            }
            Source::DragonPart(_) | Source::Ordinary => SlotAccess::Missing,
        }
    }

    pub(super) fn locate_specialized_resident_owner(
        &self,
        at: Location,
    ) -> Option<SourceEntity<(Target, bool)>> {
        let source = self.specialized_resident_source(at)?;
        let (_, target) = source.value;
        Some(SourceEntity {
            id: source.value.0.id(),
            value: (
                target,
                self.resident_source_target_visible(at.owner.dimension, target),
            ),
        })
    }
}
