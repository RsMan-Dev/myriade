//! An entity's `Entity.noPhysics` for native server mods, through an
//! `EntityRef` (docs/modding/src/how-to/native-mods.md, "Entity and level
//! handles"): whether its moves pass through blocks. Offered only for the
//! kinds whose flag has one value between ticks; an item recomputes its own
//! inside its tick, so it reads None, as do the other kinds, a stale handle
//! and a client. The write is refused for the three kinds whose own tick
//! rewrites the flag; the ender dragon, an area-effect cloud, an ominous item
//! spawner and a shulker bullet take it, starting from the true value their
//! constructors set.
use super::{Game, commands::Target};
use horizon_client::mobs::Species;
use horizon_core::nbt::Tag;
use mod_api::EntityRef;

impl Game<'_> {
    /// `Entity.noPhysics`: a player's is whether it spectates (`Player.tick`
    /// sets it from `isSpectator`); a mob's is the flag a mod wrote, false
    /// until then (a vex sets it only for the length of its own tick); an
    /// arrow's or a trident's is its `AbstractArrow.noPhysics`, set once a
    /// loyal trident turns back. The ender dragon, an area-effect cloud, an
    /// ominous item spawner and a shulker bullet set it in their constructors
    /// and never clear it, so theirs is the value a mod wrote, or that
    /// constructor default (`true`) until then. An armor stand's is recomputed
    /// on load, so it keeps a written value only until then; a kind Horizon
    /// does not simulate keeps it in its saved compound.
    pub(super) fn entity_no_physics(&self, entity: EntityRef) -> Option<bool> {
        if let Some(id) = self.native_ecs_projectile(entity) {
            return Some(self.ecs_flag(id, "NoPhysics").unwrap_or(false));
        }
        match self.native_entity_target(entity)? {
            Target::Player => Some(self.player.as_ref()?.spectator),
            Target::Guest(conn) => Some(self.command_guest(conn)?.motion.as_deref()?.spectator),
            Target::Mob(id) if self.ecs_has(id) => self.ecs_flag(id, "NoPhysics"),
            Target::Mob(id) => Some(self.remote_entities.iter().find(|m| m.id == id)?.no_physics),
            Target::Dragon(id) | Target::Cloud(id) | Target::Spawner(id) => {
                Some(self.stored_no_physics(id))
            }
            Target::Stand(id) => {
                let stand = self.armor_stand_saved(id)?;
                Some(stand.no_physics.unwrap_or(stand.marker || stand.no_gravity))
            }
            Target::Other(id) => Some(
                self.persist
                    .raw
                    .get(&id)
                    .is_some_and(|tag| horizon_client::entity_nbt::flag(tag, "NoPhysics")),
            ),
            _ => None,
        }
    }

    /// `Entity.noPhysics` for a kind that keeps no field of its own: the value
    /// a mod wrote, or the kind's constructor default (`true` for the dragon,
    /// an area-effect cloud, an ominous item spawner and a shulker bullet).
    fn stored_no_physics(&self, id: u32) -> bool {
        self.persist.no_physics.get(&id).copied().unwrap_or(true)
    }

    /// `Entity.noPhysics` for a native server mod: the value
    /// `entity_no_physics` reads. Server-only; a player, an item entity and a
    /// vex rewrite the flag every tick (`Player.tick`, `ItemEntity.tick`,
    /// `Vex.tick`), so they are refused by kind, and so is a client's write.
    /// An arrow's or a trident's write carries the synced flag clients read.
    /// An armor stand recomputes the flag on load, so its write is not
    /// persisted; an unsimulated kind keeps it in its saved compound. The
    /// dragon, an area-effect cloud, an ominous item spawner and a shulker
    /// bullet take the write, which replaces their constructor's `true`.
    pub(super) fn entity_set_no_physics(&mut self, entity: EntityRef, no_physics: bool) -> bool {
        if !self.is_logical_server() {
            return false;
        }
        if let Some(id) = self.native_ecs_projectile(entity) {
            return self.native_projectile_merge(id, "NoPhysics", Tag::Byte(i8::from(no_physics)));
        }
        match self.native_entity_target(entity) {
            Some(Target::Mob(id)) if self.ecs_has(id) => {
                // A vex's `noPhysics` is its own to set (`Vex.tick`).
                if self.ecs_mob(id).is_some_and(|m| m.species == Some(Species::Vex)) {
                    return false;
                }
                self.ecs_put_flag(id, "NoPhysics", no_physics)
            }
            Some(Target::Mob(id)) => {
                let Some(mob) = self.remote_entities.iter_mut().find(|m| m.id == id) else {
                    return false;
                };
                if mob.species() == Species::Vex {
                    return false;
                }
                mob.no_physics = no_physics;
                true
            }
            Some(Target::Dragon(id) | Target::Cloud(id) | Target::Spawner(id)) => {
                self.persist.no_physics.insert(id, no_physics);
                true
            }
            Some(Target::Stand(id)) => self.armor_stand_edit(id, |s| s.no_physics = Some(no_physics)),
            // No `others` list: nothing of this kind is held to flag.
            Some(Target::Other(_)) => false,
            _ => false,
        }
    }

    /// `/kill` and the native discard drop a projectile from its list: the
    /// `noPhysics` write stored for a shulker bullet leaves with it.
    pub(super) fn remove_projectile(&mut self, id: u32) {
        if let Some(e) = self.ecs.entity_of(id) {
            self.ecs.despawn(e);
        }
        self.forget_entity(id);
    }

    /// A command drops an ominous item spawner: its stored `noPhysics` write
    /// leaves with it.
    pub(super) fn remove_ominous_spawner(&mut self, id: u32) {
        if let Some(e) = self.ecs.entity_of(id) {
            self.ecs.despawn(e);
        }
        self.forget_entity(id);
    }

    /// An ominous item spawner finishing inside a `retain_mut`: forget its
    /// stored `noPhysics` write and refuse it, in one call.
    pub(super) fn forget_finished_spawner(&mut self, id: u32) -> bool {
        self.forget_entity(id);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The write is what the movement code reads, not only the read-back: a
    /// shulker's lid (`piston_push::lid_push`, the gate that asks
    /// `Entity::no_physics`) pushes a bullet whose flag a mod cleared and
    /// spares one left at the kind's default `true`.
    #[test]
    #[ignore = "daily"]
    fn no_physics_entry_is_cleared_when_an_ominous_item_spawner_finishes() {
        use super::super::native_entity_refs::tests::with_game;
        use horizon_client::items::ItemStack;

        with_game(|game| {
            game.add_ominous_item_spawner(ItemStack::new("minecraft:air", 1), [4.5, 200.0, 4.5], 1);
            let id = game.ominous_ids()[0];
            game.persist.no_physics.insert(id, false);
            assert!(game.persist.no_physics.contains_key(&id));

            game.tick_ecs().unwrap();
            assert!(
                !game.ominous_ids().contains(&id),
                "the spawner finished"
            );
            assert!(
                !game.persist.no_physics.contains_key(&id),
                "the entry left with the spawner that finished"
            );
        });
    }


    #[test]
    #[ignore = "daily"]
    fn no_physics_write_is_refused_on_a_client() {
        use super::super::native_entity_refs::tests::with_client_game;
        use mod_api::WorldEpoch;

        with_client_game(|game| {
            let entity = EntityRef::new(WorldEpoch::new(0), 1);
            assert!(!entity.set_no_physics(game, true));
            assert!(!entity.set_no_physics(game, false));
        });
    }
}
