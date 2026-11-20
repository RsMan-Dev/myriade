//! The components every creature has: position, velocity, shape, markers of the move.
//! A cow as data components and batch systems (palier 1): the stroll goal (a geometric wait with the law of
//! `nextInt(60) == 0` each tick, then `LandRandomPos` 10 x 7), the move control, `LivingEntity.travel` on the
//! ground with Horizon's real block collision sweep (`SceneCollisions::resolve_in_border_with`), and sleeping.
//! Species data (size, step, speed, stroll range) are component values, not branches.
#[allow(unused_imports)]
use crate::IFloor as _;
#[allow(unused_imports)]
use bevy_ecs::{
    entity_disabling::Disabled,
    prelude::*,
    query::Allow,
    schedule::{Schedule, ScheduleLabel, SingleThreadedExecutor},
};
#[allow(unused_imports)]
use horizon_client_lib::mobs::{
    Mob, MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use horizon_client_render::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

// ---- components: data only -------------------------------------------------------------------------------
#[derive(Component, Clone, Copy, Debug)]
pub struct Pos(pub Vec3);

#[derive(Component, Clone, Copy, Debug)]
pub struct Vel(pub Vec3);

#[derive(Component, Clone, Copy, Debug)]
pub struct Yaw(pub f32);

/// The species' size and walking data.
#[derive(Component, Clone, Copy, Debug)]
pub struct Body {
    pub width: f32,
    pub height: f32,
    pub step_height: f32,
    pub gravity: f64,
    /// The movement speed attribute.
    pub speed: f64,
    /// The downward speed is multiplied by it in the air (1: none; the hen's `slow_fall`).
    pub fall_factor: f64,
    /// `Strider`: lava holds it up (the collision stands it on the lava surface).
    pub lava_walker: bool,
    /// The motion of an object (`ItemEntity.tick`: the gravity before the move, the drags of the air and the ground, the bounce, the water
    /// and the lava slowing it) instead of a creature's (`LivingEntity.travel`).
    pub object: bool,
    /// With `object`: the motion of an experience orb (`ExperienceOrb.tick`) instead of an item's.
    pub orb: bool,
    /// A primed TNT's motion (`PrimedTnt.tick`, `tnt.rs`).
    pub tnt: bool,
}

/// The world box of the cow: persistent data, translated by the move, changed by growth; nothing rebuilds it.
#[derive(Component, Clone, Copy, Debug)]
pub struct Bounds(pub Aabb);

/// The spot an animal is bound to (`Mob.extra.home`: a phantom's anchor, a vex's origin).
#[derive(Component, Clone, Copy, Debug)]
pub struct Anchor(pub [i32; 3]);

/// The column of the spatial index the cow is listed in (`u32::MAX`: not yet).
#[derive(Component, Clone, Copy, Debug)]
pub struct Cell(pub u32, pub [i32; 8], pub u32);

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Contact {
    pub on_ground: bool,
    pub horizontal: bool,
    pub fall_distance: f64,
}

/// What the move control left for the travel: the forward input and the speed it scales by.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Drive {
    pub zza: f32,
    /// The vertical input (`yya`: a flier's, from `FlyingMoveControl`).
    pub yya: f32,
    /// `Entity.setNoGravity` of a flier while its move control has a spot to go to (`locomotion/fly.rs`).
    pub no_gravity: bool,
    /// The sideways input (`xxa`).
    pub xxa: f32,
    /// The level plus one of a levitation effect (0: none), read by the ground travel.
    pub levitation: i32,
    pub speed: f32,
    /// `Guardian.isMoving` (DATA_ID_MOVING): its move control had a spot to swim to this tick.
    pub moving: bool,
    /// The move control asks for a jump (`MoveControl`: the target is above the step height and near).
    pub jump: bool,
    /// The jump input (a goal sets it: `FloatGoal` keeps it while the cow is in deep water).
    pub jumping: bool,
    /// `Mob.noJumpDelay`: ticks before the cow may jump again (10 after a jump, 0 when the input stops).
    pub no_jump_delay: i32,
    /// `MoveControl.Operation.JUMPING`: after a jump the move control only keeps the speed until the cow lands.
    pub jumping_op: bool,
    /// A wall climber (`walk::Params.climber`: a spider): pressed against a wall it climbs it. `climbing` is `Spider.climbing`: the contact of
    /// the last tick (`locomotion/climb.rs`).
    pub climber: bool,
    pub climbing: bool,
    /// `SmoothSwimmingMoveControl` on the ground (`walk::Params.smooth_land`: the frog): the largest turn per tick and the speed out of the water.
    pub smooth_land: Option<(f32, f32)>,
    /// `LivingEntity.shouldDiscardFriction`: the velocity keeps its horizontal part and takes the vertical one without drag while it is set (a
    /// long jump of a frog or a goat: the goal sets and clears it).
    pub discard_friction: bool,
    /// `Mob.xRot` of a swimmer steered by `SmoothSwimmingMoveControl` (the pitch the inputs follow).
    pub pitch: f32,
    /// `LivingEntity.getFlyingSpeed` of a mount its player steers: a tenth of its speed (`mount.rs`); none: 0,02.
    pub air_speed: Option<f32>,
}

/// `MoveControl`: the spot to walk to this tick.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct MoveTo {
    pub active: bool,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub speed_modifier: f64,
}

/// `RandomStrollGoal`: whether a walk is on, where to, whether an attempt is waiting in the timer wheel.
#[derive(Component, Clone, Copy, Debug)]
pub struct Stroll {
    pub walking: bool,
    /// A stroll attempt is in the timer wheel.
    pub scheduled: bool,
    pub tx: f64,
    pub tz: f64,
    /// The block row of the path target (1 on the slab; a water spot is one lower).
    pub ty: i32,
    pub horizontal_range: i32,
    pub vertical_range: i32,
    /// The attempt's chance is 1 in this (`reducedTickDelay(120)`).
    pub interval: i32,
    /// The chance of a plain spot instead of a land one (`WaterAvoidingRandomStrollGoal.probability`: 0.001; the snow golem's 1.0000001e-5).
    pub plain: f32,
    /// `RandomStrollGoal.forceTrigger`: the next ask skips the wait and the chance, until a spot is picked.
    pub force: bool,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Rng(pub u64);

/// `PathNavigation`: the path being followed.
#[derive(Component, Clone, Debug)]
pub struct Nav {
    pub path: Option<Path>,
    /// `PathNavigation.doStuckDetection`: where the cow was 100 ticks ago.
    pub last_check: u64,
    pub last_pos: Vec3,
    /// The speed modifier of the goal that asked for the path.
    pub speed: f64,
    /// The searches the path finder ran for this animal (a counter for the benches).
    pub searches: u32,
    /// `PathNavigation.timeLastRecompute` and `delayedRecompute`.
    pub last_recompute: u64,
    pub delayed: bool,
    /// `createPath`'s accuracy: how close to the target block the path may end (1 for a spot, 0 for an entity).
    pub reach: i32,
    /// `Brain.CANT_REACH_WALK_TARGET_SINCE`: the own tick of the first path since the last one that reached its target which did not (0: none).
    pub cant_reach_since: u64,
}

impl Default for Nav {
    fn default() -> Self {
        Self { path: None, last_check: 0, last_pos: Vec3::ZERO, speed: 1.0, searches: 0, last_recompute: 0, delayed: false, reach: 1, cant_reach_since: 0 }
    }
}

/// The size of an adult and of a baby of the species (data, not code).
#[derive(Component, Clone, Copy, Debug)]
pub struct Sizes {
    pub adult: (f32, f32),
    pub baby: (f32, f32),
}

/// A path was asked for (the stroll picked a spot): `pathfind` visits only these.
#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
pub struct NeedsPath;

/// The walk of the cow is over (path done, blocked, or none found): `path_ended` arms its next stroll attempt.
#[derive(Component, Clone, Copy, Debug)]
pub struct PathEnded;

/// The cow follows a path: `follow` and `move_control` visit only these.
#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
#[component(on_add = crate::locomotion::hop::following_started)]
pub struct Following;

/// The entity stands still (`stare_freeze`): `follow` and `move_control` skip it, its path is kept.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Stilled;

/// The physics of the cow is active (it moves, falls, or was just pushed): `travel` visits only these, and takes
/// the marker off when the cow is back at rest.
#[derive(Component, Clone, Copy, Debug)]
#[cfg_attr(feature = "sparse", component(storage = "SparseSet"))]
#[component(on_add = crate::cow::moving_added)]
pub struct Moving;

/// `Mob.noActionTime` as a deadline: the stroll goal is refused from this tick on (100 ticks after a player was
/// last within 32 blocks, or the cow was hurt).
#[derive(Component, Clone, Copy, Debug)]
pub struct ActiveUntil(pub u64);

/// The players of the bench: positions (x, z). `dirty` is the event: a player moved, came or left.
#[derive(Resource, Default)]
pub struct Players {
    pub list: Vec<(f64, f64)>,
    pub(crate) prev: Vec<(f64, f64)>,
    pub dirty: bool,
    /// The players by bin of 32 blocks (side `n`), rebuilt when one moves, the places they had before, and the
    /// cells the last move touched (scratch buffers kept between ticks).
    pub(crate) bins: Vec<Vec<(f64, f64)>>,
    pub(crate) n: i64,
    pub(crate) old: Vec<(f64, f64)>,
    pub(crate) candidates: Vec<usize>,
}

/// An object (an item on the ground, an experience orb) neither pushes nor is pushed (`Entity.isPushable`): `push` does not visit it.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PushFree;

/// A baby grows up at this tick (it is in the timer wheel then; nothing counts its age every tick).
#[derive(Component, Clone, Copy, Debug)]
#[component(on_insert = watch_in::<WATCH_FOLLOW_PARENT>, on_discard = watch_out::<WATCH_FOLLOW_PARENT>)]
pub struct GrowAt(pub u64);

impl Rng {
    pub fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
    pub fn next_int(&mut self, bound: i32) -> i32 {
        (self.next_u32() % bound as u32) as i32
    }
    pub fn next_f64(&mut self) -> f64 {
        f64::from(self.next_u32()) / (f64::from(u32::MAX) + 1.0)
    }
}
