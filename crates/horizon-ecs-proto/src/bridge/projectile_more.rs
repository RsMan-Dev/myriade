//! The arrows and the projectiles of the level, through `EcsWorld`: a launch of a kind of `projectile::KINDS`, a shot arrow with its pickup
//! and end effects, the pickups a player asks within reach, the host's answer to a pickup, and the arrow events since the last call.
use super::host::{EcsWorld, Select};
use crate::arrow_more::{self, ArrowShot, BaseDamage, Critical, FiredFrom, NoPhysics, ThrownItem, Turn};
use crate::behaviours::homing_projectile::Homing;
use crate::bridge::ids;
use crate::cow::{Pos, Vel, Yaw};
use crate::nbt_fields::common::Pitch;
use horizon_client_lib::items::ItemStack;
use crate::projectile::{self, Flight, Flying, Projectile};
use crate::shulker_bullet;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};

impl EcsWorld {
    /// Launches a projectile of `kind` (its identifier in `projectile::KINDS`) from `at` with `velocity`, shot by `owner`; None for an
    /// unknown kind. The mob world is lent for the call.
    pub fn launch(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], velocity: [f64; 3], owner: Option<Who>) -> Option<Entity> {
        let (_, flight) = *projectile::KINDS.iter().find(|(k, _)| *k == kind)?;
        let (at, velocity) = (vec3(at), vec3(velocity));
        Some(self.lent(mw, |w| projectile::launch(w, flight, at, velocity, owner)))
    }

    /// Launches a projectile of `kind` carrying `stack` (its item, `ThrownItem`); None for an unknown kind.
    pub fn launch_item(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], velocity: [f64; 3], owner: Option<Who>, stack: Option<ItemStack>) -> Option<Entity> {
        let e = self.launch(mw, kind, at, velocity, owner)?;
        if let Some(stack) = stack {
            self.world_mut().entity_mut(e).insert(ThrownItem(stack));
        }
        Some(e)
    }

    /// Shoots an arrow from `at` with `velocity`, owned by `owner`, carrying its pickup and end effects `shot`.
    pub fn shoot_arrow(&mut self, mw: &MobWorld<'_>, at: [f64; 3], velocity: [f64; 3], owner: Option<Who>, shot: ArrowShot, weapon: Option<ItemStack>) -> Entity {
        let (at, velocity) = (vec3(at), vec3(velocity));
        self.lent(mw, |w| {
            let e = projectile::launch(w, Flight::ARROW, at, velocity, owner);
            arrow_more::attach(w, e, shot, weapon);
            e
        })
    }

    /// The player `by` (a player or a guest, both `Who::Player`) asks for the stuck arrows within `reach` of `at`: a `PickedUp` event per
    /// arrow the rule lets it take.
    pub fn pickup(&mut self, at: [f64; 3], reach: f64, by: Who) {
        let near = self.select(&Select { near: Some((at, reach)), ..Default::default() });
        for e in near {
            arrow_more::pickup(self.world_mut(), e, by);
        }
    }

    /// The host's answer to the `PickedUp` of the arrow `e`: `took` removes it, otherwise it stays stuck.
    pub fn arrow_taken(&mut self, e: Entity, took: bool) {
        arrow_more::taken(self.world_mut(), e, took);
    }

    /// The kind identifier of the projectile `id` (its `Flight` id); None when `id` is no projectile.
    pub fn projectile_kind(&self, id: u32) -> Option<&'static str> {
        let e = self.entity_of(id)?;
        self.world().get::<Flight>(e).map(|f| f.id)
    }

    /// The shooter of the projectile `id`; None when it has none or `id` is no projectile.
    pub fn projectile_owner(&self, id: u32) -> Option<Who> {
        let e = self.entity_of(id)?;
        self.world().get::<projectile::Projectile>(e)?.owner
    }

    /// The box of the projectile `id` as (min, max) corners; None when `id` is no projectile.
    pub fn projectile_bounds(&self, id: u32) -> Option<([f64; 3], [f64; 3])> {
        let e = self.entity_of(id)?;
        let (at, size) = (self.world().get::<Pos>(e)?.0, self.world().get::<Flight>(e)?.size);
        let b = arrow_more::arrow_box(at, size);
        let (min, max) = (b.min_position(), b.max_position());
        Some(([min.x, min.y, min.z], [max.x, max.y, max.z]))
    }

    /// The linked ids of the projectiles whose box meets the box from `min` to `max`. Projectiles are not in the spatial index: this is a
    /// bounded query on the `Projectile` marker, to be called on events only (never per tick).
    pub fn projectiles_in(&self, min: [f64; 3], max: [f64; 3]) -> Vec<u32> {
        let area = Aabb::new(vec3(min), vec3(max));
        self.world()
            .iter_entities()
            .map(|r| r.id())
            .filter(|&e| self.world().get::<projectile::Projectile>(e).is_some())
            .filter(|&e| self.world().get::<Pos>(e).zip(self.world().get::<Flight>(e)).is_some_and(|(p, f)| arrow_more::arrow_box(p.0, f.size).intersects(area)))
            .filter_map(|e| ids::id_of(self.world(), e))
            .collect()
    }

    /// A breeze turns the projectile `id` back, owned by `by` (`arrow_more::deflect`); false when `id` is no projectile or is stuck.
    /// `dir` is not read by the reverse deflection.
    pub fn deflect(&mut self, id: u32, by: Option<Who>, dir: Vec3) -> bool {
        self.deflect_projectile(id, Turn::Reverse, [dir.x, dir.y, dir.z], by, false)
    }

    /// The deflection `turn` of the projectile `id` by `by` (`dir`: see `arrow_more::Turn`); `by_attack` is the jar's flag. False when `id`
    /// is no projectile or is stuck.
    pub fn deflect_projectile(&mut self, id: u32, turn: Turn, dir: [f64; 3], by: Option<Who>, by_attack: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        arrow_more::turn(self.world_mut(), e, turn, by, vec3(dir), by_attack)
    }

    /// True when the projectile `id` is an arrow or a spectral arrow.
    pub fn projectile_is_arrow(&self, id: u32) -> bool {
        matches!(self.projectile_kind(id), Some("minecraft:arrow") | Some(crate::species::lists::SPECTRAL_ARROW))
    }

    /// Sets the shooter of the projectile `id` to `by`; false when `id` is no projectile.
    pub fn set_projectile_owner(&mut self, id: u32, by: Option<Who>) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        match self.world_mut().get_mut::<Projectile>(e) {
            Some(mut p) => {
                p.owner = by;
                true
            }
            None => false,
        }
    }

    /// The base damage of the projectile `id`; None when unset or `id` is no projectile.
    pub fn projectile_base_damage(&self, id: u32) -> Option<f64> {
        self.world().get::<BaseDamage>(self.entity_of(id)?).map(|b| b.0)
    }

    /// Sets the base damage of the projectile `id`; false when `id` is no projectile.
    pub fn set_projectile_base_damage(&mut self, id: u32, damage: f64) -> bool {
        self.set_flag(id, BaseDamage(damage))
    }

    /// The critical flag of the projectile `id`; None when unset or `id` is no projectile.
    pub fn projectile_critical(&self, id: u32) -> Option<bool> {
        self.world().get::<Critical>(self.entity_of(id)?).map(|c| c.0)
    }

    /// Sets the critical flag of the projectile `id`; false when `id` is no projectile.
    pub fn set_projectile_critical(&mut self, id: u32, critical: bool) -> bool {
        self.set_flag(id, Critical(critical))
    }

    /// The `noPhysics` flag of the projectile `id`; None when unset or `id` is no projectile.
    pub fn projectile_no_physics(&self, id: u32) -> Option<bool> {
        self.world().get::<NoPhysics>(self.entity_of(id)?).map(|n| n.0)
    }

    /// Sets the `noPhysics` flag of the projectile `id`; false when `id` is no projectile.
    pub fn set_projectile_no_physics(&mut self, id: u32, no_physics: bool) -> bool {
        self.set_flag(id, NoPhysics(no_physics))
    }

    /// The ticks of fire the projectile `id` has left (`remainingFireTicks`); None when `id` is no projectile.
    pub fn projectile_fire_ticks(&self, id: u32) -> Option<u32> {
        let e = self.entity_of(id)?;
        self.world().get::<Projectile>(e)?;
        Some(crate::inside_effects::fire_left(self.world(), e) as u32)
    }

    /// Sets the fire of the projectile `id` to `ticks` from now (0 puts it out); false when `id` is no projectile.
    pub fn set_projectile_fire(&mut self, id: u32, ticks: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        if self.world().get::<Projectile>(e).is_none() {
            return false;
        }
        crate::inside_effects::ignite_projectile(self.world_mut(), e, u64::from(ticks));
        true
    }

    /// Sets the mod gate of the projectile `id` (`ModGate`): its impacts are announced and wait for `resolve_impact`; false when `id` is no
    /// projectile.
    pub fn set_mod_gate(&mut self, id: u32, on: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        if self.world().get::<Projectile>(e).is_none() {
            return false;
        }
        if on {
            self.world_mut().entity_mut(e).insert(arrow_more::ModGate);
        } else {
            self.world_mut().entity_mut(e).remove::<arrow_more::ModGate>();
        }
        true
    }

    /// The mod's answer on the suspended impact of the projectile `id`: `cancel` ends it with no impact, else its impact is applied on the
    /// next tick. False when `id` is not waiting.
    pub fn resolve_impact(&mut self, id: u32, cancel: bool) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        arrow_more::resolve(self.world_mut(), e, cancel)
    }

    /// Inserts `value` on the projectile `id`, only when it is a projectile.
    fn set_flag<T: Component>(&mut self, id: u32, value: T) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        if self.world().get::<Projectile>(e).is_none() {
            return false;
        }
        self.world_mut().entity_mut(e).insert(value);
        true
    }

    /// The item the projectile `id` carries (a thrown potion, egg, pearl); None when it carries none.
    pub fn projectile_item(&self, id: u32) -> Option<ItemStack> {
        let e = self.entity_of(id)?;
        self.world().get::<ThrownItem>(e).map(|t| t.0.clone())
    }

    /// The weapon the arrow `id` was fired from (`FiredFrom`); None when it has none or `id` is no projectile.
    pub fn projectile_weapon(&self, id: u32) -> Option<ItemStack> {
        let e = self.entity_of(id)?;
        self.world().get::<FiredFrom>(e).map(|w| w.0.clone())
    }

    /// Sets the velocity of the projectile `id`; false when `id` is no projectile.
    pub fn set_projectile_velocity(&mut self, id: u32, velocity: [f64; 3]) -> bool {
        self.set_flag(id, Vel(vec3(velocity)))
    }

    /// True when the projectile `id` is flying (its `Flying` marker, not stuck).
    pub fn projectile_flying(&self, id: u32) -> bool {
        self.entity_of(id).is_some_and(|e| self.world().get::<Flying>(e).is_some())
    }

    /// True when the projectile `id` is a homing shulker bullet.
    pub fn shulker_bullet(&self, id: u32) -> bool {
        self.entity_of(id).is_some_and(|e| self.world().get::<Homing>(e).is_some())
    }

    /// A player's blow on the shulker bullet `id` (`shulker_bullet::struck`): true when the bullet is destroyed.
    pub fn shulker_struck(&mut self, mw: &MobWorld<'_>, id: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.lent(mw, |w| shulker_bullet::struck(w, e))
    }

    /// The position, velocity, yaw and pitch of the projectile `id` (yaw and pitch 0 when it has none); None when `id` is no projectile.
    pub fn projectile_motion(&self, id: u32) -> Option<(Vec3, Vec3, f32, f32)> {
        let e = self.entity_of(id)?;
        let w = self.world();
        let pos = w.get::<Pos>(e)?.0;
        let vel = w.get::<Vel>(e).map_or(Vec3::new(0.0, 0.0, 0.0), |v| v.0);
        let yaw = w.get::<Yaw>(e).map_or(0.0, |y| y.0);
        let pitch = w.get::<Pitch>(e).map_or(0.0, |p| p.0);
        Some((pos, vel, yaw, pitch))
    }
}

fn vec3(a: [f64; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}
