//! Projectile handles for native server mods: owner, deflection, and an
//! arrow's pickup, damage and critical flag. The projectiles of the ECS
//! (arrows, tridents, thrown items) are read and written by their network id
//! through the bridge, rockets included.
use super::{Game, commands::Target};
use horizon_client::{
    arrows::Pickup,
    lan::playerdata::{ints_uuid, uuid_ints},
    mobs::Who,
    projectiles::{OwnerFields, Turn},
};
use horizon_core::{geometry::Vec3, nbt::{Compound, JavaString, Tag}};
use horizon_ecs_proto::arrow_more::Turn as EcsTurn;
use horizon_ecs_proto::targeting::Who as EcsWho;
use mod_api::{ArrowPickup, Deflection, EntityRef, PierceLevelError, ProjectileRef};

impl Game<'_> {
    /// The network id of a projectile entity of this server's ECS level; None
    /// for a stale handle, a client or an entity that is no projectile.
    pub(super) fn native_ecs_projectile(&self, entity: EntityRef) -> Option<u32> {
        let id = entity.id();
        (self.native_lifecycle.epoch() == Some(entity.epoch())
            && !self.held_out.contains(&id)
            && self.ecs.projectile_kind(id).is_some())
        .then_some(id)
    }

    fn native_projectile_backing(&self, projectile: ProjectileRef) -> Option<u32> {
        self.native_ecs_projectile(projectile.entity())
    }

    /// Narrows an entity to a projectile: an ECS projectile or a firework
    /// rocket. None for another kind, a stale handle or a client.
    pub(super) fn projectile(&self, entity: EntityRef) -> Option<ProjectileRef> {
        let projectile = ProjectileRef::new(entity);
        self.native_projectile_backing(projectile)
            .map(|_| projectile)
    }

    /// The ECS owner an entity stands for: a player or a guest by its entity
    /// id, another entity by its ECS entity. The outer None for a stale entity.
    fn native_ecs_who(&self, owner: Option<EntityRef>) -> Option<Option<EcsWho>> {
        let Some(owner) = owner else {
            return Some(None);
        };
        let who = match self.native_entity_target(owner)? {
            Target::Player => EcsWho::Player(self.player.as_ref()?.entity_id as u32),
            Target::Guest(conn) => EcsWho::Player(
                self.lan.as_ref()?.hub.guests.iter().find(|g| g.conn == conn)?.entity_id as u32,
            ),
            _ => EcsWho::Entity(self.ecs.entity_of(owner.id())?),
        };
        Some(Some(who))
    }

    /// `Projectile.getOwner`: the owner as a resolved entity. None for no
    /// owner, one that is gone or in another level, or a stale handle.
    pub(super) fn projectile_owner(&self, projectile: ProjectileRef) -> Option<EntityRef> {
        let id = self.native_projectile_backing(projectile)?;
        match self.ecs.projectile_owner(id)? {
            EcsWho::Player(p) => self.entity(p),
            EcsWho::Entity(e) => self.entity(self.ecs.network_id(e)?),
        }
    }

    /// Resolves hit credit, persistent identity, and a separate non-living
    /// vehicle identity for collision exclusion. None for a stale owner or hook.
    pub(super) fn native_projectile_owner_fields(
        &mut self,
        owner: Option<EntityRef>,
    ) -> Option<(OwnerFields, Option<u32>)> {
        let Some(owner) = owner else {
            return Some(((None, None, None), None));
        };
        let target = self.native_entity_target(owner)?;
        let vehicle = match target {
            Target::Boat(id) | Target::Cart(id) => Some(id),
            _ => None,
        };
        let (who, guest) = match target {
            Target::Player => (Some(Who::Player), self.lan_acting),
            Target::Guest(conn) => (Some(Who::Player), Some(conn)),
            Target::Mob(id) => (Some(Who::Mob(id)), None),
            Target::Hook(_) => return None,
            _ => (None, None),
        };
        let uuid = uuid_ints(self.entity_uuid(owner)?);
        Some(((who, Some(uuid), guest), vehicle))
    }

    /// `Projectile.setOwner`. False for a stale projectile or owner, or an
    /// owner that is a fishing hook. Does not touch `leftOwner`.
    pub(super) fn projectile_set_owner(
        &mut self,
        projectile: ProjectileRef,
        owner: Option<EntityRef>,
    ) -> bool {
        match self.native_projectile_backing(projectile) {
            Some(id) => match self.native_ecs_who(owner) {
                Some(who) => self.ecs.set_projectile_owner(id, who),
                None => false,
            },
            None => false,
        }
    }

    /// `Projectile.deflect(deflection, by, newOwner, byAttack, power)`, with
    /// `power` per axis. An ECS projectile takes the turn through the bridge;
    /// `REVERSE` draws one float from the projectile's own random;
    /// `AIM_DEFLECT` uses `by`'s look and `MOMENTUM_DEFLECT` its normalized
    /// known speed, and both change nothing without `by`. A rocket turns the
    /// same way and takes its new owner. False, and nothing changed, for a
    /// stale projectile, `by` or owner, or a non-finite power.
    pub(super) fn projectile_deflect(
        &mut self,
        projectile: ProjectileRef,
        deflection: Deflection,
        by: Option<EntityRef>,
        new_owner: Option<EntityRef>,
        by_attack: bool,
        power: [f64; 3],
    ) -> bool {
        let Some(id) = self.native_projectile_backing(projectile) else {
            return false;
        };
        let Some(new_who) = self.native_ecs_who(new_owner) else {
            return false;
        };
        if !power.iter().all(|v| v.is_finite()) {
            return false;
        }
        let turn = match (deflection, by) {
            (Deflection::Reverse, _) => Turn::Reverse,
            (_, None) => Turn::Direction(None),
            (Deflection::AimDeflect, Some(by)) => {
                let Some([x, y, z]) = self.entity_view_vector(by) else {
                    return false;
                };
                Turn::Direction(Some(Vec3::new(x, y, z)))
            }
            (Deflection::MomentumDeflect, Some(by)) => {
                let Some(speed) = self.native_entity_known_speed(by) else {
                    return false;
                };
                Turn::Direction(Some(speed.normalize()))
            }
            _ => unreachable!("(Deflection, Option<EntityRef>): unhandled variant"),
        };
            let (ecs_turn, dir) = match turn {
                Turn::Reverse => (EcsTurn::Reverse, power),
                Turn::Direction(None) => (EcsTurn::None, power),
                Turn::Direction(Some(d)) => {
                    (EcsTurn::AimDeflect, [d.x * power[0], d.y * power[1], d.z * power[2]])
                }
            };
        self.ecs.deflect_projectile(id, ecs_turn, dir, new_who, by_attack)
    }

    /// The NBT value under `key` of the ECS projectile `id`; None for a stale
    /// id or an absent key.
    fn native_projectile_tag(&self, id: u32, key: &str) -> Option<Tag> {
        self.ecs.entity_nbt(self.ecs.entity_of(id)?)?.get(&JavaString::from(key)).cloned()
    }

    /// Merges `tag` under `key` into the ECS projectile `id`; false when it has no entity.
    pub(super) fn native_projectile_merge(&mut self, id: u32, key: &str, tag: Tag) -> bool {
        let Some(e) = self.ecs.entity_of(id) else {
            return false;
        };
        let mut patch = Compound::new();
        patch.insert(key.into(), tag);
        self.ecs.merge_nbt(e, &patch)
    }

    /// `AbstractArrow.pickup`, for an arrow or a trident.
    pub(super) fn projectile_pickup(&self, projectile: ProjectileRef) -> Option<ArrowPickup> {
        let id = self.native_projectile_backing(projectile)?;
        let Tag::Byte(byte) = self.native_projectile_tag(id, "pickup")? else {
            return None;
        };
        Some(match Pickup::from_byte(i64::from(byte)) {
            Pickup::Disallowed => ArrowPickup::Disallowed,
            Pickup::Allowed => ArrowPickup::Allowed,
            Pickup::CreativeOnly => ArrowPickup::CreativeOnly,
        })
    }

    /// Sets `AbstractArrow.pickup`. False for a stale handle or a projectile
    /// that is not an arrow or a trident (no `pickup` key).
    pub(super) fn projectile_set_pickup(
        &mut self,
        projectile: ProjectileRef,
        pickup: ArrowPickup,
    ) -> bool {
        let Some(id) = self.native_projectile_backing(projectile) else {
            return false;
        };
        if self.native_projectile_tag(id, "pickup").is_none() {
            return false;
        }
        let pickup = match pickup {
            ArrowPickup::Disallowed => Pickup::Disallowed,
            ArrowPickup::Allowed => Pickup::Allowed,
            ArrowPickup::CreativeOnly => Pickup::CreativeOnly,
            _ => unreachable!("ArrowPickup: unhandled variant"),
        };
        self.native_projectile_merge(id, "pickup", Tag::Byte(pickup.byte()))
    }

    /// The network id of an arrow handle; the error names what refuses it.
    fn native_arrow_id(&self, projectile: ProjectileRef) -> Result<u32, PierceLevelError> {
        let id = self.native_projectile_backing(projectile).ok_or(PierceLevelError::Stale)?;
        if self.ecs.projectile_is_arrow(id) {
            Ok(id)
        } else {
            Err(PierceLevelError::NotArrow)
        }
    }

    /// `AbstractArrow.getPierceLevel`: the signed byte stored by vanilla.
    pub(super) fn projectile_pierce_level(
        &self,
        projectile: ProjectileRef,
    ) -> Result<i8, PierceLevelError> {
        let id = self.native_arrow_id(projectile)?;
        Ok(match self.native_projectile_tag(id, "PierceLevel") {
            Some(Tag::Byte(level)) => level,
            _ => 0,
        })
    }

    /// `AbstractArrow.setPierceLevel`: stores the signed byte without clamping.
    pub(super) fn projectile_set_pierce_level(
        &mut self,
        projectile: ProjectileRef,
        level: i8,
    ) -> Result<(), PierceLevelError> {
        let id = self.native_arrow_id(projectile)?;
        self.native_projectile_merge(id, "PierceLevel", Tag::Byte(level));
        Ok(())
    }

    /// `AbstractArrow.getBaseDamage`, for an arrow (a trident's is fixed).
    pub(super) fn projectile_base_damage(&self, projectile: ProjectileRef) -> Option<f64> {
        let id = self.native_projectile_backing(projectile)?;
        self.ecs.projectile_base_damage(id)
    }

    /// `AbstractArrow.setBaseDamage`. False for a stale handle, a projectile
    /// that is not an arrow, or a value that is not finite.
    pub(super) fn projectile_set_base_damage(
        &mut self,
        projectile: ProjectileRef,
        damage: f64,
    ) -> bool {
        let Ok(id) = self.native_arrow_id(projectile) else {
            return false;
        };
        damage.is_finite() && self.ecs.set_projectile_base_damage(id, damage)
    }

    /// `AbstractArrow.isCritArrow`, for an arrow.
    pub(super) fn projectile_critical(&self, projectile: ProjectileRef) -> Option<bool> {
        let id = self.native_projectile_backing(projectile)?;
        self.ecs.projectile_critical(id)
    }

    /// `AbstractArrow.setCritArrow`; clients see the flag at once through the
    /// entity data. False for a stale handle or a projectile that is not an arrow.
    pub(super) fn projectile_set_critical(
        &mut self,
        projectile: ProjectileRef,
        critical: bool,
    ) -> bool {
        let Ok(id) = self.native_arrow_id(projectile) else {
            return false;
        };
        self.ecs.set_projectile_critical(id, critical)
    }
}
