//! Object-owned passenger memory and typed transfer loading.
use super::*;

impl Game<'_> {
    /// Whether `id` is a boat in the ECS.
    pub(in crate::game) fn is_boat(&self, id: u32) -> bool {
        self.ecs.boat_status(id).is_some()
    }

    /// Loads the saved projectile `tag` in the ECS level and gives it its id, which keeps its portal cooldown, its transferred state
    /// and its uuid; None when the level refuses the saved form.
    fn load_projectile_ecs(
        &mut self,
        tag: &Tag,
        uuid: Option<[i32; 4]>,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
    ) -> Option<u32> {
        let Tag::Compound(c) = tag else { return None };
        let e = self.ecs_with(|ecs, mw| ecs.load_entity(mw, c))?;
        let id = self.ecs_id_of(e);
        self.restore_portal_cooldown(id, tag);
        if let Some(state) = transferred {
            self.adopt_transferred_entity_state(id, state);
        }
        if let Some(own) = uuid {
            self.remember_uuid(id, Some(own), tag);
        }
        Some(id)
    }

    pub(in crate::game) fn load_arrow_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        _player_uuid: Option<[i32; 4]>,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
    ) {
        if let Some(id) = self.load_projectile_ecs(&tag, uuid, transferred) {
            self.remember_tags(id, &tag);
        }
    }

    pub(in crate::game) fn load_spit_or_bullet_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        _player_uuid: Option<[i32; 4]>,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
    ) {
        self.load_projectile_ecs(&tag, uuid, transferred);
    }

    pub(in crate::game) fn load_fireball_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        _player_uuid: Option<[i32; 4]>,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
    ) {
        self.load_projectile_ecs(&tag, uuid, transferred);
    }

    pub(in crate::game) fn load_throwable_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        _player_uuid: Option<[i32; 4]>,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
    ) {
        self.load_projectile_ecs(&tag, uuid, transferred);
    }

    pub(in crate::game) fn load_falling_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
    ) {
        let Tag::Compound(c) = &tag else { return };
        let Some(id) = self.ecs_load(c) else { return };
        if let Some(state) = transferred {
            self.adopt_transferred_entity_state(id, state);
        }
        self.remember(id, uuid, tag);
    }

    pub(in crate::game) fn load_trident_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
    ) {
        let Tag::Compound(c) = &tag else { return };
        let Some(id) = self.ecs_load(c) else { return };
        if let Some(state) = transferred {
            self.adopt_transferred_entity_state(id, state);
        }
        self.remember(id, uuid, tag);
    }

    pub(in crate::game) fn load_rocket_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
    ) {
        let Tag::Compound(c) = &tag else { return };
        let Some(id) = self.ecs_load(c) else { return };
        if let Some(state) = transferred {
            self.adopt_transferred_entity_state(id, state);
        }
        self.remember(id, uuid, tag);
    }

    pub(in crate::game) fn load_tnt_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
    ) {
        self.load_tnt_with_portal(tag, uuid, transferred, false);
    }

    pub(in crate::game) fn load_tnt_with_portal(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
        used_portal: bool,
    ) {
        if uuid.is_some_and(|u| self.target_with_uuid(u).is_some()) {
            return;
        }
        let mut tag = tag;
        let riders = passenger_graph::take_passengers(&mut tag);
        if entity_nbt::position(&tag).is_none() {
            return;
        }
        let Tag::Compound(c) = &tag else { return };
        let Some(id) = self.ecs_load(c) else { return };
        self.ecs_set_tnt_used_portal(id, used_portal);
        // `Entity.load`: the `PortalCooldown` of one that just came through.
        if let Some(state) = transferred {
            self.adopt_transferred_entity_state(id, state);
        }
        self.remember(id, uuid, tag);
        if let (Some(u), Some(e)) = (uuid, self.ecs.entity_of(id)) {
            self.ecs.set_uuid(e, u);
        }
        if let Some(riders) = riders {
            self.load_passengers(id, riders);
        }
    }

    pub(in crate::game) fn load_boat_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        k: &str,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
    ) -> Option<u32> {
        // A runtime transfer owns a distinct live object: like a Mob, the copy
        // refused residency stays below the passengers admitted before it.
        // A disk copy with a taken UUID is still not constructed.
        if transferred.is_none() && uuid.is_some_and(|u| self.target_with_uuid(u).is_some()) {
            return None;
        }
        let refused = transferred.is_some() && self.uuid_admission_refused(uuid);
        // `PersistentEntitySectionManager.addEntity` pops a refused copy: it and its riders never reach the level.
        // `k` names the boat's wood for a host copy only, which is no longer kept.
        let _ = k;
        if refused {
            return None;
        }
        if entity_nbt::boat_from_nbt(&tag).is_some() {
            let mut tag = tag;
            let riders = passenger_graph::take_passengers(&mut tag);
            let Tag::Compound(c) = &tag else {
                return None;
            };
            let id = self.ecs_load(c)?;
            if let Some(state) = transferred {
                self.adopt_transferred_entity_state(id, state);
            }
            self.remember(id, uuid, tag);
            if let Some(riders) = riders {
                self.load_passengers(id, riders);
            }
            return Some(id);
        }
        None
    }

    /// `PersistentEntitySectionManager.addEntityUuid`: another resident object
    /// already holds `uuid`, so this one is refused level residency.
    pub(in crate::game) fn uuid_admission_refused(&self, uuid: Option<[i32; 4]>) -> bool {
        uuid.is_some_and(|uuid| {
            self.target_with_uuid(uuid)
                .is_some_and(|target| target != crate::game::commands::Target::Player)
        })
    }

    /// The saved mob `tag` loaded into the ECS, which keeps its riders; a transferred copy keeps its state.
    /// Nothing is kept off the level: a refused copy is not held anywhere (`Option A`).
    pub(in crate::game) fn load_mob_transferred(
        &mut self,
        tag: Tag,
        mount: Option<u32>,
        transferred: Option<crate::game::entity_initialization::TransferredEntityState>,
    ) -> Option<u32> {
        // RESTE-SANS-ECS: the mount is the ECS's `Passengers` tree, not a host `Mob.extra.mount`; the mob's
        // anger target, dying body (`died`) and restored attachments are not re-applied at load.
        let _ = mount;
        let Tag::Compound(c) = &tag else {
            return None;
        };
        let uuid = entity_nbt::uuid(&tag);
        let id = self.ecs_load(c)?;
        if let Some(state) = transferred {
            self.adopt_transferred_entity_state(id, state);
        }
        self.remember(id, uuid, tag);
        Some(id)
    }

    /// `Entity.shouldBeSaved`: the mobs riding entity `id` under `tag`'s
    /// `Passengers`, after the passengers it kept from loading. Discarded
    /// passengers cannot save even before their list entry is swept.
    pub(super) fn mob_passenger_objects(&self, id: u32) -> Vec<u32> {
        self.ecs_rider_of(id).into_iter().collect()
    }

    pub(in crate::game) fn mob_passenger_ids(&self, root: u32) -> Vec<u32> {
        self.mob_passenger_objects(root)
    }

    /// `Entity.teleportCrossDimension`: pair each serialized node with that
    /// exact object's runtime ownership in one walk, never by UUID or Vec order.
    pub(in crate::game) fn take_vehicle_tree_transfer(
        &mut self,
        root: u32,
    ) -> Option<(Tag, TransferredVehicleTree)> {
        let (kind, mut tag) = if self.is_boat(root) {
            let mut tag = Tag::Compound(self.ecs_mob_tree(root)?);
            self.save_portal_cooldown(root, &mut tag);
            (VehicleTreeKind::Boat, tag)
        } else if let Some(c) = self.ecs.is_active().then(|| self.ecs_mob_tree(root)).flatten() {
            let mut tag = Tag::Compound(c);
            self.save_portal_cooldown(root, &mut tag);
            (VehicleTreeKind::Ecs, tag)
        } else {
            let target = self.level_entity(root as i32)?;
            (VehicleTreeKind::Other, self.command_target_nbt(target)?)
        };
        let name = entity_nbt::kind(&tag)?;
        passenger_graph::take_passengers(&mut tag);
        write_attachments(&mut tag, root, &self.entity_attachments);
        let mut tags = Vec::new();
        let mut passengers = Vec::new();
        for slot in self.ordered_passenger_slots(root) {
            match slot {
                PassengerSlot::Live(id) => {
                    let (tag, state) = self.take_vehicle_tree_transfer(id)?;
                    tags.push(tag);
                    passengers.push(Some(state));
                }
                PassengerSlot::Saved(tag) => {
                    tags.push(tag);
                    passengers.push(None);
                }
            }
        }
        add_passengers(&mut tag, tags);
        Some((
            tag,
            TransferredVehicleTree {
                source_id: root,
                kind,
                name,
                state: self.take_transferred_entity_state(root),
                passengers,
            },
        ))
    }

    pub(super) fn prepare_vehicle_transfer(
        &mut self,
        mut tag: Tag,
        tree: TransferredVehicleTree,
    ) -> Option<PreparedVehicleTransfer> {
        if entity_nbt::kind(&tag).as_deref() != Some(tree.name.as_str()) {
            log::warn!(
                "Horizon: transferred vehicle type does not match its captured object state"
            );
            return None;
        }
        let tags = match passenger_graph::take_passengers(&mut tag) {
            Some(Tag::List(tags)) => tags,
            _ => Vec::new(),
        };
        if tags.len() != tree.passengers.len() {
            log::warn!(
                "Horizon: transferred vehicle topology does not match its captured object state"
            );
            return None;
        }
        let passengers = tags
            .into_iter()
            .zip(tree.passengers)
            .map(|(tag, child)| match child {
                Some(child) => self
                    .prepare_vehicle_transfer(tag, child)
                    .map(PreparedVehiclePassenger::Object),
                None => Some(PreparedVehiclePassenger::Saved(tag)),
            })
            .collect::<Option<Vec<_>>>()?;
        Some(PreparedVehicleTransfer {
            source_id: tree.source_id,
            tag,
            kind: tree.kind,
            name: tree.name,
            state: tree.state,
            passengers,
        })
    }

    /// Reconstruct using actual object IDs, even when the copied root is refused
    /// residency. `Entity.teleportCrossDimension` admits every passenger before
    /// its copied vehicle, whether that root is a mob, boat or minecart.
    pub(in crate::game) fn restore_vehicle_tree_state(
        &mut self,
        tag: Tag,
        tree: TransferredVehicleTree,
    ) -> Option<RestoredVehicleTree> {
        let prepared = self.prepare_vehicle_transfer(tag, tree)?;
        let mut by_source = HashMap::new();
        let root = self.restore_vehicle_transfer(prepared, &mut by_source)?;
        Some(RestoredVehicleTree { root, by_source })
    }

    pub(super) fn restore_vehicle_transfer(
        &mut self,
        tree: PreparedVehicleTransfer,
        by_source: &mut HashMap<u32, u32>,
    ) -> Option<u32> {
        let PreparedVehicleTransfer {
            source_id,
            tag,
            kind,
            name,
            state,
            passengers,
        } = tree;
        let riders = passengers
            .into_iter()
            .map(|child| match child {
                PreparedVehiclePassenger::Object(child) => self
                    .restore_vehicle_transfer(child, by_source)
                    .map(PassengerSlot::Live),
                PreparedVehiclePassenger::Saved(tag) => Some(PassengerSlot::Saved(tag)),
            })
            .collect::<Option<Vec<_>>>()?;
        let root = self.restore_vehicle_object(tag, kind, &name, state)?;
        for slot in riders {
            match slot {
                PassengerSlot::Live(id) => self.link_loaded_passenger(id, root),
                // A slot with no source object still takes ordinary admission in
                // the destination, as main's NBT reconstruction did.
                PassengerSlot::Saved(tag) => self.load_passengers(root, Tag::List(vec![tag])),
            }
        }
        by_source.insert(source_id, root);
        Some(root)
    }

    /// The arrival of an ECS entity that crossed a portal: loaded into the ECS level, its riders after it.
    fn load_ecs_transferred(
        &mut self,
        tag: Tag,
        uuid: Option<[i32; 4]>,
        state: crate::game::entity_initialization::TransferredEntityState,
    ) -> Option<u32> {
        let mut tag = tag;
        let riders = passenger_graph::take_passengers(&mut tag);
        let Tag::Compound(c) = &tag else {
            return None;
        };
        let id = self.ecs_load(c)?;
        self.adopt_transferred_entity_state(id, state);
        self.remember(id, uuid, tag);
        if let Some(riders) = riders {
            self.load_passengers(id, riders);
        }
        Some(id)
    }

    pub(super) fn restore_vehicle_object(
        &mut self,
        tag: Tag,
        kind: VehicleTreeKind,
        name: &str,
        state: crate::game::entity_initialization::TransferredEntityState,
    ) -> Option<u32> {
        let uuid = entity_nbt::uuid(&tag);
        match kind {
            VehicleTreeKind::Mob => self.load_mob_transferred(tag, None, Some(state)),
            VehicleTreeKind::Boat => self.load_boat_transferred(tag, uuid, name, Some(state)),
            VehicleTreeKind::Ecs => self.load_ecs_transferred(tag, uuid, state),
            VehicleTreeKind::Other => {
                let before = self.next_entity;
                self.load_listed_transferred(name, uuid, self.level.player_uuid, tag, state)
                    .ok()?;
                ((before + 1)..=self.next_entity).find(|id| self.passenger_object_present(*id))
            }
        }
    }

    /// The saved tree of the ECS mob `id` with its riders, as a vehicle's tree is written; None when the ECS does not hold it.
    pub(super) fn mob_object_tree_nbt(&mut self, id: u32) -> Option<Tag> {
        let mut tag = Tag::Compound(self.ecs_mob_tree(id)?);
        self.put_mob_passengers(&mut tag, id);
        Some(tag)
    }

}
