//! The mounts of the ECS entities, by `Entity` (greffe lot 70): the host's sites hold the entity they got from `entity_of`, and each call
//! reads or writes the bridge's `EcsWorld` (bridge/host_mount.rs). An inert level answers as an empty world: 0, none, false, nothing.
use super::level::EcsLevel;
use bevy_ecs::entity::Entity;
use horizon_client_lib::mobs::MobWorld;

impl EcsLevel {
    /// The jump charge of the mount `e` (`EcsWorld::charge_jump`); 0.0 when inert.
    pub fn charge_jump(&mut self, e: Entity, pressed: bool) -> f32 {
        self.ecs_mut().map_or(0.0, |b| b.charge_jump(e, pressed))
    }
    /// The rider's keys of the mount `e` (`EcsWorld::ride_input`).
    #[allow(clippy::too_many_arguments)]
    pub fn ride_input(&mut self, e: Entity, move_xz: [f32; 2], jump: f32, jump_held: bool, sprint: bool, sneak: bool, yaw: f32, pitch: f32) {
        if let Some(b) = self.ecs_mut() {
            b.ride_input(e, move_xz, jump, jump_held, sprint, sneak, yaw, pitch);
        }
    }
    /// The jump of the mount `e` starts (`EcsWorld::start_jump`): the sound and its volume.
    pub fn start_jump(&mut self, e: Entity, data: i32) -> Option<(&'static str, f32)> {
        self.ecs_mut()?.start_jump(e, data)
    }
    pub fn jump_sound(&self, e: Entity) -> Option<(&'static str, f32)> {
        self.ecs()?.jump_sound(e)
    }
    pub fn jumpable(&self, e: Entity) -> bool {
        self.ecs().is_some_and(|b| b.jumpable(e))
    }
    /// The player `player` gets off the mount `id` (`EcsWorld::dismount_player`).
    pub fn dismount_player(&mut self, id: u32, player: u32) -> bool {
        let Some(e) = self.entity_of(id) else { return false };
        self.ecs_mut().is_some_and(|b| b.dismount_player(e, player))
    }
    pub fn dashes(&self, e: Entity) -> bool {
        self.ecs().is_some_and(|b| b.dashes(e))
    }
    /// Whether the mount `id` is steered by its rider's look (`EcsWorld::steers_by_look`).
    pub fn steers_by_look(&self, id: u32) -> bool {
        self.entity_of(id).zip(self.ecs()).is_some_and(|(e, w)| w.steers_by_look(e))
    }
    pub fn has_jump_cooldown(&self, e: Entity) -> bool {
        self.ecs().is_some_and(|b| b.has_jump_cooldown(e))
    }
    /// A player is the controlling passenger of the mount `id` (saddled, and holding the stick when it steers by stick).
    pub fn player_steers(&self, id: u32) -> bool {
        self.entity_of(id).zip(self.ecs()).is_some_and(|(e, w)| w.player_steers(e))
    }

    /// The max health and the health of `e`.
    pub fn vehicle_health(&self, e: Entity) -> Option<(f32, f32)> {
        self.ecs()?.vehicle_health(e)
    }
    /// The point the first rider of `e` sits at.
    pub fn rider_seat(&self, e: Entity) -> Option<[f64; 3]> {
        self.ecs()?.rider_seat(e)
    }
    /// Where the rider of `e` gets off (`EcsWorld::dismount_point`), the floors and collisions read in `mw`.
    pub fn dismount_point(&self, mw: &MobWorld<'_>, e: Entity) -> Option<[f64; 3]> {
        self.ecs()?.dismount_point(mw, e)
    }
    pub fn dismounts_in_water(&self, e: Entity, eyes_in_water: bool) -> bool {
        self.ecs().is_some_and(|b| b.dismounts_in_water(e, eyes_in_water))
    }
    /// Once a tick, on the ridden mounts only (`EcsWorld::dismount_underwater`).
    pub fn dismount_underwater(&mut self) {
        if let Some(b) = self.ecs_mut() {
            b.dismount_underwater();
        }
    }
    /// The creature riders of `e` take a fall (`EcsWorld::fall_riders`).
    pub fn fall_riders(&mut self, e: Entity, distance: f64, modifier: f32) {
        if let Some(b) = self.ecs_mut() {
            b.fall_riders(e, distance, modifier);
        }
    }
    /// `mobInteract` of the mount `e` by `player` with `item` (`EcsWorld::mount_interact`): the saddle, or the player's ride.
    pub fn mount_interact(&mut self, e: Entity, player: u32, item: &str, sneaking: bool) -> bool {
        self.ecs_mut().is_some_and(|b| b.mount_interact(e, player, item, sneaking))
    }
    /// Shears take the saddle off `e` (`EcsWorld::shear_saddle`).
    pub fn shear_saddle(&mut self, e: Entity, item: &str) -> Option<bool> {
        self.ecs_mut()?.shear_saddle(e, item)
    }
    /// Every rider of `e` gets off (`EcsWorld::refuse_rider`).
    pub fn refuse_rider(&mut self, e: Entity) {
        if let Some(b) = self.ecs_mut() {
            b.refuse_rider(e);
        }
    }
}
