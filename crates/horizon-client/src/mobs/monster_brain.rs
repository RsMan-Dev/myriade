//! The brains of the breeze (`Breeze.BRAIN_PROVIDER`, `BreezeAi`) and the
//! creaking (`Creaking.BRAIN_PROVIDER`, `CreakingAi`), ported behaviour for
//! behaviour on the villager brain's memories (`villager_brain::Slot`):
//! sensors with vanilla's scan delays, the core, idle and fight activities,
//! and their behaviours started and ticked in `Brain.tick`'s order
//! (priority, then the activity's slot in its hash map, then insertion).
//!
//! Behaviour durations, `DoNothing` and the creaking's look ticker draw from
//! the level random; positions, sounds and jumps from the mob's own random;
//! `RunOne` shuffles with a random of its own (`ShufflingList.random`),
//! seeded when the brain is built.
use super::{
    MobEvent, MobWorld, Species, Who,
    brain::{Timed, living_near},
    goals::Cx,
    path::Path,
    villager_brain::{NEVER, Slot, Tracker, Visible, WalkTarget, compute_path, reached},
};
use crate::health::DamageLevel;
use crate::projectiles::{Projectile, ProjectileKind};
use horizon_core::{
    geometry::{Vec3, trig},
    random::{LegacyRandom, RandomSource},
};
use mod_api::SoundSource;

/// `Activity` (the ones these brains use).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Core,
    Idle,
    Fight,
}
impl Act {
    fn slot(self) -> u8 {
        match self {
            Act::Core => 1,
            Act::Idle => 5,
            Act::Fight => 14,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Act::Core => "core",
            Act::Idle => "idle",
            Act::Fight => "fight",
        }
    }
}

/// The memories either brain registers.
#[derive(Clone, Debug, Default)]
pub struct Memories {
    pub attack_target: Slot<Who>,
    pub attack_cooling_down: Slot<bool>,
    pub cant_reach_walk_target_since: Slot<i64>,
    /// `HURT_BY`: the damage (its game time).
    pub hurt_by: Slot<i64>,
    pub hurt_by_entity: Slot<Who>,
    pub look_target: Slot<Tracker>,
    pub mobs: Slot<Vec<Who>>,
    pub visible_mobs: Slot<Visible>,
    pub nearest_attackable: Slot<Who>,
    pub nearest_players: Slot<Vec<Who>>,
    pub nearest_visible_player: Slot<Who>,
    pub nearest_visible_targetable_player: Slot<Who>,
    pub nearest_visible_targetable_players: Slot<Vec<Who>>,
    pub path: Slot<Path>,
    pub walk_target: Slot<WalkTarget>,
    pub breeze_shoot: Slot<()>,
    pub breeze_shoot_charging: Slot<()>,
    pub breeze_shoot_recovering: Slot<()>,
    pub breeze_shoot_cooldown: Slot<()>,
    pub breeze_jump_cooldown: Slot<()>,
    pub breeze_jump_inhaling: Slot<()>,
    pub breeze_jump_target: Slot<[i32; 3]>,
    pub breeze_leaving_water: Slot<()>,
}
impl Memories {
    /// `Brain.forgetOutdatedMemories`.
    fn tick(&mut self) {
        self.attack_target.tick();
        self.attack_cooling_down.tick();
        self.cant_reach_walk_target_since.tick();
        self.hurt_by.tick();
        self.hurt_by_entity.tick();
        self.look_target.tick();
        self.mobs.tick();
        self.visible_mobs.tick();
        self.nearest_attackable.tick();
        self.nearest_players.tick();
        self.nearest_visible_player.tick();
        self.nearest_visible_targetable_player.tick();
        self.nearest_visible_targetable_players.tick();
        self.path.tick();
        self.walk_target.tick();
        self.breeze_shoot.tick();
        self.breeze_shoot_charging.tick();
        self.breeze_shoot_recovering.tick();
        self.breeze_shoot_cooldown.tick();
        self.breeze_jump_cooldown.tick();
        self.breeze_jump_inhaling.tick();
        self.breeze_jump_target.tick();
        self.breeze_leaving_water.tick();
    }
}

/// The sensors, in `BRAIN_PROVIDER` order (all scan every 20 ticks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sensor {
    NearestLiving,
    HurtBy,
    NearestPlayers,
    BreezeAttack,
}

/// A breeze's or creaking's brain.
#[derive(Clone, Debug)]
pub struct Brain {
    pub mem: Memories,
    sensors: Vec<(Sensor, i64)>,
    /// The active non-core activity (`CORE` is always active).
    pub activity: Act,
    default_activity: Act,
    list: Vec<Entry>,
}
#[derive(Clone, Debug)]
struct Entry {
    prio: i32,
    act: Act,
    b: B,
}

/// Whether the species thinks with this brain.
pub fn has_brain(s: Species) -> bool {
    matches!(s, Species::Breeze | Species::Creaking)
}


/// What behaviours read and emit during a brain tick.
struct Bx<'a, 'w, 'l> {
    w: &'a MobWorld<'w>,
    level: &'a mut DamageLevel<'l>,
    events: &'a mut Vec<MobEvent>,
    now: i64,
}

fn fight_requirements(b: &Brain) -> bool {
    b.mem.attack_target.present()
        && (b.default_activity != Act::Fight || !b.mem.walk_target.present())
}

// ---- Sensors ------------------------------------------------------------


// ---- Behaviours ---------------------------------------------------------

/// `StartAttacking`'s target finders.
#[derive(Clone, Copy, Debug)]
enum Finder {
    /// The breeze's `NEAREST_ATTACKABLE`.
    NearestAttackable,
    /// The breeze's `HURT_BY` source, if living.
    HurtBy,
    /// The creaking's `NEAREST_VISIBLE_ATTACKABLE_PLAYER`, while active.
    ActiveCreaking,
}

/// The behaviours of both brains.
#[derive(Clone, Debug)]
enum B {
    Swim {
        t: Timed,
        creaking: bool,
    },
    LookAtTargetSink(Timed),
    /// `MoveToTargetSink`, and the breeze's `SlideToTargetSink`.
    MoveToTargetSink {
        t: Timed,
        cooldown: i32,
        path: Option<Path>,
        last_target: Option<[i32; 3]>,
        speed: f32,
        slide: bool,
    },
    StartAttacking(bool, Finder),
    /// `StopAttackingIfTargetInvalid`: the breeze's while the target was
    /// attackable within the last 100 ticks (`positives_left`), the
    /// creaking's while a visible attackable player.
    StopAttacking {
        running: bool,
        breeze: bool,
        positives_left: i32,
    },
    RunOne(Box<RunOne>),
    DoNothing(Timed),
    RandomStroll(bool, f32),
    SetEntityLookTargetSometimes {
        running: bool,
        until_next: i32,
    },
    SetWalkTargetFromLookTarget(bool),
    SetWalkTargetFromAttackTarget(bool),
    MeleeAttack(bool),
    Shoot(Timed),
    LongJump(Timed),
    ShootWhenStuck(Timed),
    Slide(Timed),
}

/// `RunOne`: shuffled by weight, the first that starts runs.
#[derive(Clone, Debug)]
struct RunOne {
    running: bool,
    random: LegacyRandom,
    kids: Vec<(B, i32, f64)>,
}
impl RunOne {
}

impl B {
    fn running(&self) -> bool {
        match self {
            B::Swim { t, .. }
            | B::LookAtTargetSink(t)
            | B::MoveToTargetSink { t, .. }
            | B::DoNothing(t)
            | B::Shoot(t)
            | B::LongJump(t)
            | B::ShootWhenStuck(t)
            | B::Slide(t) => t.running,
            B::StartAttacking(r, _)
            | B::StopAttacking { running: r, .. }
            | B::RandomStroll(r, _)
            | B::SetEntityLookTargetSometimes { running: r, .. }
            | B::SetWalkTargetFromLookTarget(r)
            | B::SetWalkTargetFromAttackTarget(r)
            | B::MeleeAttack(r) => *r,            B::RunOne(g) => g.running,
        }
    }



}

fn manhattan_sq(a: [i32; 3], b: [i32; 3]) -> i64 {
    let d = [
        i64::from(a[0] - b[0]),
        i64::from(a[1] - b[1]),
        i64::from(a[2] - b[2]),
    ];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}
fn same_path(a: Option<&Path>, b: Option<&Path>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.nodes == b.nodes && a.target == b.target,
        _ => false,
    }
}
fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

// ---- The breeze's behaviours --------------------------------------------

/// `Mth.wrapDegrees(float)`.
fn wrap_degrees(angle: f32) -> f32 {
    let mut w = angle % 360.0;
    if w >= 180.0 {
        w -= 360.0;
    }
    if w < -180.0 {
        w += 360.0;
    }
    w
}
/// The attack target's position, eye height, facing and height.
struct Quarry {
    position: Vec3,
    head_yaw: f32,
    height: f64,
    passenger: bool,
}
fn quarry(w: &MobWorld, who: Who) -> Option<Quarry> {
    let t = w.target_info(who)?;
    let head_yaw = match who {
        Who::Player => w.selected_player().map_or(0.0, |p| p.yaw),
        Who::Mob(id) => w.seen(id).map_or(0.0, |s| s.yaw),
    };
    Some(Quarry {
        position: t.position,
        head_yaw,
        height: t.bounds.max_position().y - t.bounds.min_position().y,
        passenger: t.passenger,
    })
}

// ---- The packages -------------------------------------------------------

fn timed(min: i32, max: i32) -> Timed {
    Timed::new(min, max)
}
/// `availableBehaviorsByPriority`'s order: priority, then the activity's
/// hash slot (core 1, idle 5, fight 14), then insertion order.
fn sorted(mut v: Vec<(i32, Act, B)>) -> Vec<Entry> {
    v.sort_by_key(|(p, a, _)| (*p, a.slot()));
    v.into_iter()
        .map(|(prio, act, b)| Entry { prio, act, b })
        .collect()
}

fn run_one(seed: i64, kids: Vec<(B, i32)>) -> B {
    B::RunOne(Box::new(RunOne {
        running: false,
        random: LegacyRandom::new(seed),
        kids: kids.into_iter().map(|(b, w)| (b, w, 0.0)).collect(),
    }))
}
fn move_sink(min: i32, max: i32, slide: bool) -> B {
    B::MoveToTargetSink {
        t: timed(min, max),
        cooldown: 0,
        path: None,
        last_target: None,
        speed: 0.0,
        slide,
    }
}
/// `BreezeAi.getActivities`.
fn breeze_packages(seed: i64) -> Vec<Entry> {
    use Act::*;
    sorted(vec![
        (
            0,
            Core,
            B::Swim {
                t: timed(60, 60),
                creaking: false,
            },
        ),
        (1, Core, B::LookAtTargetSink(timed(45, 90))),
        (0, Idle, B::StartAttacking(false, Finder::NearestAttackable)),
        (1, Idle, B::StartAttacking(false, Finder::HurtBy)),
        (2, Idle, move_sink(20, 40, true)),
        (
            3,
            Idle,
            run_one(
                seed,
                vec![
                    (B::DoNothing(timed(20, 100)), 1),
                    (B::RandomStroll(false, 0.6), 2),
                ],
            ),
        ),
        (
            0,
            Fight,
            B::StopAttacking {
                running: false,
                breeze: true,
                positives_left: 0,
            },
        ),
        (1, Fight, B::Shoot(timed(20, 20))),
        (2, Fight, B::LongJump(timed(200, 200))),
        (3, Fight, B::ShootWhenStuck(timed(60, 60))),
        (4, Fight, B::Slide(timed(60, 60))),
    ])
}
/// `CreakingAi.getActivities`.
fn creaking_packages(seed: i64) -> Vec<Entry> {
    use Act::*;
    sorted(vec![
        (
            0,
            Core,
            B::Swim {
                t: timed(60, 60),
                creaking: true,
            },
        ),
        (1, Core, B::LookAtTargetSink(timed(45, 90))),
        (2, Core, move_sink(150, 250, false)),
        (10, Idle, B::StartAttacking(false, Finder::ActiveCreaking)),
        (
            11,
            Idle,
            B::SetEntityLookTargetSometimes {
                running: false,
                until_next: 0,
            },
        ),
        (
            12,
            Idle,
            run_one(
                seed,
                vec![
                    (B::RandomStroll(false, 0.3), 2),
                    (B::SetWalkTargetFromLookTarget(false), 2),
                    (B::DoNothing(timed(30, 60)), 1),
                ],
            ),
        ),
        (10, Fight, B::SetWalkTargetFromAttackTarget(false)),
        (11, Fight, B::MeleeAttack(false)),
        (
            12,
            Fight,
            B::StopAttacking {
                running: false,
                breeze: false,
                positives_left: 0,
            },
        ),
    ])
}

// ---- The probe's view ---------------------------------------------------

fn who(w: Who) -> String {
    match w {
        Who::Player => "player".into(),
        Who::Mob(id) => format!("e{id}"),
    }
}
fn pos(p: [i32; 3]) -> String {
    format!("{},{},{}", p[0], p[1], p[2])
}
fn tracker(t: Tracker) -> String {
    match t {
        Tracker::Entity(e, _) => who(e),
        Tracker::Block(p) => pos(p),
        Tracker::Point(v) => pos([
            v[0].floor() as i32,
            v[1].floor() as i32,
            v[2].floor() as i32,
        ]),
    }
}
