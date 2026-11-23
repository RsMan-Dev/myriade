//! `LivingEntity` movement for mobs: `aiStep` input decay and jumping,
//! `travel` in air, water and lava (friction-scaled acceleration, gravity,
//! fluid slowdown and falling adjustment), fluid immersion heights, fall
//! damage, fire and the walk animation.
use super::{MobEvent, MobWorld};
use crate::health::DamageLevel;
use crate::{blocks::Fluid, collisions::SceneCollisions};
use horizon_core::{
    geometry::{Aabb, Vec3, trig},
    position::BlockPos,
    random::RandomSource,
};
use horizon_world::native_load_context::LoopContext;
use horizon_world::{
    block_landing::LandingEntityKind,
    entity_fluid::{CurrentAccumulator, CurrentTarget},
    entity_move::MoverType,
    fluid::{FluidKind, FluidState},
    fluid_flow::FluidFlowWorld,
    move_gate::MoveGate,
    movement_restitution::{RestitutionInput, RestitutionWorld, restitute_movement},
    registry::BlockStateId,
};
#[path = "climbing.rs"]
mod climbing;

/// `DeathProtection.TOTEM_EFFECTS`: clear, then regeneration II, absorption II
/// and fire resistance.
pub(super) fn totem_death_effects() -> Vec<crate::items::ConsumeEffect> {
    use crate::items::{ConsumeEffect, ConsumeEffectSpec};
    let spec = |id: &str, duration, amplifier| ConsumeEffectSpec {
        id: id.to_owned(),
        duration,
        amplifier,
        ambient: false,
        show_particles: true,
        show_icon: true,
    };
    vec![
        ConsumeEffect::ClearAllEffects,
        ConsumeEffect::ApplyEffects {
            effects: vec![
                spec("minecraft:regeneration", 900, 1),
                spec("minecraft:absorption", 100, 1),
                spec("minecraft:fire_resistance", 800, 0),
            ],
            probability: 1.0,
        },
    ]
}

/// How many block particles a landing throws up
/// (`(int)(min(0.2F + power / 15, 2.5) * 150)`), none when the fall is within
/// `safe_fall_distance` (`power = max(floor(fall + 1.0E-6 - safe), 0)`).
fn landing_dust_count(fall_distance: f64, safe_fall_distance: f64) -> Option<i32> {
    let power = f64::from(((fall_distance + 1.0e-6 - safe_fall_distance).floor() as i32).max(0));
    (power > 0.0).then(|| (150.0 * (f64::from(0.2_f32) + power / 15.0).min(2.5)) as i32)
}
/// Where the dust starts: at the mob, or, when its own block is not the one it
/// landed on (`landing`), on that block's face toward the mob.
fn landing_dust_at(position: Vec3, landing: [i32; 3]) -> Vec3 {
    if landing[0] == position.x.floor() as i32 && landing[2] == position.z.floor() as i32 {
        return position;
    }
    let dx = position.x - f64::from(landing[0]) - 0.5;
    let dz = position.z - f64::from(landing[2]) - 0.5;
    let scale = dx.abs().max(dz.abs());
    Vec3::new(
        f64::from(landing[0]) + 0.5 + dx / scale * 0.5,
        position.y,
        f64::from(landing[2]) + 0.5 + dz / scale * 0.5,
    )
}

/// `Entity.moveDist`, `flyDist`, `nextStep` and `Parrot.nextFlap`.
#[derive(Clone, Copy, Debug)]
pub struct Steps {
    pub move_dist: f32,
    pub fly_dist: f32,
    pub next_step: f32,
    pub next_flap: f32,
    /// `AbstractHorse.gallopSoundCounter`.
    pub gallop: i32,
}
impl Default for Steps {
    fn default() -> Self {
        Self {
            move_dist: 0.0,
            fly_dist: 0.0,
            next_step: 1.0,
            next_flap: 1.0,
            gallop: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Fluids {
    /// The boat whose fluid interaction box last seated this passenger.
    pub boat: Option<(u32, crate::motion::RiddenBoat)>,
    pub water: f64,
    pub lava: f64,
    pub in_water: bool,
    pub in_lava: bool,
    pub eye_in_water: bool,
    pub eye_in_lava: bool,
}
/// `FluidFlowWorld` over the mob's scene: the neighbours a flow vector reads.
struct FlowScene<'a, 'w>(&'a MobWorld<'w>);
impl FluidFlowWorld for FlowScene<'_, '_> {
    type Error = std::convert::Infallible;
    type Block = BlockStateId;
    fn fluid(&mut self, p: BlockPos) -> Result<FluidState, Self::Error> {
        Ok(self.0.fluid_tables.fluid(self.0.world.block(p.x, p.y, p.z)))
    }
    fn block(&mut self, p: BlockPos) -> Result<BlockStateId, Self::Error> {
        Ok(self.0.world.block(p.x, p.y, p.z))
    }
    fn blocks_fluid_flow(&self, s: BlockStateId) -> bool {
        self.0.fluid_tables.blocks_flow(s)
    }
    fn is_ice_class(&self, s: BlockStateId) -> bool {
        self.0.fluid_tables.ice(s)
    }
    fn is_face_sturdy(
        &mut self,
        s: BlockStateId,
        _: BlockPos,
        d: horizon_core::geometry::Direction,
    ) -> Result<bool, Self::Error> {
        Ok(self.0.fluid_tables.sturdy(s, d))
    }
}
/// The mob's velocity, for `CurrentAccumulator::apply_to`.
struct Pushed<'a>(&'a mut Vec3);
impl CurrentTarget for Pushed<'_> {
    type Error = std::convert::Infallible;
    fn is_player_class(&self) -> bool {
        false
    }
    fn velocity(&mut self) -> Result<Vec3, Self::Error> {
        Ok(*self.0)
    }
    fn add_delta_movement(&mut self, delta: Vec3) -> Result<(), Self::Error> {
        let sum = Vec3::new(self.0.x + delta.x, self.0.y + delta.y, self.0.z + delta.z);
        if delta.is_finite() && sum.is_finite() {
            *self.0 = sum;
        }
        Ok(())
    }
}
/// Dimensions as `EntityDimensions` holds them: the floats `Mob::dims` widened, back.
pub fn dims_array((w, h, e): (f64, f64, f64)) -> [f32; 3] {
    [w as f32, h as f32, e as f32]
}
pub fn wrap(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}
impl MobWorld<'_> {
    /// Entity.updateFluidHeightAndDoFluidPushing's currents over the deflated
    /// box: the flow of every water and lava block it touches, summed (a thin
    /// fluid layer scales its flow down), as (water, lava).
    fn fluid_currents(&self, bounds: Aabb) -> (CurrentAccumulator, CurrentAccumulator) {
        let base_y = bounds.min_position().y;
        let b = bounds.deflate(Vec3::new(0.001, 0.001, 0.001));
        let (lo, hi) = (b.min_position(), b.max_position());
        let mut scene = FlowScene(self);
        let (mut water, mut lava) = (CurrentAccumulator::default(), CurrentAccumulator::default());
        for x in lo.x.floor() as i32..=(hi.x.ceil() as i32 - 1) {
            for y in lo.y.floor() as i32..=(hi.y.ceil() as i32 - 1) {
                for z in lo.z.floor() as i32..=(hi.z.ceil() as i32 - 1) {
                    let p = BlockPos::new(x, y, z);
                    let state = self.fluid_tables.fluid(self.world.block(x, y, z));
                    let current = match state.kind() {
                        // A mod's fluid is in neither tag, so it pushes no entity.
                        FluidKind::Empty | FluidKind::Mod(_) | FluidKind::ModFlowing(_) => continue,
                        FluidKind::Water | FluidKind::FlowingWater => &mut water,
                        FluidKind::Lava | FluidKind::FlowingLava => &mut lava,
                    };
                    let surface = f64::from(y) + self.fluid_height(x, y, z).1;
                    if surface < lo.y {
                        continue;
                    }
                    current.height = current.height.max(surface - base_y);
                    let mut flow =
                        crate::motion::pushing_flow(self.loop_context, &mut scene, state, p)
                            .unwrap_or(Vec3::ZERO);
                    if current.height < 0.4 {
                        flow = flow.scale(current.height);
                    }
                    current.accumulate(flow);
                }
            }
        }
        (water, lava)
    }
    /// `Entity.updateFluidInteraction`'s pushes for an entity fluids push
    /// (`isPushedByFluid`) with the box `bounds`: its `velocity` after the
    /// water's and the lava's currents (`EntityFluidInteraction.applyCurrentTo`).
    pub fn pushed_by_currents(&self, bounds: Aabb, mut velocity: Vec3) -> Vec3 {
        let (water, lava) = self.fluid_currents(bounds);
        let lava_scale = if self.dimension_type.fast_lava {
            0.007
        } else {
            0.0023333333333333335
        };
        let mut pushed = Pushed(&mut velocity);
        if water.height > 0.0 {
            let _ = water.apply_to(&mut pushed, 0.014);
        }
        if lava.height > 0.0 {
            let _ = lava.apply_to(&mut pushed, lava_scale);
        }
        velocity
    }
    /// `BlockState.getCollisionShape(level, pos)` (the empty collision
    /// context) as boxes in the world.
    pub fn block_collision_boxes(&self, x: i32, y: i32, z: i32) -> Vec<Aabb> {
        let offset = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
        self.block_shape(x, y, z)
            .map(|shape| shape.boxes().iter().map(|b| b.move_by(offset)).collect())
            .unwrap_or_default()
    }
    /// Whether the block's collision shape (empty context) has a box meeting `area`.
    fn block_collides_with(&self, x: i32, y: i32, z: i32, area: Aabb) -> bool {
        let offset = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
        self.block_shape(x, y, z).is_some_and(|shape| {
            shape
                .boxes()
                .iter()
                .any(|b| b.move_by(offset).intersects(area))
        })
    }
    fn block_shape(&self, x: i32, y: i32, z: i32) -> Option<horizon_core::geometry::VoxelShape> {
        let mut scene = SceneCollisions {
            world: self.world,
            binding: self.collision,
            subject: None,
        };
        let state = self.world.block(x, y, z);
        let at = horizon_core::position::BlockPos::new(x, y, z);
        let context = horizon_world::collision_context::CollisionContext::empty(false);
        self.collision
            .collision_shape_with_context(&mut scene, state, at, &context)
            .ok()
    }
    /// `Level.findSupportingBlock`: of the blocks whose collision shape
    /// meets `area`, the one whose centre is nearest `from` (ties: the
    /// greatest by `Vec3i.compareTo`, y then z then x).
    ///
    /// Like `BlockCollisions`' cursor, a block whose shape stays inside its
    /// own cell (not `hasLargeCollisionShape`, not a moving piston) is only
    /// shaped when that cell meets `area`: elsewhere it cannot. The pick is
    /// a total order, so the column-by-column visit finds the same block.
    pub fn supporting_block(&self, area: Aabb, from: Vec3) -> Option<[i32; 3]> {
        use horizon_core::geometry::Axis;
        let range = |axis| {
            (area.min(axis) - 1.0e-7).floor() as i32 - 1
                ..=(area.max(axis) + 1.0e-7).floor() as i32 + 1
        };
        // Aabb.intersects of the unit cell at `c` along one axis.
        let meets = |axis, c: i32| {
            let c = f64::from(c);
            c < area.max(axis) && c + 1.0 > area.min(axis)
        };
        // Where no section of the cursor holds a block reaching out of its
        // cell, only the cells meeting `area` can hold the support.
        let (rx, ry, rz) = (range(Axis::X), range(Axis::Y), range(Axis::Z));
        let outer = self.world.may_hold(
            [*rx.start(), *ry.start(), *rz.start()],
            [*rx.end(), *ry.end(), *rz.end()],
            |s| self.collision.reaches_out(s),
        );
        let mut best: Option<(f64, [i32; 3])> = None;
        for x in rx {
            for z in rz.clone() {
                let column = meets(Axis::X, x) && meets(Axis::Z, z);
                if !outer && !column {
                    continue;
                }
                let Some(chunk) = self.world.chunks.get(&(x >> 4, z >> 4)) else {
                    continue;
                };
                for y in ry.clone() {
                    if !outer && !meets(Axis::Y, y) {
                        continue;
                    }
                    let state = chunk.block(x & 15, y, z & 15);
                    let Ok(block) = self.collision.collision_block(state) else {
                        continue;
                    };
                    if !(block.large_collision_shape
                        || block.moving_piston
                        || (column && meets(Axis::Y, y)))
                    {
                        continue;
                    }
                    if !self.block_collides_with(x, y, z, area) {
                        continue;
                    }
                    let d = (f64::from(x) + 0.5 - from.x).powi(2)
                        + (f64::from(y) + 0.5 - from.y).powi(2)
                        + (f64::from(z) + 0.5 - from.z).powi(2);
                    let greater = |b: [i32; 3]| (y, z, x) > (b[1], b[2], b[0]);
                    if best.is_none_or(|(bd, b)| d < bd || d == bd && greater(b)) {
                        best = Some((d, [x, y, z]));
                    }
                }
            }
        }
        best.map(|(_, p)| p)
    }
    pub fn no_collision(&self, b: Aabb) -> bool {
        let mut scene = SceneCollisions {
            world: self.world,
            binding: self.collision,
            subject: None,
        };
        scene.unobstructed(b).unwrap_or(false)
    }
    /// `LevelReader.containsAnyLiquid`: the blocks from the box's minimum
    /// corner to the block holding its maximum corner.
    pub fn contains_any_liquid(&self, b: Aabb) -> bool {
        let (lo, hi) = (b.min_position(), b.max_position());
        let (x0, y0, z0) = (
            lo.x.floor() as i32,
            lo.y.floor() as i32,
            lo.z.floor() as i32,
        );
        let (x1, y1, z1) = (
            hi.x.floor() as i32,
            hi.y.floor() as i32,
            hi.z.floor() as i32,
        );
        (x0..=x1).any(|x| {
            (y0..=y1).any(|y| {
                (z0..=z1).any(|z| self.facts.get(self.world.block(x, y, z)).fluid != Fluid::None)
            })
        })
    }
}
/// LivingEntity.getFluidFallingAdjustedMovement.
pub fn fluid_falling(gravity: f64, falling: bool, sprinting: bool, m: Vec3) -> Vec3 {
    if gravity != 0.0 && !sprinting {
        let y = if falling && (m.y - 0.005).abs() >= 0.003 && (m.y - gravity / 16.0).abs() < 0.003 {
            -0.003
        } else {
            m.y - gravity / 16.0
        };
        Vec3::new(m.x, y, m.z)
    } else {
        m
    }
}

/// `CollisionGetter.findFreePosition`: within `allowed`, the point nearest
/// `preferred` at which a box of the size, centred there, meets no block.
pub(crate) fn free_position(
    w: &MobWorld,
    allowed: Aabb,
    preferred: Vec3,
    size_x: f64,
    size_y: f64,
) -> Option<Vec3> {
    use horizon_core::geometry::{ShapeOp, VoxelShape};
    let allowed_shape = VoxelShape::from_box(allowed).ok()?;
    if allowed_shape.is_empty() {
        return None;
    }
    let half = Vec3::new(size_x / 2.0, size_y / 2.0, size_x / 2.0);
    let area = allowed.inflate(half);
    let (lo, hi) = (area.min_position(), area.max_position());
    let mut collisions = VoxelShape::empty();
    // getBlockCollisions: the block shapes that reach into the area.
    for x in (lo.x - 1.0e-7).floor() as i32 - 1..=(hi.x + 1.0e-7).floor() as i32 + 1 {
        for y in (lo.y - 1.0e-7).floor() as i32 - 1..=(hi.y + 1.0e-7).floor() as i32 + 1 {
            for z in (lo.z - 1.0e-7).floor() as i32 - 1..=(hi.z + 1.0e-7).floor() as i32 + 1 {
                let boxes = w.block_collision_boxes(x, y, z);
                let touches = boxes.iter().any(|b| {
                    let (a, c) = (b.min_position(), b.max_position());
                    a.x < hi.x && c.x > lo.x && a.y < hi.y && c.y > lo.y && a.z < hi.z && c.z > lo.z
                });
                if !touches {
                    continue;
                }
                for b in boxes {
                    let grown = VoxelShape::from_box(b.inflate(half)).ok()?;
                    collisions = collisions.join(&grown, ShapeOp::OR).ok()?;
                }
            }
        }
    }
    let free = allowed_shape.join(&collisions, ShapeOp::ONLY_FIRST).ok()?;
    free.closest_point(preferred)
}

/// `Entity.doWaterSplashEffect` for a mob: its own random, its splash sound
/// (`getSwimSplashSound`), particles for the client and the SPLASH game event.
struct Splash<'a> {    events: &'a mut Vec<MobEvent>,
}
impl horizon_world::water_splash::WaterSplashWorld for Splash<'_> {
    type Error = std::convert::Infallible;
    type Entity = ();
    type Sound = &'static str;
    fn this_entity(&self) {}
    fn controlling_passenger(&mut self) -> Result<Option<()>, Self::Error> {
        Ok(None)
    }
    fn velocity(&mut self, _: ()) -> Result<Vec3, Self::Error> {
        Ok(self.mob.velocity)
    }
    fn splash_sound(&mut self, high_speed: bool) -> Result<&'static str, Self::Error> {
        use super::Species;
        // Entity.getSwimSplashSound and getSwimHighSpeedSplashSound: the only
        // high-speed override is Player's, which never reaches here, so every
        // mob splashes generically when it enters fast. The slow overrides are
        // Monster's, the hoglin's, the dolphin's and the axolotl's.
        if high_speed {
            return Ok("entity.generic.splash");
        }
        let species = self.mob.species();
        Ok(match species {
            Species::Dolphin => "entity.dolphin.splash",
            Species::Axolotl => "entity.axolotl.splash",
            _ if species == Species::Hoglin || super::ai::is_monster(species) => {
                "entity.hostile.splash"
            }
            _ => "entity.generic.splash",
        })
    }
    fn random_float(&mut self) -> Result<f32, Self::Error> {
        Ok(self.mob.random.next_float())
    }
    fn random_double(&mut self) -> Result<f64, Self::Error> {
        Ok(self.mob.random.next_double())
    }
    fn play_sound(
        &mut self,
        sound: &'static str,
        volume: f32,
        pitch: f32,
    ) -> Result<(), Self::Error> {
        if !self.mob.silent {
            let at = [
                self.mob.position.x,
                self.mob.position.y,
                self.mob.position.z,
            ];
            self.events.push(MobEvent::Sound(sound, at, volume, pitch));
        }
        Ok(())
    }
    fn position(&self) -> Vec3 {
        self.mob.position
    }
    fn width(&self) -> f32 {
        self.mob.width()
    }
    fn particle(
        &mut self,
        kind: horizon_world::water_splash::SplashParticle,
        position: Vec3,
        velocity: Vec3,
    ) -> Result<(), Self::Error> {
        self.events.push(MobEvent::ParticleAt {
            kind: match kind {
                horizon_world::water_splash::SplashParticle::Bubble => "bubble",
                horizon_world::water_splash::SplashParticle::Splash => "splash",
            },
            at: [position.x, position.y, position.z],
            velocity: [velocity.x, velocity.y, velocity.z],
        });
        Ok(())
    }
    fn splash_event(&mut self) -> Result<(), Self::Error> {
        self.events.push(MobEvent::GameEvent {
            event: "minecraft:splash",
            at: [
                self.mob.position.x,
                self.mob.position.y,
                self.mob.position.z,
            ],
            source: self.mob.id,
            state: horizon_world::registry::BlockStateId(0),
        });
        Ok(())
    }
}

/// `Entity.getEyePosition(partialTick)` as the cell the renderer lights an
/// entity from (`Entity.getLightProbePosition` is that position and
/// `EntityRenderer.getPackedLightCoords` takes `BlockPos.containing` of it):
/// `Mth.lerp(partialTick, xo, x)` on each axis, plus `getEyeHeight()` on y.
pub fn eye_probe_cell(previous: Vec3, position: Vec3, eye_height: f64, partial: f64) -> [i32; 3] {
    let at = previous.lerp(position, partial);
    [
        at.x.floor() as i32,
        (at.y + eye_height).floor() as i32,
        at.z.floor() as i32,
    ]
}


/// `Entity.restituteMovementAfterCollisions`' native inputs, bound for a mob:
/// its own bounciness attribute, the block's coefficients and the pose of a
/// mob that never sneaks. The phases themselves are
/// `movement_restitution::restitute_movement`, the same port the player and
/// dropped items use.
struct MobRestitution<'a, 'w, 'l> {    world: &'a MobWorld<'w>,
    level: &'a mut DamageLevel<'l>,
    /// `Entity.getOnPosLegacy`: the block whose bounce this move sees.
    on: BlockPos,
}
impl<'a, 'w, 'l> MobRestitution<'a, 'w, 'l> {
    /// The block's movement data: its coefficients and its bounce tag.
    fn movement(
        &self,
        state: BlockStateId,
    ) -> Result<horizon_world::movement_properties::MovementBlock, ()> {
        self.world.movement.get(state).map_err(|_| ())
    }
}
impl RestitutionWorld for MobRestitution<'_, '_, '_> {
    // The block data cannot fail here: a state the registry does not know
    // reads as a block with no bounce, never as an error.
    type Error = ();
    /// The entity's own suppression is its sneak pose, and a mob never
    /// sneaks; the block's tag is `block_suppresses_bounce` below.
    fn suppressing_bounce(&mut self) -> Result<bool, ()> {
        Ok(false)
    }
    /// `getAttributeValue(BOUNCINESS)`: a sulfur cube's.
    fn entity_bounciness(&mut self) -> Result<f64, ()> {
        Ok(self.mob.entity_bounciness())
    }
    fn velocity(&mut self) -> Result<Vec3, ()> {
        Ok(self.mob.velocity)
    }
    /// `living_motion::effective_gravity`: the gravity attribute, floored to
    /// 0.01 while a Slow Falling mob is on its way down.
    fn effective_gravity(&mut self) -> Result<f64, ()> {
        Ok(self.mob.effective_gravity())
    }
    fn block_suppresses_bounce(&mut self, state: BlockStateId) -> Result<bool, ()> {
        Ok(self.movement(state)?.suppresses_bounce)
    }
    /// `Entity.getBlockBounciness`: the restitution of the block under the
    /// entity, so a mob landing on a slime block or a bed bounces too, after
    /// the block's parts, on the side that ticks the mob.
    fn block_bounce_restitution(&mut self, state: BlockStateId) -> Result<f32, ()> {
        let own = self.movement(state)?.coefficients.bounce_restitution;
        let contact = crate::block_contact::Contact {
            entity: self.mob.id as i32,
            kind: &self.mob.kind.id,
            state,
            position: self.on,
        };
        Ok(crate::block_contact::bounce_restitution(
            &mut self.level.reborrow(),
            contact,
            own,
        ))
    }
    /// `LivingEntity.getAirDrag`.
    fn air_drag(&mut self) -> Result<f32, ()> {
        Ok(self.mob.air_drag())
    }
    /// `Entity.getBlockPosBelowThatAffectsMyMovement`: `Entity.getOnPos(0.500001F)`,
    /// which keeps the supporting fence gate's or wall's column. The offset is
    /// over 0.5, so the fence case never applies.
    fn movement_support_position(&mut self) -> Result<BlockPos, ()> {
        let position = self.mob.position;
        let offset = f64::from(horizon_world::movement_support::MOVEMENT_SUPPORT_OFFSET);
        let Some(support) = self.mob.supporting else {
            return Ok(BlockPos::new(
                position.x.floor() as i32,
                (position.y - offset).floor() as i32,
                position.z.floor() as i32,
            ));
        };
        let state = self.world.world.block(support[0], support[1], support[2]);
        let keeps_column = self
            .movement(state)
            .is_ok_and(|b| b.wall || b.coefficients.fence_gate);
        Ok(if keeps_column {
            BlockPos::new(support[0], support[1], support[2])
        } else {
            BlockPos::new(support[0], (position.y - offset).floor() as i32, support[2])
        })
    }
    /// `BlockLandingRegistry.bounce_on`'s only case is the shelf mushroom's
    /// bounce sound, which no path plays yet (`landing.rs` binds it for the
    /// player too and the game's sound code resolves neither).
    fn bounce_on(
        &mut self,
        state: BlockStateId,
        position: BlockPos,
        fall_distance: f64,
        movement: Vec3,
    ) -> Result<Vec3, ()> {
        Ok(crate::block_contact::bounce(
            self.level,
            crate::block_contact::Contact {
                entity: self.mob.id as i32,
                kind: &self.mob.kind.id,
                state,
                position,
            },
            fall_distance,
            movement,
        ))
    }
    fn bounce_event(&mut self) -> Result<(), ()> {
        Ok(())
    }
    /// `Entity.syncPosition`: the tracker keeps where the mob bounced as a
    /// step of its next move (`SteppedInterpolationTracker.updateTracking`).
    fn mark_position_for_sync(&mut self) {
        self.mob.sync_position = true;
    }
    fn set_velocity(&mut self, velocity: Vec3) -> Result<(), ()> {
        self.mob.velocity = velocity;
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;


    /// `Entity.getLightProbePosition(partialTick)` for a living entity is
    /// `getEyePosition`: `getEyeHeight()` over `Mth.lerp(partialTick, xo, x)`.
    /// Horizon probed this tick's own position, so a mob walking into the next
    /// cell was drawn lit by that cell for the whole frame before it got there,
    /// and a falling one for a frame after it left.
    #[test]
    fn a_mob_probe_follows_the_frame() {
        let eye = 1.62;
        // Walking from x = 0.9 to 1.1: the eye is in the cell at 0 until the
        // frame reaches the boundary, and in the cell at 1 from there on.
        let (previous, position) = (Vec3::new(0.9, 64.0, 0.5), Vec3::new(1.1, 64.0, 0.5));
        assert_eq!(eye_probe_cell(previous, position, eye, 0.0), [0, 65, 0]);
        assert_eq!(eye_probe_cell(previous, position, eye, 0.5), [1, 65, 0]);
        assert_eq!(eye_probe_cell(previous, position, eye, 1.0), [1, 65, 0]);
        // The old probe was the tick's own position on every frame.
        assert_eq!(position.x.floor() as i32, 1);
        // Falling a block: the eye is at 71.62 at the start of the frame and
        // 70.62 at its end.
        let (previous, position) = (Vec3::new(0.5, 70.0, 0.5), Vec3::new(0.5, 69.0, 0.5));
        assert_eq!(eye_probe_cell(previous, position, eye, 0.0), [0, 71, 0]);
        assert_eq!(eye_probe_cell(previous, position, eye, 0.5), [0, 71, 0]);
        assert_eq!(eye_probe_cell(previous, position, eye, 1.0), [0, 70, 0]);
    }







    /// `LivingEntity.checkFallDamage`'s dust: nothing within the safe distance,
    /// `150 * min(0.2 + power / 15, 2.5)` block particles beyond it.
    #[test]
    fn a_landing_throws_up_dust_by_the_power_of_the_fall() {
        // Within the safe distance of 3, or by a hair over it: power 0.
        assert_eq!(landing_dust_count(2.5, 3.0), None);
        assert_eq!(landing_dust_count(3.0, 3.0), None);
        // A fall of 7: power 4, 150 * (0.2 + 4 / 15) = 70.
        assert_eq!(landing_dust_count(7.0, 3.0), Some(70));
        // A long fall is capped at 150 * 2.5.
        assert_eq!(landing_dust_count(60.0, 3.0), Some(375));
        // A mob that takes no fall damage still has a safe distance of its own.
        assert_eq!(landing_dust_count(7.0, 10.0), None);
    }
    /// Over the block it lands on the dust starts at the mob; straddling
    /// into another column it is pulled to that block's near edge.
    #[test]
    fn landing_dust_is_pulled_to_the_block_it_landed_on() {
        let on = Vec3::new(4.3, 65.0, 7.6);
        assert_eq!(landing_dust_at(on, [4, 64, 7]), on);
        // The mob stands over block (4, 7) but its feet block is (5, 7).
        let straddling = landing_dust_at(Vec3::new(5.1, 65.0, 7.5), [4, 64, 7]);
        assert_eq!(straddling, Vec3::new(5.0, 65.0, 7.5));
    }





}

#[path = "living/fluid_heights.rs"]
mod fluid_heights;
#[allow(dead_code)]
pub mod section_writes;
pub mod teleports;


/// `Entity.move`'s fall-reset ray for a mob, with the world's
/// `minecraft:fall_damage_resetting` blocks.
pub(crate) fn fall_reset_hit(w: &MobWorld, from: Vec3, moved: Vec3) -> bool {
    let reset = w.block_tags.get("minecraft:fall_damage_resetting");
    crate::entities::fall_reset_hit(
        w.loop_context,
        w.world,
        w.fluid_tables,
        |p| {
            let name = &w.facts.get(w.world.block(p.x, p.y, p.z)).name;
            reset.is_some_and(|t| t.contains(&**name))
        },
        from,
        moved,
    )
}

