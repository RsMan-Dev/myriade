//! Boats and rafts (`AbstractBoat`): water/land/air status, floating,
//! paddling controls, collisions, riding (`positionRider`, `clampRotation`,
//! dismounting), `BoatItem` placement, `VehicleEntity.hurtServer` breaking,
//! saving and `AbstractBoatRenderer` with animated paddles.
mod below_world;
#[cfg(test)]
mod friction_tests;
#[cfg(test)]
mod lifetime_tests;
mod placement;
pub(super) use placement::OutlineFluidRay;

pub(super) use horizon_client_gameplay::boat_models::BoatModels;
pub(super) use horizon_gameplay::boats::*;

use super::{Game, InteractTarget, entity::Entity, entity_culling::at, view_vector};
use horizon_client::actions::Action;
use horizon_client::entity_rules::EntityRules;
use horizon_client::{
    blocks::Fluid,
    collisions::{EntityCollisions, SceneCollisions},
    items::ItemStack,
    mesher::Vertex,
    mobs::anim::Poses,
    motion::{BubbleColumn, RiddenBoat, bubble_column_at},
};
use horizon_core::{
    geometry::{Aabb, ShapeOp, Vec3, VoxelShape, trig},
    position::BlockPos,
    random::RandomSource,
};
use horizon_world::collision_context::CollisionContext;
use horizon_ecs_proto::{boat_hurt::{HitResult, UseResult}, targeting::Who};
use std::{f32::consts::PI, sync::Arc};

/// The push the paddles give along the boat's yaw (`tick_boat`'s
/// controlBoat): the sine and cosine of the yaw in float radians from `Mth`'s
/// sine table, times the acceleration as floats, widened to double.
fn paddle_push(yaw: f32, acceleration: f32) -> (f64, f64) {
    (
        f64::from(trig::sin(f64::from(-yaw * trig::DEG_TO_RAD)) * acceleration),
        f64::from(trig::cos(f64::from(yaw * trig::DEG_TO_RAD)) * acceleration),
    )
}
impl Game<'_> {
    fn fluid_height(&self, x: i32, y: i32, z: i32) -> Option<(f32, bool)> {
        self.fluid_height_of(x, y, z, Some(Fluid::Water))
    }
    /// The horizontal water plane around the camera for the water mirror: the median
    /// standing-water surface of a grid of columns 48 blocks around, each scanned down from
    /// the camera to its ground. A lake or the sea has one plane; the mirror shows only the
    /// water on it. With no water in reach the last plane found stays, so the mirror never
    /// switches off as the camera moves.
    pub(super) fn water_plane(&self, camera: glam::DVec3) -> Option<f32> {
        const RADIUS: i32 = 48;
        const STEP: usize = 4;
        const DEPTH: i32 = 96;
        let (cx, cz) = (camera.x.floor() as i32, camera.z.floor() as i32);
        let top = camera.y.floor() as i32 + 2;
        // Scanned again once the camera moves 8 blocks, or every 5 seconds: a scan reads up
        // to 60 000 blocks on the game thread, and the plane rarely changes.
        let scan = (
            [cx >> 3, top >> 3, cz >> 3],
            self.world.game_time.div_euclid(100),
        );
        if self.water_plane_scan.replace(Some(scan)) == Some(scan) {
            return self.water_plane_memo.get();
        }
        let mut heights = Vec::with_capacity(625);
        for x in (cx - RADIUS..=cx + RADIUS).step_by(STEP) {
            for z in (cz - RADIUS..=cz + RADIUS).step_by(STEP) {
                // The top of the column's water: the highest standing water under the
                // camera (a full block's water continues above it).
                for y in (top - DEPTH..=top).rev() {
                    if let Some((h, source)) = self.fluid_height(x, y, z) {
                        if source && h < 1.0 {
                            heights.push(y as f32 + h);
                        }
                        break;
                    }
                    if self.res.facts().get(self.world.block(x, y, z)).solid_render {
                        break;
                    }
                }
            }
        }
        if heights.is_empty() {
            return self.water_plane_memo.get();
        }
        heights.sort_by(f32::total_cmp);
        let found = heights[heights.len() / 2];
        // The plane in use stays while a quarter of the columns are still on it, so two
        // lakes at different heights do not make the mirror flicker between them.
        let plane = match self.water_plane_memo.get() {
            Some(last)
                if 4 * heights.iter().filter(|h| (*h - last).abs() < 0.05).count()
                    >= heights.len() =>
            {
                last
            }
            _ => found,
        };
        self.water_plane_memo.set(Some(plane));
        Some(plane)
    }
    /// `FluidState.getHeight` and whether it is a source, for `fluid` or,
    /// with `None`, any fluid (`ClipContext.Fluid.ANY`).
    pub(super) fn fluid_height_of(
        &self,
        x: i32,
        y: i32,
        z: i32,
        fluid: Option<Fluid>,
    ) -> Option<(f32, bool)> {
        let f = self.res.facts().get(self.world.block(x, y, z));
        if f.fluid == Fluid::None || fluid.is_some_and(|want| f.fluid != want) {
            return None;
        }
        let above = self
            .res
            .facts()
            .get(self.world.block(x, y.wrapping_add(1), z));
        // FluidState.getHeight: 1 under the same fluid, else amount / 9.
        let height = if above.fluid == f.fluid {
            1.0
        } else {
            f32::from(f.fluid_level) / 9.0
        };
        Some((height, f.fluid_level == 8 && !f.fluid_falling))
    }
    /// `getWaterLevelAbove`: the top of the water column the hull is in.
    /// A float, as vanilla's: the block's y plus its water height in float.
    fn water_level_above(&self, b: &Boat) -> f32 {
        let bb = b.bounds();
        let (lo, hi) = (bb.min_position(), bb.max_position());
        let (y0, y1) = (hi.y.floor() as i32, hi.y.ceil() as i32);
        'rows: for y in y0..y1 {
            let mut height = 0.0f32;
            for x in lo.x.floor() as i32..hi.x.ceil() as i32 {
                for z in lo.z.floor() as i32..hi.z.ceil() as i32 {
                    if let Some((h, _)) = self.fluid_height(x, y, z) {
                        height = height.max(h);
                    }
                    if height >= 1.0 {
                        continue 'rows;
                    }
                }
            }
            if height < 1.0 {
                return y as f32 + height;
            }
        }
        (i64::from(y1) + 1) as f32
    }
    fn boat_status(&self, b: &mut Boat) -> Status {
        let bb = b.bounds();
        let (lo, hi) = (bb.min_position(), bb.max_position());
        // isUnderwater
        let max_y = hi.y + 0.001;
        let mut under = false;
        for x in lo.x.floor() as i32..hi.x.ceil() as i32 {
            for y in hi.y.floor() as i32..max_y.ceil() as i32 {
                for z in lo.z.floor() as i32..hi.z.ceil() as i32 {
                    if let Some((h, source)) = self.fluid_height(x, y, z)
                        && max_y < f64::from(y as f32 + h)
                    {
                        if !source {
                            b.water_level = hi.y;
                            return Status::UnderFlowingWater;
                        }
                        under = true;
                    }
                }
            }
        }
        if under {
            b.water_level = hi.y;
            return Status::UnderWater;
        }
        // checkInWater
        let mut in_water = false;
        b.water_level = -f64::MAX;
        for x in lo.x.floor() as i32..hi.x.ceil() as i32 {
            for y in lo.y.floor() as i32..(lo.y + 0.001).ceil() as i32 {
                for z in lo.z.floor() as i32..hi.z.ceil() as i32 {
                    if let Some((h, _)) = self.fluid_height(x, y, z) {
                        // `float height = y + getHeight`: summed in float.
                        let level = f64::from(y as f32 + h);
                        b.water_level = b.water_level.max(level);
                        in_water |= lo.y < level;
                    }
                }
            }
        }
        if in_water {
            return Status::InWater;
        }

        let friction = self.boat_ground_friction(b, bb);
        if friction > 0.0 {
            b.land_friction = friction;
            Status::OnLand
        } else {
            Status::InAir
        }
    }
    /// `AbstractBoat.getGroundFriction`: average friction of the blocks whose
    /// collision shape meets a thin slab just under the hull (lily pads skipped).
    fn boat_ground_friction(&self, b: &Boat, bb: Aabb) -> f32 {
        let (lo, hi) = (bb.min_position(), bb.max_position());
        let ground = Aabb::new(
            Vec3::new(lo.x, lo.y - 0.001, lo.z),
            Vec3::new(hi.x, lo.y, hi.z),
        );
        let (g_lo, g_hi) = (ground.min_position(), ground.max_position());
        let (x0, x1, y0, y1, z0, z1) = (
            (g_lo.x.floor() - 1.0) as i32,
            (g_hi.x.ceil() + 1.0) as i32,
            (g_lo.y.floor() - 1.0) as i32,
            (g_hi.y.ceil() + 1.0) as i32,
            (g_lo.z.floor() - 1.0) as i32,
            (g_hi.z.ceil() + 1.0) as i32,
        );
        let slab = VoxelShape::from_box(ground).unwrap_or_else(|_| VoxelShape::empty());
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        let (mut friction, mut count) = (0.0f32, 0);
        for x in x0..x1 {
            for z in z0..z1 {
                let edges = i32::from(x == x0 || x == x1 - 1) + i32::from(z == z0 || z == z1 - 1);
                if edges == 2 {
                    continue;
                }
                for y in y0..y1 {
                    if edges == 1 && (y == y0 || y == y1 - 1) {
                        continue;
                    }
                    let state = self.world.block(x, y, z);
                    let f = self.res.facts().get(state);
                    if f.name.as_ref() == "minecraft:air" || f.has_class("LilyPadBlock") {
                        continue;
                    }
                    let Ok(shape) = self.collision.collision_shape_with_context(
                        &mut scene,
                        state,
                        BlockPos::new(x, y, z),
                        &CollisionContext::empty(false),
                    ) else {
                        continue;
                    };
                    let world_shape =
                        shape.moved(Vec3::new(f64::from(x), f64::from(y), f64::from(z)));
                    if world_shape
                        .join_is_not_empty(&slab, ShapeOp::AND)
                        .unwrap_or(false)
                    {
                        friction +=
                            self.boat_friction(b, BlockPos::new(x, y, z), state, f.friction);
                        count += 1;
                    }
                }
            }
        }
        if count > 0 {
            friction / count as f32
        } else {
            0.0
        }
    }
    /// `Entity.move`'s fall reset for an entity that keeps a fall and moves on
    /// by `moved` from `from`: a move of a block or more casts up to 8 blocks
    /// of it (`ClipContext.Block.FALLDAMAGE_RESETTING`, `ClipContext.Fluid.WATER`)
    /// and a hit resets the fall, before the move's own drop counts.
    pub(super) fn vehicle_fall_reset(&self, fall: f64, from: Vec3, moved: Vec3) -> f64 {
        let length = moved.length_squared();
        if fall == 0.0 || length < 1.0 {
            return fall;
        }
        let reset = &self.res.motion().fall_reset;
        let fluids = &self.res.motion().fluids;
        let to = from.add_vector(moved.normalize().scale(length.sqrt().min(8.0)));
        let hit = horizon_core::geometry::traverse_blocks(from, to, 1_000_000, |p| {
            // A tagged block clips as a full block, not its collider.
            let name = &self.res.facts().get(self.world.block(p.x, p.y, p.z)).name;
            if reset.contains(&**name) && VoxelShape::block().clip(from, to, p).is_some() {
                return Some(());
            }
            // The water's shape, through the shared fluid shape cache.
            horizon_client::entities::water_clip(
                &self.loop_context,
                &self.world,
                fluids,
                from,
                to,
                p,
            )
            .then_some(())
        });
        if matches!(hit, Ok(Some(()))) {
            0.0
        } else {
            fall
        }
    }
    /// `AbstractBoat.checkFallDamage` after a move that went `moved_y` (with
    /// `b` already where it landed): on the ground the fall resets; a move
    /// down over anything but water (the block under its `blockPosition`)
    /// adds to it.
    pub(super) fn boat_fall(&self, b: &Boat, moved_y: f64) -> f64 {
        self.boat_fall_at(b.position, b.on_ground, b.fall_distance, moved_y)
    }
    /// `boat_fall` on the plain numbers, for a boat the ECS holds.
    pub(in crate::game) fn boat_fall_at(&self, position: Vec3, on_ground: bool, fall_distance: f64, moved_y: f64) -> f64 {
        if on_ground {
            return 0.0;
        }
        if moved_y < 0.0 {
            let below = self.res.facts().get(self.world.block(
                position.x.floor() as i32,
                (position.y.floor() as i32).wrapping_sub(1),
                position.z.floor() as i32,
            ));
            if below.fluid != Fluid::Water {
                return fall_distance - f64::from(moved_y as f32);
            }
        }
        fall_distance
    }
    /// `AbstractBoat.tick` for one boat (`controlled` when the player rides it).
    pub(super) fn tick_boat(&mut self, mut b: Boat, controlled: bool) -> Boat {
        b.previous = b.position;
        b.previous_yaw = b.yaw;
        // The lead of a boat ticks on the ECS (a server's boats live there).
        let had_status = std::mem::replace(&mut b.status_known, true);
        b.old_status = b.status;
        b.status = self.boat_status(&mut b);
        b.tick_submersion();
        // The ECS wears it off for the boats it ticks, before they burn.
        if controlled {
            b.wear_off_hurt();
        }
        // A remote client gets the controller from the passenger packet, even when
        // this window occupies the second seat. Local worlds derive it here.
        if self.remote.is_none() {
            b.paddle_controller = controlled;
        }
        if !controlled {
            b.paddling = [false; 2];
            b.input = [false; 4];
        }
        // `Entity.baseTick`: the boat is immersed and pushed by the current it
        // sits in (`isPushedByFluid`), before `floatBoat` scales the velocity.
        // updateFluidInteraction: in water the fall resets.
        if b.push_by_fluid(&self.world, &self.res.motion().fluids, &self.loop_context)
            .0
            > 0.0
        {
            b.fall_distance = 0.0;
        }
        // floatBoat
        // `-getGravity()`: none with NoGravity.
        let mut vspeed = 0.0 - b.get_gravity();
        let mut buoyancy = 0.0;
        let inv_friction: f32;
        let obstacles = self.boat_move_obstacles(&b);
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        if had_status
            && b.old_status == Status::InAir
            && b.status != Status::InAir
            && b.status != Status::OnLand
        {
            // getY(1.0): the top of its box.
            b.water_level = b.position.y + HEIGHT;
            // getWaterLevelAbove() - getBbHeight(): a float subtraction.
            let target = f64::from(self.water_level_above(&b) - HEIGHT as f32) + 0.101;
            let moved = Aabb::new(
                Vec3::new(
                    b.bounds().min_position().x,
                    target,
                    b.bounds().min_position().z,
                ),
                Vec3::new(
                    b.bounds().max_position().x,
                    target + HEIGHT,
                    b.bounds().max_position().z,
                ),
            );
            if scene.unobstructed(moved).unwrap_or(false) {
                b.position.y = target;
                b.velocity.y = 0.0;
            }
            b.status = Status::InWater;
        } else {
            match b.status {
                Status::InWater => {
                    buoyancy = (b.water_level - b.position.y) / HEIGHT;
                    inv_friction = 0.9;
                }
                Status::UnderFlowingWater => {
                    vspeed = -7.0e-4;
                    inv_friction = 0.9;
                }
                Status::UnderWater => {
                    buoyancy = f64::from(0.01_f32);
                    inv_friction = 0.45;
                }
                Status::InAir => inv_friction = 0.9,
                Status::OnLand => {
                    inv_friction = b.land_friction;
                    if controlled {
                        b.land_friction /= 2.0;
                    }
                }
            }
            let f = f64::from(inv_friction);
            b.velocity = Vec3::new(b.velocity.x * f, b.velocity.y + vspeed, b.velocity.z * f);
            b.delta_rotation *= inv_friction;
            if buoyancy > 0.0 {
                b.velocity.y = (b.velocity.y + buoyancy * (0.04 / 0.65)) * 0.75;
            }
        }
        // controlBoat
        if controlled {
            let [left, right, up, down] = b.input;
            let mut acceleration = 0.0f32;
            if left {
                b.delta_rotation -= 1.0;
            }
            if right {
                b.delta_rotation += 1.0;
            }
            if right != left && !up && !down {
                acceleration += 0.005;
            }
            b.yaw += b.delta_rotation;
            if up {
                acceleration += 0.04;
            }
            if down {
                acceleration -= 0.005;
            }
            let (x, z) = paddle_push(b.yaw, acceleration);
            b.velocity.x += x;
            b.velocity.z += z;
            b.paddling = [right && !left || up, left && !right || up];
        }
        // Entity.move: collisions zero the blocked components, the world border's
        // among them (`CollisionGetter.borderCollision`). The boat's own move
        // is a self move (arismoko/horizon#2414: vanilla's `AbstractBoat.tick` passes SELF);
        // PR 1a records the movement unchanged.
        let mut scene = EntityCollisions {
            blocks: scene,
            entities: &obstacles,
            border: Some((self.border.geometry(), [b.position.x, b.position.z])),
        };
        let movement = b.take_movement();
        let mut gate = horizon_client::move_gate::armed_with(
            &self.loop_context,
            b.id,
            || std::borrow::Cow::Owned(b.entity_type()),
            horizon_world::entity_move::MoverType::SelfMovement,
            movement,
        );
        let movement = gate.delta();
        if let Ok(moved) = horizon_client::move_gate::entities_resolve(
            &mut gate,
            &mut scene,
            b.bounds(),
            movement,
            0.0,
            b.on_ground,
        ) {
            b.fall_distance = self.vehicle_fall_reset(b.fall_distance, b.position, moved);
            b.position = Vec3::new(
                b.position.x + moved.x,
                b.position.y + moved.y,
                b.position.z + moved.z,
            );
            b.on_ground = movement.y < 0.0 && moved.y > movement.y;
            b.fall_distance = self.boat_fall(&b, moved.y);
            if (moved.x - movement.x).abs() > 1.0e-7 {
                b.velocity.x = 0.0;
            }
            if (moved.z - movement.z).abs() > 1.0e-7 {
                b.velocity.z = 0.0;
            }
            if (moved.y - movement.y).abs() > 1.0e-7 {
                b.velocity.y = 0.0;
            }
        }
        // Entity.move's last step: soul sand and honey slow the boat too.
        let factor = crate::motion::block_speed_factor_at(
            &self.world,
            &self.motion.blocks,
            &self.loop_context,
            b.position,
            None,
        );
        b.velocity = horizon_world::entity_move::scale_by_block_speed(b.velocity, factor);
        Self::tick_boat_paddles(&mut b, &mut self.audio);
        b
    }
    /// `AbstractBoat.tick` advances the oars even when this client does not move the hull.
    pub(super) fn tick_boat_paddles(b: &mut Boat, audio: &mut super::Audio) {
        // Paddle sounds at the top of each stroke.
        for i in 0..2 {
            if b.paddle_state(i) {
                let p = b.paddles[i] % (2.0 * PI);
                if p <= PI / 4.0 && (b.paddles[i] + PI / 8.0) % (2.0 * PI) >= PI / 4.0 {
                    let sound = match b.status {
                        Status::InWater | Status::UnderWater | Status::UnderFlowingWater => {
                            Some("minecraft:entity.boat.paddle_water")
                        }
                        Status::OnLand => Some("minecraft:entity.boat.paddle_land"),
                        Status::InAir => None,
                    };
                    if let Some(sound) = sound {
                        let v = view_vector(b.yaw, 0.0);
                        let (dx, dz) = if i == 1 { (-v[2], v[0]) } else { (v[2], -v[0]) };
                        let pitch = 0.8 + 0.4 * b.random.next_float();
                        audio.play(
                            sound,
                            Some([b.position.x + dx, b.position.y, b.position.z + dz]),
                            1.0,
                            pitch,
                        );
                    }
                }
                b.paddles[i] += PI / 8.0;
            } else {
                b.paddles[i] = 0.0;
            }
        }
    }
    /// Refuse a removed tick target before carrying its slot into the clone.
    pub(super) fn store_boat_back(&mut self, index: usize, boat: Boat) -> bool {
        if self
            .persist
            .removals
            .contains(&super::commands::Target::Boat(boat.id))
        {
            return false;
        }
        self.boats.store_back(index, boat)
    }
    /// `AbstractBoat.tick`: `status = getStatus()` for a boat this window
    /// does not tick (`tick_boat` sets it for the others): a client's boats
    /// other than the one it drives, and a server's boats its players drive.
    pub(super) fn refresh_boat_status(&mut self, i: usize) {
        let b = &self.boats[i];
        let mut probe = Boat::new(b.id, b.wood.clone(), b.position, b.yaw);
        let status = self.boat_status(&mut probe);
        self.boats.update(i, |b| b.status = status);
        // `oldStatus = status` runs on every boat, so the next tick has one.
        self.boats.update(i, |b| b.status_known = true);
        self.boats.update(i, Boat::tick_submersion);
    }
    /// `BubbleColumnBlock.entityInside`: the bubble columns boat `i` touches
    /// (its box deflated by 1e-5 as `Entity.checkInsideBlocks` does), in x, y,
    /// z order.
    pub(super) fn boat_bubble_columns(&self, i: usize) -> Vec<BubbleColumn> {
        let b = self.boats[i].bounds().deflate(Vec3::new(
            f64::from(1.0e-5f32),
            f64::from(1.0e-5f32),
            f64::from(1.0e-5f32),
        ));
        let (lo, hi) = (b.min_position(), b.max_position());
        let mut columns = Vec::new();
        // Entity.checkInsideBlocks floors both ends of the deflated box.
        // Narrowing ceil(max) before subtracting one wraps at i32::MIN,
        // turning a tiny box into a scan of the entire integer Y domain.
        for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
            for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
                for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                    if let Some(column) = bubble_column_at(
                        &self.world,
                        self.pack.assets.registry,
                        self.res.facts(),
                        self.collision,
                        BlockPos::new(x, y, z),
                    ) {
                        columns.push(column);
                    }
                }
            }
        }
        columns
    }
    /// `Entity.handleOnInsideBubbleColumn`: a column under more water lifts
    /// boat `i` up to 0.7 or drags it down to -0.3 (`AbstractBoat` does not
    /// override `onInsideBubbleColumn`), applied by the column's own push below
    /// the surface.
    pub(super) fn boat_inside_bubble_column(&mut self, i: usize, column: BubbleColumn) {
        // Client display only (the server's boats live in the ECS): a client moves only the boat its player drives.
        if !self.controls_boat(self.boats[i].id) {
            return;
        }
        self.boats.update(i, |b| column.push(&mut b.velocity));
    }
    /// `AbstractBoat.onAboveBubbleColumn`: whether the splash comes up, one
    /// time in a hundred for a boat that is not under water.
    pub(super) fn boat_splash_roll(&mut self, i: usize) -> bool {
        !self.boats[i].under_water() && self.world.random.next_int_bound(100).unwrap_or(1) == 0
    }
    /// `AbstractBoat.onAboveBubbleColumn`: the client's half after the roll,
    /// the splash sound and particle at the boat.
    pub(super) fn boat_bubble_splash(&mut self, i: usize) {
        use horizon_client::particles::{ParticleOptions, ParticleType};
        let p = self.boats[i].position;
        let pitch = 0.8 + 0.4 * self.world.random.next_float();
        self.audio.play(
            "minecraft:entity.generic.splash",
            Some([p.x, p.y, p.z]),
            1.0,
            pitch,
        );
        let x = p.x + f64::from(self.world.random.next_float());
        let z = p.z + f64::from(self.world.random.next_float());
        self.particles.add(
            &ParticleOptions::simple(ParticleType::Splash),
            [x, p.y + 0.7, z],
            [0.0; 3],
        );
    }
    /// `AbstractBoat.onAboveBubbleColumn`: on the server (this window
    /// simulates the boat) the column is remembered, the countdown starts and
    /// the splash's game event goes to the listeners (its sound and particle
    /// are the client's).
    fn boat_above_bubble_column(&mut self, i: usize, drag_down: bool) {
        self.boats.update(i, |b| {
            b.above_bubble_column = true;
            b.bubble_down = drag_down;
            if b.bubble_time == 0 {
                b.bubble_time = BUBBLE_TIME;
            }
        });
        if !self.boat_splash_roll(i) {
            return;
        }
        // A window that is its own client hears and sees the splash too.
        if self.options.execution_role.is_client() {
            self.boat_bubble_splash(i);
        }
        let (id, p) = (self.boats[i].id, self.boats[i].position);
        // Entity.gameEvent(SPLASH, getControllingPassenger()): the window's
        // own player when it drives the boat.
        let ctx = if self.riding == Some(id) && self.player.is_some() {
            self.player_context(None)
        } else {
            self.who_context(None, None)
        };
        self.post_game_event("minecraft:splash", [p.x, p.y, p.z], ctx);
    }
    /// `AbstractBoat.handleBubbleColumnEffect`: an up column throws the boat
    /// up at 0.6, a down column pulls it down by 0.7.
    pub(super) fn handle_bubble_column_effect(&mut self, i: usize, drag_down: bool) {
        if !self.controls_boat(self.boats[i].id) {
            return;
        }
        self.boats.update(i, |b| {
            if drag_down {
                // movement.add(0.0, -0.7, 0.0)
                b.velocity.y += -0.7;
            } else {
                b.velocity.y = 0.6;
            }
        });
    }
    /// `Entity.lavaHurt`'s sound once the hurt went through.
    pub(in crate::game) fn lava_hurt_sound(&mut self, at: [f64; 3]) {
        let pitch = 2.0 + self.world.random.next_float() * 0.4;
        self.audio
            .play("minecraft:entity.generic.burn", Some(at), 0.4, pitch);
    }
    /// Whether a vehicle's box, deflated by 1e-5, reaches into lava below its
    /// surface (`Entity.collidedWithFluid`).
    pub(super) fn in_lava(&self, bb: Aabb) -> bool {
        let (lo, hi) = (bb.min_position(), bb.max_position());
        let e = 1.0e-5;
        for x in (lo.x + e).floor() as i32..=(hi.x - e).floor() as i32 {
            for y in (lo.y + e).floor() as i32..=(hi.y - e).floor() as i32 {
                for z in (lo.z + e).floor() as i32..=(hi.z - e).floor() as i32 {
                    if let Some((height, _)) = self.fluid_height_of(x, y, z, Some(Fluid::Lava))
                        && lo.y + e < f64::from(y) + f64::from(height)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }
    /// Where the boat at `i` lets a player facing `yaw` off.
    pub(super) fn boat_player_dismount_target(&self, id: u32, yaw: f32) -> Vec3 {
        let Some(v) = self.ecs_view(id) else {
            return Vec3::ZERO;
        };
        let at = Vec3::new(v.position[0], v.position[1], v.position[2]);
        self.boat_dismount_target(&Boat::new(id, Arc::from(""), at, v.yaw), 0.6, yaw)
    }
    /// `getDismountLocationForPassenger` for the player riding the ECS boat `id`: the rider leaves its seat and
    /// stands in front of it or on top.
    pub(super) fn dismount_boat(&mut self, id: u32) {
        if self.riding == Some(id) && !self.native_may_leave(id) {
            return;
        }
        self.riding = None;
        self.ecs.boat_release(id);
        let Some(at) = self.ecs_view(id).map(|v| Vec3::new(v.position[0], v.position[1], v.position[2])) else {
            return;
        };
        if self.dismount_in_portal(at) {
            return;
        }
        let target = self.boat_player_dismount_target(id, self.yaw);
        if let Some(p) = self.player.as_mut() {
            p.position = target;
            p.velocity = Vec3::ZERO;
        }
    }
    /// The ECS spawns a boat of `wood` and links it to a network id (`BoatItem.use` and the dispenser share it).
    pub(super) fn spawn_boat_ecs(&mut self, wood: &str, chest: bool, at: [f64; 3], yaw: f32) -> Option<u32> {
        let mut next = self.next_entity;
        let id = self.with_ecs(|ecs, mw| {
            ecs.spawn_boat_id(mw, wood, chest, at, yaw, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        id
    }
    /// `getDismountLocationForPassenger` for a passenger `width` wide
    /// facing `yaw`.
    pub(super) fn boat_dismount_target(&self, b: &Boat, width: f64, yaw: f32) -> Vec3 {
        let r = yaw.to_radians();
        let distance = (WIDTH * std::f64::consts::SQRT_2 + width + 1.0e-5) / 2.0;
        let (s, c) = (-(r.sin()) as f64, r.cos() as f64);
        let m = s.abs().max(c.abs());
        let (x, z) = (
            b.position.x + s * distance / m,
            b.position.z + c * distance / m,
        );
        let top = b.bounds().max_position().y;
        let (bx, by, bz) = (x.floor() as i32, top.floor() as i32, z.floor() as i32);
        let below = self.res.facts().get(self.world.block(bx, by - 1, bz));
        let here = self.res.facts().get(self.world.block(bx, by, bz));
        if below.fluid != Fluid::Water
            && !here.full_collision
            && (below.full_collision || below.legacy_solid)
        {
            Vec3::new(x, f64::from(by), z)
        } else {
            Vec3::new(b.position.x, top, b.position.z)
        }
    }
    /// The boat `BoatItem.use` would place for `stack` (its SUCCESS), or
    /// None when it aims at nothing (PASS) or the spot is taken (FAIL).
    pub(super) fn boat_to_place(&self, stack: &ItemStack) -> Option<Boat> {
        let (wood, chest) = any_boat_wood(&stack.item)?;
        if !self.boat_models.has(wood, chest) {
            return None;
        }
        let hit = self.boat_item_hit()?;
        let mut boat = Boat::new(
            self.next_entity + 1,
            Arc::from(wood),
            Vec3::new(hit[0], hit[1], hit[2]),
            self.yaw,
        );
        if chest {
            boat = boat.with_chest();
        }
        let bb = boat.bounds();
        let (lo, hi) = (bb.min_position(), bb.max_position());
        let e = 1.0e-7;
        let inner = Aabb::new(
            Vec3::new(lo.x + e, lo.y + e, lo.z + e),
            Vec3::new(hi.x - e, hi.y - e, hi.z - e),
        );
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        if !scene.unobstructed(inner).unwrap_or(false) || self.boat_entity_collision(bb) {
            return None;
        }
        Some(boat)
    }
    /// The boat under the crosshair and its distance; the ECS picks it, `riding` is never a target.
    pub(super) fn boat_hit(&self) -> Option<(u32, f64)> {
        let (eye, dir) = self.aim()?;
        self.ecs.boat_pick(eye, dir, self.entity_reach, self.riding)
    }
    /// Whether the player rides a chest boat: `HasCustomInventoryScreen`, so
    /// the inventory key sends `ServerboundPlayerCommandPacket.Action.OPEN_INVENTORY`
    /// (`ServerGamePacketListenerImpl.handlePlayerCommand` opens its screen).
    pub(super) fn riding_chest_boat(&self) -> bool {
        self.riding
            .and_then(|id| self.ecs.entity_of(id))
            .and_then(|e| self.ecs.boat_wood(e))
            .is_some_and(|(_, chest)| chest)
    }
    /// `AbstractChestBoat.openCustomInventoryScreen`: the inventory key while riding a chest boat opens its chest.
    pub(super) fn open_ridden_chest_boat(&mut self) -> bool {
        let Some(id) = self.riding.filter(|_| self.riding_chest_boat()) else {
            return false;
        };
        self.open_chest_boat_container(id);
        true
    }
    /// `AbstractBoat.modifyPassengerFluidInteractionBox`: the ridden boat's top and whether it is under water, for
    /// `Entity.getFluidInteractionBox`; the ECS holds the boat.
    pub(super) fn riding_boat_fluid_box(&self) -> Option<RiddenBoat> {
        let id = self.riding?;
        let status = self.ecs.boat_status(id)?;
        let v = self.ecs_view(id)?;
        Some(RiddenBoat {
            under_water: status.under_water,
            max_y: Boat::bounds_at(Vec3::new(v.position[0], v.position[1], v.position[2])).max_position().y,
        })
    }
    /// `BoatItem.use`: the ECS spawns the boat `boat_to_place` finds and links it to a network id.
    pub(super) fn place_boat(&mut self, stack: &ItemStack) -> bool {
        let Some(boat) = self.boat_to_place(stack) else {
            return false;
        };
        let Some((wood, chest)) = any_boat_wood(&stack.item) else {
            return false;
        };
        let (at, yaw, kind) = (
            [boat.position.x, boat.position.y, boat.position.z],
            boat.yaw,
            boat.entity_type(),
        );
        let mut next = self.next_entity;
        let id = self.with_ecs(|ecs, mw| {
            ecs.spawn_boat_id(mw, wood, chest, at, yaw, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        let Some(id) = id else {
            return false;
        };
        self.apply_item_stack_config(super::commands::Target::Boat(id), &kind, stack, true);
        let key = (at[0].floor() as i32 >> 4, at[2].floor() as i32 >> 4);
        self.world.modified.insert(key);
        if self.serving.is_some() || self.options.dedicated.is_some() {
            self.native_vehicle_create(kind.as_str(), at);
        }
        let ctx = self.player_context(None);
        self.post_game_event("minecraft:entity_place", at, ctx);
        if !self.creative {
            self.consume_held();
        }
        self.award_stat("used", &stack.item, 1);
        self.swing_arm();
        true
    }
    /// `AbstractBoat.interact` on the ECS: the lead, the chest or the seat of the boat `target` names, or of the one under the crosshair.
    pub(super) fn interact_boat(&mut self, target: Option<InteractTarget>) -> bool {
        let Some((id, d)) = (match target {
            Some(t) => Some((t.id as u32, 0.0)),
            None => self.boat_hit(),
        }) else {
            return false;
        };
        let sneaking = self.keys.held(Action::Sneak);
        // Only a client's own pick is stopped by a block: the range check passed.
        if target.is_none() && !self.before_block(d) {
            return false;
        }
        let player = self.own_entity_id() as u32;
        let guest = self.ridden_by_guest(id);
        match self.with_ecs(|ecs, mw| ecs.boat_use(mw, id, player, 0, sneaking, false, guest)) {
            UseResult::Pass => false,
            UseResult::Mounted => true,
            UseResult::Opened => {
                self.open_chest_boat_container(id);
                self.swing_arm();
                true
            }
            UseResult::Leashed | UseResult::Unleashed => {
                self.swing_arm();
                true
            }
        }
    }
    /// `ContainerEntity.interactWithContainerVehicle` on the ECS: the chest's slots go under the placeholder key the menu edits.
    pub(super) fn open_chest_boat_container(&mut self, id: u32) {
        let (Some(at), Some(items)) = (self.ecs.position(id), self.ecs.boat_items(id)) else {
            return;
        };
        let key = (i32::MAX - 15, super::minecarts::CART_KEY_Y, id as i32);
        if !self.world.block_entities.contains_key(&key) {
            self.world
                .block_entities
                .insert(key, horizon_client::containers::BlockEntity::Chest(items));
        }
        self.container = Some(key);
        self.set_screen(super::Screen::Inventory);
        self.capture(false);
        self.right = false;
        let ctx = self.player_context(None);
        self.post_game_event("minecraft:container_open", at, ctx);
        self.anger_piglins(true);
    }
    /// `VehicleEntity.hurtServer` from the player's attack, on the boat under the crosshair.
    pub(super) fn attack_boat(&mut self) -> bool {
        let Some((id, d)) = self.boat_hit() else {
            return false;
        };
        if !self.before_block(d) || self.mob_hit().is_some_and(|m| m.1 < d) {
            return false;
        }
        self.strike_boat(id);
        self.swing_arm();
        true
    }
    /// The player's blow on the boat `id` (`Player.attack`'s full damage against its type); the ECS takes the hit.
    pub(super) fn strike_boat(&mut self, id: u32) {
        let (Some(e), Some(area)) = (self.ecs.entity_of(id), self.ecs.bounds(id)) else {
            return;
        };
        let Some((wood, chest)) = self.ecs.boat_wood(e) else {
            return;
        };
        let Some(row) = horizon_ecs_proto::species::boats::WOODS
            .iter()
            .find(|w| w.0 == wood)
        else {
            return;
        };
        let kind = if chest { row.2 } else { row.1 };
        let (damage, scale, _, magic) = self.melee_damage(kind, false, Some(id));
        let sweep = self.sweep_before_hit(kind, false);
        let attacker = self.own_entity_id() as u32;
        let source = horizon_client::health::HurtSource {
            damage_type: "minecraft:player_attack",
            causing: attacker as i32,
            direct: attacker as i32,
            position: None,
        };
        let vanilla = self.saved_refuses_damage(id, "minecraft:player_attack", self.creative);
        let invulnerable = crate::game::entity::damage_level_of!(self, scoped)
            .invulnerability_check(
                horizon_client::health::Hurting::Entity(id as i32, kind, source),
                vanilla,
            );
        let hurt = if invulnerable {
            None
        } else {
            self.hurt_boat_result(id, damage, self.creative, Some(Who::Player(attacker)))
        };
        // AbstractChestBoat.destroy: a player's hit that breaks a chest boat (not a creative discard) angers the piglins.
        if chest
            && hurt == Some(true)
            && !self.creative
            && self
                .ui
                .rules
                .bool_at(horizon_client::gamerules::vanilla::ENTITY_DROPS)
        {
            self.anger_piglins(true);
        }
        self.vehicle_attack_sounds(area, sweep, magic.then_some(id as i32));
        if damage > 0.0
            && let Some(destroyed) = hurt
        {
            let (knockback, _) = self.extra_knockback(kind, scale);
            if knockback > 0.0 {
                // Player.causeExtraKnockback uses Entity.push for a boat, not LivingEntity.knockback.
                if !destroyed {
                    let (x, z) = self.extra_knockback_direction();
                    self.ecs.push_entity(
                        id,
                        [
                            f64::from(-(x as f32) * knockback),
                            0.1,
                            f64::from(-(z as f32) * knockback),
                        ],
                    );
                }
                self.special_attack_slowdown(knockback);
            }
            self.special_attack_exhaustion();
        }
        // Player.onAttack: the hit starts the attack strength again.
        self.attack_ticks = 0;
    }
    /// `VehicleEntity.hurtServer` on the boat `id`: the ECS adds the damage and breaks it past 40 (`creative`: discarded at once).
    /// None when the hit is cancelled, Some(true) when the boat is gone.
    pub(in crate::game) fn hurt_boat_result(
        &mut self,
        id: u32,
        mut damage: f32,
        creative: bool,
        by: Option<Who>,
    ) -> Option<bool> {
        let mut cancel = false;
        self.native_vehicle_damage("boat", &mut damage, &mut cancel);
        if cancel {
            return None;
        }
        let drops = self
            .ui
            .rules
            .bool_at(horizon_client::gamerules::vanilla::ENTITY_DROPS);
        match self.with_ecs(|ecs, mw| ecs.boat_hit(mw, id, damage, by, creative, drops)) {
            HitResult::Ignored => None,
            HitResult::Hurt => Some(false),
            HitResult::Broken { .. } => Some(true),
        }
    }
    /// `VehicleEntity.hurtServer` for fire (`inFire`, `onFire`) or a cactus, which an `Invulnerable` boat ignores.
    /// True when the boat broke.
    pub(super) fn burn_boat(&mut self, id: u32, damage: f32, damage_type: &str) -> bool {
        let invulnerable = self.saved_refuses_damage(id, damage_type, false);
        let kind = self.ecs_view(id).map_or("", |v| v.kind).to_owned();
        let source = horizon_client::health::HurtSource {
            damage_type,
            causing: 0,
            direct: 0,
            position: None,
        };
        !self.native_nonliving_invulnerable(id, &kind, source, invulnerable) && self.hurt_boat(id, damage, false)
    }
    /// Hurts the boat `id` from a non-player source; true when it was removed.
    pub(super) fn hurt_boat(&mut self, id: u32, damage: f32, creative: bool) -> bool {
        self.hurt_boat_result(id, damage, creative, None) == Some(true)
    }
    /// Whether a spectator must be refused this boat's menu.
    pub(super) fn boat_loot_pending(&self, id: u32) -> bool {
        matches!(
            self.persist.raw.get(&id),
            Some(horizon_core::nbt::Tag::Compound(map)) if map.contains_key(&"LootTable".into())
        )
    }
    /// Existing boat access routes all use the shared unpack operation.
    pub(super) fn unpack_boat_loot(&mut self, id: u32, player: bool) {
        let player = player.then(|| self.native_acting_player()).flatten();
        self.unpack_loot(super::container_loot::LootTarget::Boat(id), player);
    }

    /// AbstractBoatRenderer.
    pub(super) fn boat_geometry(&self, camera: glam::DVec3, alpha: f64, out: &mut Vec<Vertex>) {
        let a = alpha as f32;
        let range = super::entity_culling::default_range(
            glam::DVec3::new(WIDTH, HEIGHT, WIDTH),
            self.view_scale(),
        );
        for b in &self.boats {
            // LevelExtractor: the camera entity is not drawn from its own eyes.
            if self.hides_as_camera(b.id) {
                continue;
            }
            // EntityRenderer.shouldRender.
            let (previous, position) = (at(b.previous), at(b.position));
            let Some(sight) = self.standing_entity_sight(
                camera, previous, position, alpha, WIDTH, HEIGHT, range, 0.0,
            ) else {
                continue;
            };
            let culling = self.sighted_decision(sight, || self.entity_cull_decision(b, alpha));
            if culling.hides_entity() {
                continue;
            }
            let entity_start = out.len();
            self.mark_motion(entity_start, Self::MOTION_OBJECT | u64::from(b.id));
            let models = if b.chest {
                &self.boat_models.chest_models
            } else {
                &self.boat_models.models
            };
            let Some(model) = models.get(&*b.wood) else {
                continue;
            };
            let folder = if b.chest { "chest_boat" } else { "boat" };
            let Some(uv) = self
                .gui
                .atlas
                .sprites
                .get(&format!("entity/{folder}/{}", b.wood))
                .map(|r| [r.u0, r.v0, r.u1, r.v1])
            else {
                continue;
            };
            let p = glam::DVec3::new(
                b.previous.x + (b.position.x - b.previous.x) * alpha,
                b.previous.y + (b.position.y - b.previous.y) * alpha,
                b.previous.z + (b.position.z - b.previous.z) * alpha,
            ) - camera;
            let yaw = b.previous_yaw + (b.yaw - b.previous_yaw) * a;
            let hurt = b.hurt_time as f32 - a;
            let damage = (b.damage - a).max(0.0);
            let mut base =
                glam::Mat4::from_translation(p.as_vec3() + glam::Vec3::new(0.0, 0.375, 0.0))
                    * glam::Mat4::from_rotation_y((180.0 - yaw).to_radians());
            if hurt > 0.0 {
                base *= glam::Mat4::from_rotation_x(
                    (hurt.sin() * hurt * damage / 10.0 * b.hurt_dir as f32).to_radians(),
                );
            }
            // AbstractBoatRenderer.extractRenderState: bubbleAngle.
            let wobble = b.bubble_angle(a);
            // AbstractBoatRenderer.submit: a boat above water wobbles in a
            // bubble column, about the axis (1, 0, 1).
            if !b.under_water() && wobble.abs() >= 1.0e-5 {
                base *= glam::Mat4::from_axis_angle(
                    glam::Vec3::new(1.0, 0.0, 1.0).normalize(),
                    wobble * (std::f64::consts::PI / 180.0) as f32,
                );
            }
            base *= glam::Mat4::from_scale(glam::Vec3::new(-1.0, -1.0, 1.0))
                * glam::Mat4::from_rotation_y(90f32.to_radians());
            let mut poses = Poses::new(model);
            // AbstractBoatModel.animatePaddle
            for (side, name) in [(0usize, "left_paddle"), (1, "right_paddle")] {
                if !poses.has(name) {
                    continue;
                }
                let t = b.rowing_time(side, a);
                let lerp = |f: f32, lo: f32, hi: f32| lo + f.clamp(0.0, 1.0) * (hi - lo);
                let pose = poses.get(name);
                pose[3] = lerp(((-t).sin() + 1.0) / 2.0, -PI / 3.0, -PI / 12.0);
                pose[4] = lerp(((-t + 1.0).sin() + 1.0) / 2.0, -PI / 4.0, PI / 4.0);
                if side == 1 {
                    pose[4] = PI - pose[4];
                }
            }
            let l = self.world.light(
                b.position.x.floor() as i32,
                (b.position.y + 0.3).floor() as i32,
                b.position.z.floor() as i32,
            );
            let packed = (u32::from(l >> 4) * 16) | ((u32::from(l & 15) * 16) << 8);
            horizon_client::mobs::render::draw(
                model,
                base,
                &poses,
                uv,
                0xFFFF_FFFF,
                packed,
                0,
                out,
            );
            // BoatRenderer.submitTypeAdditions: the water patch hides the water
            // inside the hull unless the boat is under water (RaftRenderer has none).
            if let Some(patch) = self.boat_models.water_patch.as_ref()
                && &*b.wood != "bamboo"
                && !matches!(b.status, Status::UnderWater | Status::UnderFlowingWater)
            {
                // Depth only, so its texture is never seen; one texel of the white sprite, the
                // same at every corner (no derivatives, so the base level at any distance), keeps
                // a shader pack's alpha test from throwing the patch away.
                let white = (self.gui.atlas.sprites.get("horizon/white")).map_or(uv, |r| {
                    let (u, v) = ((r.u0 + r.u1) / 2.0, (r.v0 + r.v1) / 2.0);
                    [u, v, u, v]
                });
                horizon_client::mobs::render::draw(
                    patch,
                    base,
                    &Poses::new(patch),
                    white,
                    0xFFFF_FFFF,
                    packed,
                    horizon_client::gfx::WATER_MASK_OVERLAY,
                    out,
                );
            }
            self.finish_lit_entity(culling, b, out, entity_start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::nbt::Tag;

    fn boat() -> Boat {
        Boat::new(1, Arc::from("oak"), Vec3::ZERO, 0.0)
    }

    /// `controlBoat`'s push with `Mth`'s sine table (`(float) Math.sin(i /
    /// 10430.378350470453)`, indexed by `(long) (radians * 10430.378350470453)
    /// & 65535`) and float products. Expected values worked out apart from
    /// Horizon, in Python with float32 rounding; the platform's sine gives
    /// -0.011894993484020233 for the first x.
    #[test]
    fn paddling_pushes_along_the_sine_table() {
        for (yaw, acceleration, push) in [
            (17.3, 0.04, (-0.011893643997609615, 0.03819085285067558)),
            (-123.45, 0.04, (0.03337550163269043, -0.022047126665711403)),
            (250.0, 0.04, (0.03758755698800087, -0.013681206852197647)),
            (-6.736, 0.005, (0.0005863539990969002, 0.004965443629771471)),
        ] {
            assert_eq!(paddle_push(yaw, acceleration), push, "yaw {yaw}");
        }
    }








}
#[cfg(test)]
mod cobweb_tests {
    use super::Boat;
    use crate::game::game_tests::tests::server_game;
    use horizon_client::chunk::Chunk;
    use horizon_core::geometry::Vec3;
    use std::sync::Arc;


}
