//! Save visibility while authoritative rows are lent out.
use super::*;

impl Game<'_> {
    /// Spawn the entities of chunks that finished loading.
    pub(in crate::game) fn spawn_loaded_entities(&mut self) {
        let _save_lease = myriade_engine::join_seq::visibility::PassLease::enter();
        let loaded = std::mem::take(&mut self.world.loaded_entities);
        for (key, tags) in loaded {
            if self.ecs.is_active() {
                let mut entities = Vec::new();
                for pending in tags {
                    let tag = pending.into_parts().0;
                    if entity_nbt::position(&tag).is_none() {
                        // No position: the ECS reads nothing from it, so it is kept byte for byte as the legacy path keeps it.
                        self.persist.unknown.entry(key).or_default().push(tag);
                    } else if let Tag::Compound(c) = tag {
                        entities.push(c);
                    }
                }
                self.ecs_load_chunk(key.0, key.1, &entities);
            } else {
                for pending in tags {
                    let (tag, reason) = pending.into_parts();
                    self.spawn_loaded_entity_reason(key, tag, false, reason);
                }
            }
        }
    }

    /// Install one fixture record or explicitly owned live transfer through the same
    /// metadata and join boundary. The transfer token belongs only to this call.
    #[cfg(test)]
    pub(in crate::game) fn spawn_loaded_entity(
        &mut self,
        key: ChunkKey,
        tag: Tag,
        transferred: bool,
    ) {
        self.spawn_loaded_entity_reason(
            key,
            tag,
            transferred,
            myriade_engine::world::EntityLoadReason::Ordinary,
        );
    }

    pub(in crate::game) fn spawn_loaded_entity_reason(
        &mut self,
        key: ChunkKey,
        mut tag: Tag,
        transferred: bool,
        reason: myriade_engine::world::EntityLoadReason,
    ) {
        let player_uuid = self.level.player_uuid;
        // A world older builds named by the offline profile names the player that
        // way in its entities too: migrate them as they load, so the next save of
        // the chunk keeps the account's UUID (`playerdata::migrate_owner`).
        if let Some((old, new)) = self.owner_migration {
            entity_nbt::migrate_player_uuid(
                &mut tag,
                myriade_engine::lan::playerdata::uuid_ints(old),
                myriade_engine::lan::playerdata::uuid_ints(new),
            );
        }
        // PersistentEntitySectionManager.addEntity's `loaded`.
        let from_disk = !take_fresh(&mut tag);
        take_moved(&mut tag);
        let kind = entity_nbt::kind(&tag).unwrap_or_default();
        // `EntityType.by` finds no type for a player or a kind outside the registry: nothing loads.
        self.load_entity_types();
        if !transferred
            && (kind == "minecraft:player"
                || (self.entity_types_loaded() && !self.entity_type_exists(&kind)))
        {
            return;
        }
        let is_mob = self.res.mob_types().contains_key(kind.as_str());
        if !transferred
            && from_disk
            && is_mob
            && entity_nbt::position(&tag).is_none()
            && !self.world.chunks.contains_key(&(0, 0))
        {
            // The decoder defaults Pos to zero, but that destination has
            // not been read. Keep the source record rather than admitting
            // an entity no resident chunk can save (or overwriting origin).
            // Admission is irreversible: a duplicate is rejected now,
            // not retried after its current owner has left the level.
            let duplicate = entity_nbt::uuid(&tag).is_some_and(|uuid| {
                self.restored_vehicle(uuid).is_some() || self.target_with_uuid(uuid).is_some()
            });
            if !duplicate {
                self.persist.unknown.entry(key).or_default().push(tag);
            }
            return;
        }
        drop_unleashable_leash(&mut tag, &kind, is_mob);
        let uuid = entity_nbt::uuid(&tag);
        // This shortcut deduplicates disk copies, never explicit runtime ownership.
        if !transferred && uuid.is_some_and(|u| self.restored_vehicle(u).is_some()) {
            return;
        }
        if (matches!(
            kind.as_str(),
            "minecraft:arrow"
                | "minecraft:spectral_arrow"
                | "minecraft:llama_spit"
                | "minecraft:shulker_bullet"
                | "minecraft:experience_bottle"
                | "minecraft:fireball"
                | "minecraft:small_fireball"
                | "minecraft:dragon_fireball"
                | "minecraft:wind_charge"
                | "minecraft:breeze_wind_charge"
                | "minecraft:wither_skull"
                | "minecraft:snowball"
                | "minecraft:egg"
                | "minecraft:ender_pearl"
                | "minecraft:splash_potion"
                | "minecraft:lingering_potion"
                | "minecraft:falling_block"
                | "minecraft:firework_rocket"
                | "minecraft:trident"
                | "minecraft:end_crystal"
                | "minecraft:ender_dragon"
        ) || myriade_engine::projectiles::thrown_type(&kind).is_some())
            && let Some(at) = entity_nbt::position(&tag)
        {
            let key = anon_key(&kind, at);
            let was = self.persist.anon.insert(key.clone(), (at, tag.clone()));
            self.journal_anon((key, was));
        }
        // Each tree has one loader: object-backed vehicles retain every rider for
        // their recursive loader; other kinds load the same ordered slots below.
        let passengers = (!transferred)
            .then(|| {
                let own_loader = is_mob
                    || super::super::boats::any_boat_wood(&kind).is_some()
                    || super::super::minecarts::CartKind::from_id(&kind).is_some()
                    || kind == "minecraft:tnt";
                self.take_unmodeled_passengers(&mut tag, own_loader)
            })
            .flatten();
        let before = self.next_entity;
        // Kinds outside `entity_lists!` first, then the listed kinds
        // (`Entity::load_nbt`), then whatever else is a compound.
        let unloaded = if kind == "minecraft:tnt"
            && !is_mob
            && reason == myriade_engine::world::EntityLoadReason::TeleportedTnt
        {
            self.load_tnt_with_portal(tag, uuid, true);
            Ok(())
        } else if transferred {
            self.load_listed_transferred(&kind, uuid, player_uuid, tag)
        } else {
            self.load_unlisted(key, &kind, uuid, tag)
                .or_else(|tag| self.load_listed(&kind, uuid, player_uuid, tag))
        };
        if let Err(tag) = unloaded {
            if transferred {
                log::warn!("refusing unsupported transferred entity kind {kind}");
                return;
            }
            if !kind.is_empty() && entity_nbt::position(&tag).is_some() {
                // A record no listed family claimed.
                self.load_other(tag, &kind);
            } else {
                self.persist.unknown.entry(key).or_default().push(tag);
            }
        }
        if let Some(id) =
            ((before + 1)..=self.next_entity).find(|id| self.level_entity(*id as i32).is_some())
            && let Some(passengers) = passengers
        {
            self.load_passengers(id, passengers);
        }
    }
}

impl EntityMemory {
    /// Puts back the raw record `kept` holds, or takes out the one a spawn added where none was:
    /// the abort of a transaction whose spawn wrote over it (`journal_anon`).
    pub(in crate::game) fn put_back_anon(
        &mut self,
        (key, was): ((String, [u64; 3]), Option<(Vec3, Tag)>),
    ) {
        match was {
            Some(record) => self.anon.insert(key, record),
            None => self.anon.remove(&key),
        };
    }
}
