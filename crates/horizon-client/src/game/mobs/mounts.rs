//! Riding horses, donkeys and mules: feeding (`handleEating`), saddles,
//! armour and chests, mounting, the rider's control and jump charge
//! (`LocalPlayer.aiStep`'s riding jump), positioning, dismounting.
use super::Game;
use horizon_client::{
    blocks::Fluid,
    mobs::{Mob, MobEvent, Species, Who, equine},
    motion::{MotionWorld, PlayerMotion},
    pose::{Dimensions, Pose},
    sprint::Keys,
};
use horizon_core::{
    geometry::{Aabb, Vec3, VoxelShape, trig},
};

pub(super) fn steering_item(species: Species, main: Option<&str>, offhand: Option<&str>) -> bool {
    let item = match species {
        Species::Pig => "minecraft:carrot_on_a_stick",
        Species::Strider => "minecraft:warped_fungus_on_a_stick",
        _ => return false,
    };
    main == Some(item) || offhand == Some(item)
}

#[derive(Default)]
pub(super) struct MountState {
    /// `jumpRidingTicks`: -10 cooling down after a jump, >= 0 charging.
    pub(super) jump_ticks: i32,
    /// `jumpRidingScale` (0..1).
    pub(super) jump_scale: f32,
    was_jumping: bool,
}
impl MountState {
    pub(super) fn reset_jump(&mut self) {
        self.jump_ticks = 0;
        self.jump_scale = 0.0;
    }
}

impl Game<'_> {
    /// `FoodOnAStickItem.use` while riding: boost the mount, wear the stick.
    pub(super) fn use_steering_stick(&mut self, item: &str) -> bool {
        let (kind, wear) = match item {
            "minecraft:carrot_on_a_stick" => ("minecraft:pig", 7),
            "minecraft:warped_fungus_on_a_stick" => ("minecraft:strider", 1),
            _ => return false,
        };
        if !self.ecs_boost_steered(kind) {
            return false;
        }
        if let Some(stack) = self.inventory[self.selected].clone() {
            let (updated, broken) = self.damage_stack(&stack, wear, true);
            if self.inventory[self.selected].as_ref() == Some(&stack) {
                self.inventory[self.selected] = updated;
                if let Some(broken) = broken {
                    self.player_equipment_broke(
                        horizon_core::equipment_slot::EquipmentSlot::MainHand,
                        &broken,
                    );
                    self.award_stat("broken", &broken.item, 1);
                    // hurtAndConvertOnBreak keeps the broken stick's components.
                    let mut rod = stack.transmute("minecraft:fishing_rod", 1);
                    rod.damage = 0;
                    self.inventory[self.selected] = Some(rod);
                }
            }
        }
        self.swing_arm();
        true
    }
    /// `Mob.mobInteract` of the ECS mount `id` (saddle, feed, steer, tame, seat): P's `EcsLevel::mount_interact`, by the acting player.
    /// The guest check is the host's.
    pub(in crate::game) fn interact_mount_ecs(&mut self, id: u32, item: &str) -> bool {
        if self.ridden_by_guest(id) {
            return false;
        }
        let sneaking = self.player.as_ref().is_some_and(|p| p.shift_key_down);
        let Some(player) = self.acting_player_uuid_id() else {
            return false;
        };
        self.ecs.entity_of(id).is_some_and(|e| self.ecs.mount_interact(e, player, item, sneaking))
    }
    /// Shears on the ECS mount `id`'s saddle: P takes it off (`EcsLevel::shear_saddle`, missing); the drop and the sound stay the host's.
    pub(in crate::game) fn shear_saddle_ecs(&mut self, id: u32, item: &str) -> Option<bool> {
        let e = self.ecs.entity_of(id)?;
        self.ecs.shear_saddle(e, item)
    }
    /// `Start riding jump` on the ECS mount `id`: P's `handleStartJump` gate and sound (`EcsLevel::start_jump`, missing).
    pub(in crate::game) fn start_riding_jump_ecs(&mut self, id: u32, data: i32) -> Option<(&'static str, f32)> {
        let e = self.ecs.entity_of(id)?;
        self.ecs.start_jump(e, data)
    }
    /// Whether the rider's eyes are in water (`Entity.updateFluidOnEyes` then `isEyeInFluid(WATER)`),
    /// and not in a bubble column, as `LivingEntity.baseTick` asks before it lets a rider stay.
    fn rider_eyes_in_water(&self) -> bool {
        let Some([x, y, z]) = self.eye_position() else {
            return false;
        };
        let (bx, by, bz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
        if &*self.res.facts().get(self.world.block(bx, by, bz)).name == "minecraft:bubble_column" {
            return false;
        }
        self.fluid_height_of(bx, by, bz, Some(Fluid::Water))
            .is_some_and(|(height, _)| f64::from(by) + f64::from(height) > y)
    }
    /// The rider's input for this tick; returns false when not on a mount.
    /// `Player.rideTick` for the ECS mount the player rides: the sneak, the dead mount and the water dismounts are P's to
    /// decide (`EcsLevel::ride_tick`, missing); the input and the jump go to P's rider.
    pub(super) fn tick_mount_riding(&mut self, keys: Keys) -> bool {
        let Some(id) = self.riding else {
            return false;
        };
        let Some(e) = self.ecs.entity_of(id) else {
            return false;
        };
        if keys.shift || !self.ecs.alive(id) || self.ecs.dismounts_in_water(e, self.rider_eyes_in_water()) {
            self.dismount_mount();
            return true;
        }
        let strafe = f32::from(u8::from(keys.left)) - f32::from(u8::from(keys.right));
        let forward = f32::from(u8::from(keys.forward)) - f32::from(u8::from(keys.backward));
        // A server's player charges the bar on its own client; its power comes with START_RIDING_JUMP (`start_riding_jump_ecs`).
        let jump = if self.lan_acting.is_some() { 0.0 } else { self.charge_mount_jump_ecs(id, keys) };
        self.ecs.ride_input(e, [strafe, forward], jump, keys.jump, keys.sprint, keys.shift, self.yaw, self.pitch);
        true
    }
    /// `LocalPlayer.aiStep`'s jump charge on the ECS mount `id`: P charges and rears it (`EcsLevel::charge_jump`, missing).
    fn charge_mount_jump_ecs(&mut self, id: u32, keys: Keys) -> f32 {
        let Some(e) = self.ecs.entity_of(id) else {
            return 0.0;
        };
        let jump = self.ecs.charge_jump(e, keys.jump);
        // AbstractHorse.playJumpSound: the sound and volume of the jump capability, None for a mount that does not jump.
        if jump > 0.0
            && let Some((sound, volume)) = self.ecs.jump_sound(e)
        {
            let at = self.ecs.position(id).unwrap_or_default();
            self.audio.play(&format!("minecraft:{sound}"), Some(at), volume, 1.0);
        }
        jump
    }
    /// `positionRider`: the player sits on the mount after it moves.
    pub(super) fn position_rider(&mut self) {
        // P seats the player on the ECS mount (`mount::seats`, mount.rs:397); the host reads the seat back (`EcsLevel::rider_seat`, missing).
        let seat = self.riding.and_then(|id| self.ecs.entity_of(id)).and_then(|e| self.ecs.rider_seat(e));
        if let Some(at) = seat {
            self.pose_player_on(Vec3::new(at[0], at[1], at[2]));
        }
    }
    /// The player's seat `at` on its mount: a teleport the tick it takes it, then interpolated as the mount moves.
    fn pose_player_on(&mut self, at: Vec3) {
        let first = self.posed_rider != self.riding;
        self.posed_rider = self.riding;
        if let Some(p) = self.player.as_mut() {
            self.previous_position = if first { at } else { p.position };
            p.position = at;
            p.velocity = Vec3::ZERO;
            p.fall_distance = 0.0;
        }
    }
    /// `removePassenger` + `getDismountLocationForPassenger`: where the mount
    /// lets the rider out.
    pub(super) fn dismount_mount(&mut self) {
        let Some(id) = self.riding else {
            return;
        };
        if !self.native_may_leave(id) {
            return;
        }
        self.riding = None;
        self.dismount_mount_ecs(id);
    }
    /// Where `mount` lets a player in `p` facing `yaw` off, and the pose they take there.
    fn mount_player_dismount(
        &self,
        mount: &Mount,
        yaw: f32,
        p: &PlayerMotion,
    ) -> (Vec3, Option<Pose>) {
        let rider = Rider {
            yaw,
            width: p.dimensions.width,
            poses: rider_poses(p),
            main_arm_left: false,
        };
        dismount_location(mount, &rider, &GameDismountWorld(self))
    }

    /// The same player search when the mount is lent out during a mob tick.
    pub(super) fn player_dismount_from_lent_mob(
        &self,
        mount: &Mob,
        yaw: f32,
        player: &PlayerMotion,
    ) -> Vec3 {
        if mount.species() == Species::HappyGhast {
            on_top(mount)
        } else {
            self.mount_player_dismount(&Mount::of(mount), yaw, player).0
        }
    }
    /// Where `stop_riding` sets a player in `p` facing `yaw` down off `vehicle`: None when
    /// `vehicle` is no boat, cart or mob.
    pub(in crate::game) fn player_dismount_point(
        &self,
        vehicle: u32,
        yaw: f32,
        _p: &PlayerMotion,
    ) -> Option<Vec3> {
        if self.ecs.boat_status(vehicle).is_some() {
            return Some(self.boat_player_dismount_target(vehicle, yaw));
        }
        if self.ecs.cart_kind(vehicle).is_some() {
            return Some(self.cart_player_dismount_target(vehicle));
        }
        // `getDismountLocationForPassenger` of the ECS mount, P's (`EcsLevel::dismount_point`), read in the mob world.
        let e = self.ecs.entity_of(vehicle)?;
        let mw = self.mob_world_base(&[]);
        let at = self.ecs.dismount_point(&mw, e)?;
        Some(Vec3::new(at[0], at[1], at[2]))
    }
    pub(super) fn mount_event(&mut self, e: &MobEvent) {
        match *e {
            MobEvent::Tamed { mob } => {
                // tameWithName(player): the rider, a server's player in its turn.
                let rider = self.lan.as_ref().and_then(|h| {
                    h.hub
                        .guests
                        .iter()
                        .find(|g| g.vehicle == Some(mob))
                        .map(|g| g.conn)
                });
                if let Some(conn) = rider {
                    self.as_guest(conn, |g| g.tame_mount_ecs(mob));
                } else {
                    self.tame_mount_ecs(mob);
                }
            }
            MobEvent::Buck { mob } => {
                if self.riding == Some(mob) {
                    self.dismount_mount();
                } else {
                    // ejectPassengers: a server's player gets off too
                    // (leave_guest tells their client).
                    let rider = self.lan.as_ref().and_then(|h| {
                        h.hub
                            .guests
                            .iter()
                            .find(|g| g.vehicle == Some(mob))
                            .map(|g| g.conn)
                    });
                    if let Some(conn) = rider {
                        self.as_guest(conn, |g| g.dismount_mount());
                    }
                }
                // RunAroundLikeCrazyGoal.tick: entity event 6, smoke.
                self.mob_event_particles(mob, 6);
            }
            // `Entity.ejectPassengers`: P's `eject_passengers` (mount.rs:441) lets the ECS riders off; the mount's `from` is not passed yet (listed).
            MobEvent::EjectPassengers { mob, .. } => {
                if self.riding == Some(mob) {
                    self.stop_riding();
                }
                if let Some(e) = self.ecs.entity_of(mob) {
                    self.ecs.eject_passengers(e);
                }
            }
            // `LivingEntity.stabAttack`: `target.stopRiding()`.
            MobEvent::StabDismount { target } => match target {
                Who::Player => self.stop_riding(),
                Who::Mob(id) => self.dismount_mob(id),
            },
            _ => {}
        }
    }
    /// `Entity.propagateFallToPassengers`: whoever rides the mob `mob` takes
    /// `causeFallDamage` of its fall, as the mount's own damage does. The local
    /// player, the server players and the mob on its back each get it, with
    /// their own safe distance and armour.
    pub(super) fn fall_to_passengers(&mut self, mob: u32, distance: f64, modifier: f32) {
        if self.riding == Some(mob) {
            self.rider_fall_damage(distance, modifier);
        }
        let guests: Vec<u32> = self.lan.as_ref().map_or_else(Vec::new, |h| {
            h.hub
                .guests
                .iter()
                .filter(|g| g.vehicle == Some(mob) && Some(g.conn) != self.lan_acting)
                .map(|g| g.conn)
                .collect()
        });
        for conn in guests {
            self.guest_hit_by(conn, |g| g.rider_fall_damage(distance, modifier));
        }
        // The ECS riders take the fall through P (`EcsLevel::fall_riders`, missing): their `causeFallDamage` and its events.
        if let Some(e) = self.ecs.entity_of(mob) {
            self.ecs.fall_riders(e, distance, modifier);
        }
    }
    /// The player this game runs (the local one, or the server player it acts
    /// as) takes the fall of the mount under it. `cause_mount_fall` runs the
    /// landing code's own fall damage for the player: creative flight is
    /// exempt, and the statistic, the sounds and the damage records wait in the
    /// player's events for its next tick, as a landing of its own leaves them.
    pub(super) fn rider_fall_damage(&mut self, distance: f64, modifier: f32) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        let _ = MotionWorld {
            entities: &[],
            player,
            scene: &mut self.world,
            assets: &self.pack.assets,
            collision_binding: self.collision,
            resources: self.motion,
            health: &self.health_resources,
            loop_context: &self.loop_context,
            damage_hooks: &self.damage_hooks,
            tag_items: Some(world_items!(self)),
            block_registries: Some(&self.pack.block_registries),
        }
        .cause_mount_fall(distance, modifier);
    }
}

/// The mount a rider gets off: its kind, where it stands, its box,
/// `getBbWidth` and body yaw.
struct Mount {
    species: Species,
    position: Vec3,
    bounds: Aabb,
    width: f32,
    yaw: f32,
}
impl Mount {
    fn of(m: &Mob) -> Self {
        Self {
            species: m.species(),
            position: m.position,
            bounds: m.bounds(),
            width: m.width(),
            yaw: m.yaw,
        }
    }
}
/// On top of `m`, where it lets off a rider it has no other rule for.
pub(super) fn on_top(m: &Mob) -> Vec3 {
    Vec3::new(
        m.position.x,
        m.position.y + f64::from(m.height()),
        m.position.z,
    )
}
/// The rider getting off: `getYRot`, `getBbWidth`, and
/// `getLocalBoundsForPose` for each of `Player.getDismountPoses`.
struct Rider {
    yaw: f32,
    width: f32,
    poses: Vec<(Pose, Dimensions)>,
    main_arm_left: bool,
}
/// `Player.getDismountPoses`: standing, crouching, then crawling.
const DISMOUNT_POSES: [Pose; 3] = [Pose::Standing, Pose::Crouching, Pose::Swimming];
/// The rider's box in each dismount pose, at their `minecraft:scale`.
fn rider_poses(p: &PlayerMotion) -> Vec<(Pose, Dimensions)> {
    // EntityDimensions.scale: every player pose scales alike, so the
    // standing width gives the scale without a mutable attribute lookup.
    let scale = p.dimensions.width / Pose::Standing.dimensions(1.0).width;
    DISMOUNT_POSES
        .iter()
        .map(|&pose| (pose, pose.dimensions(scale)))
        .collect()
}
/// The level queries a dismount makes.
trait DismountWorld {
    /// `getFluidState(pos).is(FluidTags.LAVA)`.
    fn lava(&self, p: [i32; 3]) -> bool;
    /// `BlockGetter.getBlockFloorHeight(pos)`.
    fn floor_height(&self, p: [i32; 3]) -> f64;
    /// `DismountHelper.canDismountTo(level, passenger, box)`.
    fn can_dismount_to(&self, bounds: Aabb) -> bool;
}
struct GameDismountWorld<'a, 'g>(&'a Game<'g>);
impl DismountWorld for GameDismountWorld<'_, '_> {
    fn lava(&self, p: [i32; 3]) -> bool {
        let g = self.0;
        g.res.facts().get(g.world.block(p[0], p[1], p[2])).fluid == Fluid::Lava
    }
    fn floor_height(&self, p: [i32; 3]) -> f64 {
        let shape = self.0.block_collision_shape(p);
        let below = std::cell::OnceCell::<VoxelShape>::new();
        horizon_core::geometry::block_floor_height(&shape, || {
            below.get_or_init(|| self.0.block_collision_shape([p[0], p[1] - 1, p[2]]))
        })
    }
    fn can_dismount_to(&self, bounds: Aabb) -> bool {
        self.0.can_dismount_to(bounds)
    }
}
/// `getDismountLocationForPassenger`, and the pose the rider takes there: each
/// vehicle class's own rule, and `Entity`'s on top of the mount for the rest.
fn dismount_location(
    mount: &Mount,
    rider: &Rider,
    world: &impl DismountWorld,
) -> (Vec3, Option<Pose>) {
    if mount.species == Species::Strider {
        return strider_dismount_location(mount, rider, world);
    }
    // `Camel` is an `AbstractHorse`, so the horse rule comes first.
    if equine::is_equine(mount.species) {
        return horse_dismount_location(mount, rider, world);
    }
    if horizon_client::mobs::drops::is_animal(mount.species) {
        return animal_dismount_location(mount, rider, world);
    }
    // Entity.getDismountLocationForPassenger: on top of the vehicle's box.
    (
        Vec3::new(
            mount.position.x,
            mount.bounds.max_position().y,
            mount.position.z,
        ),
        None,
    )
}
/// `AbstractHorse.getDismountLocationForPassenger`: out of the horse's box to
/// the main hand's side first, then the other, then its own position.
fn horse_dismount_location(
    mount: &Mount,
    rider: &Rider,
    world: &impl DismountWorld,
) -> (Vec3, Option<Pose>) {
    // HumanoidArm: the main hand is the right one unless the player set it.
    let main = if rider.main_arm_left { -90.0 } else { 90.0 };
    for turn in [main, -main] {
        let direction = escape_vector(
            f64::from(mount.width),
            f64::from(rider.width),
            mount.yaw + turn,
        );
        if let Some(found) = horse_side(mount, rider, world, direction) {
            return found;
        }
    }
    (mount.position, None)
}
/// `AbstractHorse.getDismountLocationInDirection`: from the horse's feet up to
/// 0.75 above its box, in each of the rider's dismount poses.
fn horse_side(
    mount: &Mount,
    rider: &Rider,
    world: &impl DismountWorld,
    direction: Vec3,
) -> Option<(Vec3, Option<Pose>)> {
    let (x, z) = (
        mount.position.x + direction.x,
        mount.position.z + direction.z,
    );
    let bottom = mount.bounds.min_position().y.floor() as i32;
    let limit = mount.bounds.max_position().y + 0.75;
    for (pose, dimensions) in &rider.poses {
        let mut y = bottom;
        loop {
            let floor = world.floor_height([x.floor() as i32, y, z.floor() as i32]);
            if f64::from(y) + floor > limit {
                break;
            }
            // DismountHelper.isBlockFloorValid.
            if !floor.is_infinite() && floor < 1.0 {
                let at = Vec3::new(x, f64::from(y) + floor, z);
                if world.can_dismount_to(dimensions.bounds(at)) {
                    return Some((at, Some(*pose)));
                }
            }
            y += 1;
            if f64::from(y) >= limit {
                break;
            }
        }
    }
    None
}
/// `Animal.getDismountLocationForPassenger`: the eight blocks around the
/// mount, its own facing's right side first, at the mount's own height.
fn animal_dismount_location(
    mount: &Mount,
    rider: &Rider,
    world: &impl DismountWorld,
) -> (Vec3, Option<Pose>) {
    // Entity.getMotionDirection: Direction.fromYRot, in doubles.
    let turn = (f64::from(mount.yaw) / 90.0 + 0.5).floor() as i32;
    let (fx, fz) = [(0, 1), (-1, 0), (0, -1), (1, 0)][(turn & 3) as usize];
    // Direction.getClockWise.
    let (rx, rz) = (-fz, fx);
    // DismountHelper.offsetsForDirection.
    let offsets = [
        (rx, rz),
        (-rx, -rz),
        (rx - fx, rz - fz),
        (-rx - fx, -rz - fz),
        (fx + rx, fz + rz),
        (fx - rx, fz - rz),
        (-fx, -fz),
        (fx, fz),
    ];
    let (bx, by, bz) = (
        mount.position.x.floor() as i32,
        mount.position.y.floor() as i32,
        mount.position.z.floor() as i32,
    );
    for (pose, dimensions) in &rider.poses {
        for (dx, dz) in offsets {
            let p = [bx + dx, by, bz + dz];
            let floor = world.floor_height(p);
            if floor.is_infinite() || floor >= 1.0 {
                continue;
            }
            // Vec3.upFromBottomCenterOf.
            let at = Vec3::new(
                f64::from(p[0]) + 0.5,
                f64::from(p[1]) + floor,
                f64::from(p[2]) + 0.5,
            );
            if world.can_dismount_to(dimensions.bounds(at)) {
                return (at, Some(*pose));
            }
        }
    }
    // `super.getDismountLocationForPassenger`.
    (
        Vec3::new(
            mount.position.x,
            mount.bounds.max_position().y,
            mount.position.z,
        ),
        None,
    )
}
/// `Entity.getCollisionHorizontalEscapeVector`: out of the vehicle's box
/// along `yaw`, to the edge of the square it spans.
fn escape_vector(vehicle_width: f64, passenger_width: f64, yaw: f32) -> Vec3 {
    // 1.0E-5F: a float literal widened to double.
    let distance = (vehicle_width + passenger_width + f64::from(1.0e-5_f32)) / 2.0;
    let radians = f64::from(yaw * 0.017_453_292);
    let (sin, cos) = (-trig::sin(radians), trig::cos(radians));
    let max = f64::from(sin.abs().max(cos.abs()));
    Vec3::new(
        f64::from(sin) * distance / max,
        0.0,
        f64::from(cos) * distance / max,
    )
}
/// `Strider.getDismountLocationForPassenger`: the columns along the rider's
/// facing and 22.5 and 45 degrees either side of it, each from the top of
/// the strider's box down to half a block below its feet. The first block
/// with no lava, a valid floor (`DismountHelper.isBlockFloorValid`) and room
/// for one of the rider's dismount poses (`canDismountTo`) wins, with that
/// pose; without one, on top of the strider with the pose unchanged.
fn strider_dismount_location(
    strider: &Mount,
    rider: &Rider,
    world: &impl DismountWorld,
) -> (Vec3, Option<Pose>) {
    let (vehicle, passenger) = (f64::from(strider.width), f64::from(rider.width));
    let top = strider.bounds.max_position().y;
    let bottom = strider.bounds.min_position().y - 0.5;
    // A LinkedHashSet: each block once, in the order first reached.
    let mut blocks: Vec<[i32; 3]> = Vec::new();
    for turn in [0.0, -22.5, 22.5, -45.0, 45.0] {
        let offset = escape_vector(vehicle, passenger, rider.yaw + turn);
        let (x, z) = (
            (strider.position.x + offset.x).floor() as i32,
            (strider.position.z + offset.z).floor() as i32,
        );
        let mut y = top;
        let mut block_y = top.floor() as i32;
        while y > bottom {
            if !blocks.contains(&[x, block_y, z]) {
                blocks.push([x, block_y, z]);
            }
            block_y -= 1;
            y -= 1.0;
        }
    }
    for p in blocks {
        if world.lava(p) {
            continue;
        }
        // DismountHelper.isBlockFloorValid.
        let floor = world.floor_height(p);
        if floor.is_infinite() || floor >= 1.0 {
            continue;
        }
        // Vec3.upFromBottomCenterOf.
        let at = Vec3::new(
            f64::from(p[0]) + 0.5,
            f64::from(p[1]) + floor,
            f64::from(p[2]) + 0.5,
        );
        for (pose, dimensions) in &rider.poses {
            if world.can_dismount_to(dimensions.bounds(at)) {
                return (at, Some(*pose));
            }
        }
    }
    (Vec3::new(strider.position.x, top, strider.position.z), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_client::items::ItemStack;
    use std::collections::HashMap;

    #[test]
    #[ignore = "daily"]
    fn steering_stick_break_converts_to_fishing_rod() {
        use crate::game::ecs::test_support::ecs_game_with;
        use horizon_client::motion::PlayerMotion;
        use horizon_core::geometry::Vec3;
        {
            let (mut game, id) = ecs_game_with("minecraft:pig", [0.5, 64.0, 0.5]);
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            game.riding = Some(id);
            let mut stick = ItemStack::new("minecraft:carrot_on_a_stick", 1);
            let max = game.items().get(&stick.item).unwrap().max_damage;
            stick.data_mut().custom_name = Some(horizon_core::nbt::Tag::String("Old Nag".into()));
            // One point short of breaking: the shared durability path adds wear.
            stick.damage = max - 8;
            let (updated, broken) = game.damage_stack(&stick, 7, true);
            assert!(broken.is_none());
            assert_eq!(updated.unwrap().damage, max - 1);
            stick.damage = max - 7;
            assert!(game.damage_stack(&stick, 0, true).1.is_none());
            let mut undamageable = stick.clone();
            undamageable.data_mut().set_other(
                "!minecraft:max_damage",
                Some(horizon_core::nbt::Tag::Byte(1)),
            );
            assert!(
                game.damage_stack(&undamageable, max as i32, true)
                    .1
                    .is_none()
            );
            game.inventory[game.selected] = Some(stick);
            assert!(game.use_steering_stick("minecraft:carrot_on_a_stick"));
            let rod = game.inventory[game.selected].as_ref().unwrap();
            assert_eq!(&*rod.item, "minecraft:fishing_rod");
            assert_eq!(rod.count, 1);
            assert_eq!(rod.damage, 0, "the rod starts whole");
            assert_eq!(rod.custom_name().as_deref(), Some("Old Nag"));
        }
    }

    #[test]
    fn steering_accepts_the_correct_item_in_either_hand() {
        let carrot = "minecraft:carrot_on_a_stick";
        let fungus = "minecraft:warped_fungus_on_a_stick";
        assert!(steering_item(Species::Pig, Some(carrot), None));
        assert!(steering_item(Species::Pig, None, Some(carrot)));
        assert!(steering_item(Species::Strider, None, Some(fungus)));
        assert!(!steering_item(Species::Pig, Some(fungus), None));
        assert!(!steering_item(Species::Horse, Some(carrot), Some(fungus)));
    }

    /// A lava sea up to y 30 with full blocks placed in it; air above.
    struct Sea {
        blocks: HashMap<[i32; 3], ()>,
    }
    impl Sea {
        fn with(blocks: &[[i32; 3]]) -> Self {
            Self {
                blocks: blocks.iter().map(|&b| (b, ())).collect(),
            }
        }
        fn solid(&self, p: [i32; 3]) -> bool {
            self.blocks.contains_key(&p)
        }
    }
    impl DismountWorld for Sea {
        fn lava(&self, p: [i32; 3]) -> bool {
            p[1] <= 30 && !self.solid(p)
        }
        fn floor_height(&self, p: [i32; 3]) -> f64 {
            if self.solid(p) {
                1.0
            } else if self.solid([p[0], p[1] - 1, p[2]]) {
                0.0
            } else {
                f64::NEG_INFINITY
            }
        }
        fn can_dismount_to(&self, bounds: Aabb) -> bool {
            let (lo, hi) = (bounds.min_position(), bounds.max_position());
            let cells = |a: f64, b: f64| a.floor() as i32..=(b - 1.0e-7).floor() as i32;
            !cells(lo.x, hi.x).any(|x| {
                cells(lo.y, hi.y).any(|y| cells(lo.z, hi.z).any(|z| self.solid([x, y, z])))
            })
        }
    }
    /// The top of a lava source at y 30 (`FlowingFluid.getOwnHeight`, 8/9).
    const SURFACE: f64 = 30.0 + 8.0 / 9.0;
    /// A strider (0.9 by 1.7) standing on the lava at x, z, facing south.
    fn strider_at(x: f64, z: f64) -> Mount {
        Mount {
            species: Species::Strider,
            position: Vec3::new(x, SURFACE, z),
            bounds: Aabb::new(
                Vec3::new(x - 0.45, SURFACE, z - 0.45),
                Vec3::new(x + 0.45, SURFACE + 1.7, z + 0.45),
            ),
            width: 0.9,
            yaw: 0.0,
        }
    }
    fn rider(yaw: f32) -> Rider {
        Rider {
            yaw,
            width: 0.6,
            poses: DISMOUNT_POSES
                .iter()
                .map(|&pose| (pose, pose.dimensions(1.0)))
                .collect(),
            main_arm_left: false,
        }
    }
    fn block_of(at: Vec3) -> [i32; 3] {
        [
            at.x.floor() as i32,
            at.y.floor() as i32,
            at.z.floor() as i32,
        ]
    }

    #[test]
    fn a_strider_in_open_lava_puts_its_rider_on_its_back() {
        // #3513: getting off in a lava sea used to put the player beside the
        // strider, in the lava; with no floor in reach vanilla puts the rider
        // on top of the strider instead.
        let sea = Sea::with(&[]);
        let (at, pose) = dismount_location(&strider_at(0.5, 0.5), &rider(0.0), &sea);
        assert!(!sea.lava(block_of(at)), "dismounted into lava at {at:?}");
        assert_eq!(at, Vec3::new(0.5, SURFACE + 1.7, 0.5));
        assert_eq!(pose, None);
    }

    #[test]
    fn a_strider_by_the_shore_puts_its_rider_on_the_shore() {
        // Netherrack 45 degrees to the right of a rider facing south (+z);
        // the straight-ahead and 22.5 degree columns are lava.
        let sea = Sea::with(&[[-1, 30, 1]]);
        let (at, pose) = dismount_location(&strider_at(0.5, 0.5), &rider(0.0), &sea);
        assert_eq!(at, Vec3::new(-0.5, 31.0, 1.5));
        assert_eq!(pose, Some(Pose::Standing));
    }

    #[test]
    fn a_low_ceiling_on_the_shore_makes_the_rider_crawl_off() {
        let sea = Sea::with(&[[0, 30, 1], [0, 32, 1]]);
        let (at, pose) = dismount_location(&strider_at(0.5, 0.5), &rider(0.0), &sea);
        assert_eq!(at, Vec3::new(0.5, 31.0, 1.5));
        assert_eq!(pose, Some(Pose::Swimming));
    }

    #[test]
    fn the_escape_vector_reaches_the_edge_of_the_square() {
        let edge = (0.9 + 0.6 + f64::from(1.0e-5_f32)) / 2.0;
        let v = escape_vector(0.9, 0.6, 0.0);
        assert!((v.z - edge).abs() < 1.0e-6 && v.x.abs() < 1.0e-6);
        // Facing west of south: the corner of the square.
        let v = escape_vector(0.9, 0.6, 45.0);
        assert!((v.x + edge).abs() < 1.0e-3 && (v.z - edge).abs() < 1.0e-3);
    }
}

#[cfg(test)]
#[path = "mounts/camel_jump_tests.rs"]
mod camel_jump_tests;

#[cfg(test)]
#[path = "tests/ride_dismount_tests.rs"]
mod ride_dismount_tests;

#[cfg(test)]
#[path = "tests/ride_seat_tests.rs"]
mod ride_seat_tests;
