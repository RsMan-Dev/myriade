use super::MobWorld;
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
    pub fn pushed_by_currents(&self, bounds: Aabb, velocity: Vec3) -> Vec3 {
        let cur = self.fluid_currents(bounds);
        self.apply_currents(cur, velocity)
    }
    fn apply_currents(&self, (water, lava): (CurrentAccumulator, CurrentAccumulator), mut velocity: Vec3) -> Vec3 {
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
    /// [`Self::fluids`] and [`Self::pushed_by_currents`] in one walk of the box: the fluids, and `velocity` after the currents when the
    /// box is in water or lava.
    pub fn fluids_pushed(&self, bounds: Aabb, eye_y: f64, velocity: &mut Vec3) -> Fluids {
        let mut cur = (CurrentAccumulator::default(), CurrentAccumulator::default());
        let f = self.fluids_scan(bounds, bounds.min_position().y, eye_y, Some(&mut cur));
        if f.in_water || f.in_lava {
            *velocity = self.apply_currents(cur, *velocity);
        }
        f
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



