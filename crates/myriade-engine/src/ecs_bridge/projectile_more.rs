//! The arrows and projectiles on `EcsLevel`: a launch by the host (the owner is the host's `Who`), a shot arrow, the pickups a player asks
//! within reach, the host's answer to a pickup, and the arrow events since the last call. Through `ecs()` / `ecs_mut()`: an inert level
//! launches nothing and gives nothing.
use super::level::EcsLevel;
use bevy_ecs::prelude::Entity;
use myriade_engine_lib::items::ItemStack;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{arrow_more::{ArrowShot, Turn}, targeting::Who};

impl EcsLevel {
    /// Launches a projectile of `kind` from `at` with `velocity`, shot by `owner`; returns its network id (0 when inert or the kind is unknown).
    pub fn launch(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], velocity: [f64; 3], owner: Option<Who>, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(ecs) = self.ecs_mut() else { return 0 };
        let Some(e) = ecs.launch(mw, kind, at, velocity, owner) else { return 0 };
        self.id_for(e, alloc)
    }

    /// Launches a projectile of `kind` carrying `stack` (its item, None for none); returns its network id (0 when inert or unknown).
    pub fn launch_item(&mut self, mw: &MobWorld<'_>, kind: &str, at: [f64; 3], velocity: [f64; 3], owner: Option<Who>, stack: Option<ItemStack>, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(ecs) = self.ecs_mut() else { return 0 };
        let Some(e) = ecs.launch_item(mw, kind, at, velocity, owner, stack) else { return 0 };
        self.id_for(e, alloc)
    }

    /// Shoots an arrow from `at` with `velocity`, owned by `owner` and carrying `shot`; returns its network id (0 when inert).
    pub fn shoot_arrow(&mut self, mw: &MobWorld<'_>, at: [f64; 3], velocity: [f64; 3], owner: Option<Who>, shot: ArrowShot, weapon: Option<ItemStack>, alloc: &mut dyn FnMut() -> u32) -> u32 {
        let Some(ecs) = self.ecs_mut() else { return 0 };
        let e = ecs.shoot_arrow(mw, at, velocity, owner, shot, weapon);
        self.id_for(e, alloc)
    }

    /// The player `by` (a player or a guest) asks for the stuck arrows within `reach` of `at`: the `PickedUp` events come with `arrow_events`.
    pub fn arrow_pickup(&mut self, at: [f64; 3], reach: f64, by: Who) {
        if let Some(ecs) = self.ecs_mut() {
            ecs.pickup(at, reach, by);
        }
    }

    /// The host's answer to the `PickedUp` of the arrow `arrow`: `took` removes it, otherwise it stays stuck.
    pub fn arrow_taken(&mut self, arrow: Entity, took: bool) {
        if let Some(ecs) = self.ecs_mut() {
            ecs.arrow_taken(arrow, took);
        }
    }

    /// The kind identifier of the projectile `id`; None when inert or `id` is no projectile.
    pub fn projectile_kind(&self, id: u32) -> Option<&'static str> {
        self.ecs()?.projectile_kind(id)
    }

    /// The shooter of the projectile `id`; None when it has none, is inert or `id` is no projectile.
    pub fn projectile_owner(&self, id: u32) -> Option<Who> {
        self.ecs()?.projectile_owner(id)
    }

    /// The box of the projectile `id` as (min, max) corners; None when inert or `id` is no projectile.
    pub fn projectile_bounds(&self, id: u32) -> Option<([f64; 3], [f64; 3])> {
        self.ecs()?.projectile_bounds(id)
    }

    /// The ids of the linked projectiles whose box meets the box from `min` to `max`; none when inert. A bounded query on the `Projectile`
    /// marker (projectiles are not in the spatial index): to be called on events only, never per tick.
    pub fn projectiles_in(&self, min: [f64; 3], max: [f64; 3]) -> Vec<u32> {
        self.ecs().map_or_else(Vec::new, |e| e.projectiles_in(min, max))
    }

    /// Turns the projectile `id` back, owned by `by`; false when inert, stuck or `id` is no projectile.
    pub fn deflect(&mut self, id: u32, by: Option<Who>, dir: Vec3) -> bool {
        self.ecs_mut().is_some_and(|e| e.deflect(id, by, dir))
    }

    /// The network id of the linked entity `e`; None when inert or `e` has none (`EcsWorld::id_of`).
    pub fn network_id(&self, e: Entity) -> Option<u32> {
        self.ecs()?.id_of(e)
    }

    /// The deflection `turn` of the projectile `id` by `by`; false when inert, stuck or `id` is no projectile.
    pub fn deflect_projectile(&mut self, id: u32, turn: Turn, dir: [f64; 3], by: Option<Who>, by_attack: bool) -> bool {
        self.ecs_mut().is_some_and(|e| e.deflect_projectile(id, turn, dir, by, by_attack))
    }

    /// True when the projectile `id` is an arrow or a spectral arrow; false when inert.
    pub fn projectile_is_arrow(&self, id: u32) -> bool {
        self.ecs().is_some_and(|e| e.projectile_is_arrow(id))
    }

    /// Sets the shooter of the projectile `id`; false when inert or `id` is no projectile.
    pub fn set_projectile_owner(&mut self, id: u32, by: Option<Who>) -> bool {
        self.ecs_mut().is_some_and(|e| e.set_projectile_owner(id, by))
    }

    /// The base damage of the projectile `id`; None when unset, inert or `id` is no projectile.
    pub fn projectile_base_damage(&self, id: u32) -> Option<f64> {
        self.ecs()?.projectile_base_damage(id)
    }

    /// Sets the base damage of the projectile `id`; false when inert or `id` is no projectile.
    pub fn set_projectile_base_damage(&mut self, id: u32, damage: f64) -> bool {
        self.ecs_mut().is_some_and(|e| e.set_projectile_base_damage(id, damage))
    }

    /// The critical flag of the projectile `id`; None when unset, inert or `id` is no projectile.
    pub fn projectile_critical(&self, id: u32) -> Option<bool> {
        self.ecs()?.projectile_critical(id)
    }

    /// Sets the critical flag of the projectile `id`; false when inert or `id` is no projectile.
    pub fn set_projectile_critical(&mut self, id: u32, critical: bool) -> bool {
        self.ecs_mut().is_some_and(|e| e.set_projectile_critical(id, critical))
    }

    /// The `noPhysics` flag of the projectile `id`; None when unset, inert or `id` is no projectile.
    pub fn projectile_no_physics(&self, id: u32) -> Option<bool> {
        self.ecs()?.projectile_no_physics(id)
    }

    /// Sets the `noPhysics` flag of the projectile `id`; false when inert or `id` is no projectile.
    pub fn set_projectile_no_physics(&mut self, id: u32, no_physics: bool) -> bool {
        self.ecs_mut().is_some_and(|e| e.set_projectile_no_physics(id, no_physics))
    }

    /// The item the projectile `id` carries; None when inert or it carries none.
    pub fn projectile_item(&self, id: u32) -> Option<ItemStack> {
        self.ecs()?.projectile_item(id)
    }

    /// The weapon the arrow `id` was fired from; None when inert, it has none or `id` is no projectile.
    pub fn projectile_weapon(&self, id: u32) -> Option<ItemStack> {
        self.ecs()?.projectile_weapon(id)
    }

    /// Sets the velocity of the projectile `id`; false when inert or `id` is no projectile.
    pub fn set_projectile_velocity(&mut self, id: u32, velocity: [f64; 3]) -> bool {
        self.ecs_mut().is_some_and(|e| e.set_projectile_velocity(id, velocity))
    }

    /// True when the projectile `id` is flying; false when inert or `id` is no projectile.
    pub fn projectile_flying(&self, id: u32) -> bool {
        self.ecs().is_some_and(|e| e.projectile_flying(id))
    }

    /// True when the projectile `id` is a shulker bullet; false when inert.
    pub fn shulker_bullet(&self, id: u32) -> bool {
        self.ecs().is_some_and(|e| e.shulker_bullet(id))
    }

    /// A player's blow on the shulker bullet `id`; true when the bullet is destroyed (false when inert).
    pub fn shulker_struck(&mut self, mw: &MobWorld<'_>, id: u32) -> bool {
        self.ecs_mut().is_some_and(|e| e.shulker_struck(mw, id))
    }

    /// The ticks of fire the projectile `id` has left; None when inert or `id` is no projectile.
    pub fn projectile_fire_ticks(&self, id: u32) -> Option<u32> {
        self.ecs()?.projectile_fire_ticks(id)
    }

    /// Sets the fire of the projectile `id` to `ticks` from now (0 puts it out); false when inert or `id` is no projectile.
    pub fn set_projectile_fire(&mut self, id: u32, ticks: u32) -> bool {
        self.ecs_mut().is_some_and(|e| e.set_projectile_fire(id, ticks))
    }

    /// Sets the mod gate of the projectile `id`; false when inert or `id` is no projectile.
    pub fn set_mod_gate(&mut self, id: u32, on: bool) -> bool {
        self.ecs_mut().is_some_and(|e| e.set_mod_gate(id, on))
    }

    /// The mod's answer on the suspended impact of the projectile `id` (`cancel` ends it); false when inert or not waiting.
    pub fn resolve_impact(&mut self, id: u32, cancel: bool) -> bool {
        self.ecs_mut().is_some_and(|e| e.resolve_impact(id, cancel))
    }

    /// The position, velocity, yaw and pitch of the projectile `id`; None when inert or `id` is no projectile.
    pub fn projectile_motion(&self, id: u32) -> Option<(Vec3, Vec3, f32, f32)> {
        self.ecs()?.projectile_motion(id)
    }

    /// The typed fields of the projectile `id`, read from its components; None when inert or `id` is no projectile.
    pub fn projectile_view(&self, id: u32) -> Option<ProjectileView> {
        let (pos, vel, _, _) = self.projectile_motion(id)?;
        Some(ProjectileView {
            kind: self.projectile_kind(id)?,
            owner: self.projectile_owner(id),
            item: self.projectile_item(id),
            arrow: self.projectile_is_arrow(id),
            pos: [pos.x, pos.y, pos.z],
            velocity: [vel.x, vel.y, vel.z],
            fire_ticks: self.projectile_fire_ticks(id).unwrap_or(0),
        })
    }
}

/// The fields of one projectile, as `EcsLevel::projectile_view` reads them.
pub struct ProjectileView {
    pub kind: &'static str,
    pub owner: Option<Who>,
    pub item: Option<ItemStack>,
    pub arrow: bool,
    pub pos: [f64; 3],
    pub velocity: [f64; 3],
    /// `Entity.remainingFireTicks`.
    pub fire_ticks: u32,
}
