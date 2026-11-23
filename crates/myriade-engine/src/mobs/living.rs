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



