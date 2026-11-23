//! The mod goals and behaviours of the ECS run by the game: each hook runs its mod under the panic guard, with a `GoalContext` or a
//! `BehaviorCx` whose calls act on the entity's world (`horizon_ecs_proto::mod_goals`) and whose reads are the step's view (`ecs_view`)
//! over the game host. `attach` adds what a mod's entity part registers for a species: goals to the selector, behaviours to the entity's
//! `ModBehaviors`.
use bevy_ecs::{entity::Entity, world::World};
use horizon_ecs_proto::{
    brain::{ATTACK_TARGET, LOOK_TARGET, MemKey, Need, WALK_TARGET},
    goals::TARGET,
    mod_goals::{self, ModEntry, ModGoalHost, NewBehaviour, NewGoal},
    targeting::Who,
};
use mod_api::Activity;
use mod_api::Behavior;
use mod_api::BehaviorCx;
use mod_api::BehaviorCxBackend;
use mod_api::BehaviorRefusal;
use mod_api::BehaviorRegistrar;
use mod_api::BehaviorRegistrarBackend;
use mod_api::BrainMemory;
use mod_api::BrainWalkTarget;
use mod_api::EntityRef;
use mod_api::Goal;
use mod_api::GoalCxBackend;
use mod_api::GoalRegistrarBackend;
use mod_api::Host;
use myriade_types::LegacyRandom;
use mod_api::LivingRef;
use mod_api::MemoryStatus;
use mod_api::MobRef;
use myriade_types::MoveToPathError;
use myriade_types::PathNode;
use mod_api::RegisteredBehavior;
use myriade_types::WorldEpoch;
use mod_api::content::crash::ModCall;
use mod_api::content::crash::enter_native_mod;

/// The host of the mod goals of one step: `None` epoch (no native attempt) refuses every call; `game` answers the reads the step holds no data for.
pub struct ModGoalsHost<'g> {
    pub epoch: Option<WorldEpoch>,
    /// The level of this ECS (`Entity.level`): its epoch and dimension.
    pub level: Option<mod_api::LevelRef>,
    pub game: &'g dyn Host,
}

impl ModGoalHost for ModGoalsHost<'_> {
    fn can_use(&self, w: &mut World, e: Entity, goal: &mut ModEntry) -> bool {
        self.run(w, e, goal, "goal_can_use", |g, cx| g.can_use(cx)).unwrap_or(false)
    }
    fn start(&self, w: &mut World, e: Entity, goal: &mut ModEntry) {
        self.run(w, e, goal, "goal_start", |g, cx| g.start(cx));
    }
    fn tick(&self, w: &mut World, e: Entity, goal: &mut ModEntry) {
        self.run(w, e, goal, "goal_tick", |g, cx| g.tick(cx));
    }
    fn stop(&self, w: &mut World, e: Entity, goal: &mut ModEntry) {
        self.run(w, e, goal, "goal_stop", |g, cx| g.stop(cx));
    }
    fn behaviour_check(&self, w: &mut World, e: Entity, b: &mut ModEntry) -> bool {
        self.run_behaviour(w, e, b, "behaviour_check", |x, cx| x.check_extra_start_conditions(cx)).unwrap_or(false)
    }
    fn behaviour_start(&self, w: &mut World, e: Entity, b: &mut ModEntry) {
        self.run_behaviour(w, e, b, "behaviour_start", |x, cx| x.start(cx));
    }
    fn behaviour_can_still_use(&self, w: &mut World, e: Entity, b: &mut ModEntry) -> bool {
        self.run_behaviour(w, e, b, "behaviour_can_still_use", |x, cx| x.can_still_use(cx)).unwrap_or(false)
    }
    fn behaviour_tick(&self, w: &mut World, e: Entity, b: &mut ModEntry) {
        self.run_behaviour(w, e, b, "behaviour_tick", |x, cx| x.tick(cx));
    }
    fn behaviour_stop(&self, w: &mut World, e: Entity, b: &mut ModEntry) {
        self.run_behaviour(w, e, b, "behaviour_stop", |x, cx| x.stop(cx));
    }
}

impl ModGoalsHost<'_> {
    /// Runs `f` on the goal of `goal` under `hook`; None when the epoch or the entity's network id is unknown, or the goal is not a `Goal`.
    fn run<R>(
        &self,
        w: &mut World,
        e: Entity,
        goal: &mut ModEntry,
        hook: &'static str,
        f: impl FnOnce(&mut dyn Goal, &mut mod_api::GoalContext<'_>) -> R,
    ) -> Option<R> {
        let epoch = self.epoch?;
        let id = mod_goals::id_of(w, e)?;
        let g = goal.goal.downcast_mut::<Box<dyn Goal>>()?;
        let mut back = EcsCx::new(w, e, epoch, self.level, id, self.game);
        let mut cx = mod_api::GoalContext::new(&mut back);
        let _guard = enter_native_mod(ModCall { id: goal.mod_id, hook, part: Some(goal.part) });
        Some(f(&mut **g, &mut cx))
    }
    /// `run` for a behaviour: its `BehaviorCx` acts on the entity's brain.
    fn run_behaviour<R>(
        &self,
        w: &mut World,
        e: Entity,
        behaviour: &mut ModEntry,
        hook: &'static str,
        f: impl FnOnce(&mut dyn Behavior, &mut BehaviorCx<'_>) -> R,
    ) -> Option<R> {
        let epoch = self.epoch?;
        let id = mod_goals::id_of(w, e)?;
        let b = behaviour.goal.downcast_mut::<Box<dyn Behavior>>()?;
        let mut back = EcsCx::new(w, e, epoch, self.level, id, self.game);
        let mut cx = BehaviorCx::new(&mut back);
        let _guard = enter_native_mod(ModCall { id: behaviour.mod_id, hook, part: Some(behaviour.part) });
        Some(f(&mut **b, &mut cx))
    }
}

/// The one backend of a mod call on an entity, goal or behaviour: the entity's handle and the world it acts on, and the host the
/// step's view reads through (`ecs_view`) for what the step does not hold.
pub struct EcsCx<'a> {
    pub(super) w: &'a mut World,
    pub(super) e: Entity,
    pub(super) epoch: WorldEpoch,
    pub(super) level: Option<mod_api::LevelRef>,
    pub(super) id: u32,
    rng: LegacyRandom,
    pub(super) host: &'a dyn Host,
}

impl<'a> EcsCx<'a> {
    pub(super) fn new(w: &'a mut World, e: Entity, epoch: WorldEpoch, level: Option<mod_api::LevelRef>, id: u32, host: &'a dyn Host) -> Self {
        Self { w, e, epoch, level, id, rng: LegacyRandom::new(i64::from(id)), host }
    }
}

impl GoalCxBackend for EcsCx<'_> {
    fn host(&self) -> &dyn Host {
        self
    }
    fn mob(&self) -> MobRef {
        MobRef::new(EntityRef::new(self.epoch, self.id))
    }
    /// The entity's own source (`mod_goals::GoalRandom`, seeded from its generator when its first goal was added); the id-seeded one
    /// only answers an entity that carries none.
    fn random(&mut self) -> &mut LegacyRandom {
        let Some(mut own) = self.w.get_mut::<mod_goals::GoalRandom>(self.e) else { return &mut self.rng };
        &mut own.into_inner().0
    }
    fn move_to(&mut self, at: [f64; 3], speed: f64) -> bool {
        mod_goals::move_to(self.w, self.e, at, speed)
    }
    fn move_to_path(&mut self, path: Vec<PathNode>, speed: f64) -> Result<(), MoveToPathError> {
        if !path_accepts(&path, speed) {
            return Err(MoveToPathError::NotFinite);
        }
        let nodes: Vec<[f64; 3]> = path.iter().map(|n| [n.x, n.y, n.z]).collect();
        path_refusal(mod_goals::move_to_path(self.w, self.e, &nodes, speed))
    }
    fn stop_navigation(&mut self) {
        mod_goals::stop_navigation(self.w, self.e);
    }
    fn look_at(&mut self, at: [f64; 3]) {
        mod_goals::look_at(self.w, self.e, at);
    }
    fn set_target(&mut self, target: Option<LivingRef>) -> bool {
        match target {
            None => mod_goals::set_target(self.w, self.e, None),
            Some(t) => match mod_goals::living_who(self.w, t.entity().id()) {
                Some(who) => mod_goals::set_target(self.w, self.e, Some(who)),
                None => false,
            },
        }
    }
    fn jump(&mut self) {
        mod_goals::jump(self.w, self.e);
    }
}

/// The numbers a mod hands `PathNavigation.moveTo(Path, speed)` must be finite (the handle's `supplied_path_accepts`).
fn path_accepts(nodes: &[PathNode], speed: f64) -> bool {
    speed.is_finite() && nodes.iter().all(|n| n.x.is_finite() && n.y.is_finite() && n.z.is_finite())
}

/// A path with no node left is refused (the handle's `supplied_path_refusal`).
fn path_refusal(followed: bool) -> Result<(), MoveToPathError> {
    if followed { Ok(()) } else { Err(MoveToPathError::Empty) }
}

/// The living handle a target names, in the goal's epoch.
fn living_ref(epoch: WorldEpoch, who: Who, w: &World) -> Option<LivingRef> {
    let id = match who {
        Who::Entity(e) => mod_goals::id_of(w, e)?,
        Who::Player(p) => p,
    };
    Some(LivingRef::new(EntityRef::new(epoch, id)))
}

impl BehaviorCxBackend for EcsCx<'_> {
    fn host(&self) -> &dyn Host {
        self
    }
    fn mob(&self) -> MobRef {
        MobRef::new(EntityRef::new(self.epoch, self.id))
    }
    fn walk_target(&self) -> Option<BrainWalkTarget> {
        let (at, speed, close) = mod_goals::walk_target(self.w, self.e)?;
        Some(BrainWalkTarget::new(at, speed, close))
    }
    fn look_target(&self) -> Option<[f64; 3]> {
        mod_goals::look_target(self.w, self.e)
    }
    fn attack_target(&self) -> Option<LivingRef> {
        living_ref(self.epoch, mod_goals::attack_target(self.w, self.e)?, self.w)
    }
    fn set_brain_target(&mut self, target: Option<LivingRef>) -> bool {
        let who = match target {
            None => None,
            Some(t) => match mod_goals::living_who(self.w, t.entity().id()) {
                Some(who) => Some(who),
                None => return false,
            },
        };
        mod_goals::set_brain_target(self.w, self.e, who)
    }
    fn set_walk_target(&mut self, at: [i32; 3], speed: f64, close: i32) {
        mod_goals::set_walk_target(self.w, self.e, at, speed, close);
    }
}

/// The side of `GoalRegistrar` for one entity: each goal goes to the entity's selector.
struct RegistrarFor<'a> {
    w: &'a mut World,
    e: Entity,
    kind: &'a str,
    mod_id: &'static str,
    part: &'static str,
}

impl RegistrarFor<'_> {
    fn add(&mut self, priority: i32, flags: u8, goal: Box<dyn Goal>) {
        let goal_flags = goal.flags().bits() | flags;
        let new = NewGoal {
            priority: priority.clamp(0, 255) as u8,
            flags: goal_flags,
            interruptable: goal.is_interruptable(),
            every_tick: goal.requires_update_every_tick(),
            mod_id: self.mod_id,
            part: self.part,
            goal: Box::new(goal),
        };
        mod_goals::add_goal(self.w, self.e, new);
    }
}

impl GoalRegistrarBackend for RegistrarFor<'_> {
    fn add_goal(&mut self, priority: i32, goal: Box<dyn Goal>) {
        self.add(priority, 0, goal);
    }
    fn add_target_goal(&mut self, priority: i32, goal: Box<dyn Goal>) {
        self.add(priority, TARGET, goal);
    }
    fn mob_id(&self) -> &str {
        self.kind
    }
}

/// The side of `BehaviorRegistrar` for one entity: each behaviour goes to the entity's `ModBehaviors`.
struct BehaviourFor<'a> {
    w: &'a mut World,
    e: Entity,
    kind: &'a str,
}

impl BehaviorRegistrarBackend for BehaviourFor<'_> {
    fn add_behavior(&mut self, r: RegisteredBehavior) -> Result<(), BehaviorRefusal> {
        // `Registered` is the species' declaration, checked here: an undeclared memory is never present, so the behaviour never starts.
        let needs = r
            .entry
            .iter()
            .filter_map(|&(m, s)| {
                let k = memory_key(m);
                match s {
                    MemoryStatus::Registered if horizon_ecs_proto::brain::declares(self.w, self.e, k) => None,
                    MemoryStatus::Registered => Some((k, Need::Present)),
                    _ => Some((k, need(s)?)),
                }
            })
            .collect();
        let new = NewBehaviour {
            activity: activity_name(&r.activity),
            priority: r.priority,
            duration: Some((r.min, r.max)),
            needs,
            mod_id: r.mod_id,
            part: r.part,
            behaviour: Box::new(r.behavior),
        };
        if mod_goals::add_behaviour(self.w, self.e, new) {
            return Ok(());
        }
        if self.w.get::<horizon_ecs_proto::brain::Brain>(self.e).is_none() {
            return Err(BehaviorRefusal::NoBrain);
        }
        Err(BehaviorRefusal::UnknownActivity(r.activity))
    }
    fn mob_id(&self) -> &str {
        self.kind
    }
}

/// The brain memory a mod's entry condition names.
fn memory_key(m: BrainMemory) -> &'static MemKey {
    match m {
        BrainMemory::WalkTarget => &WALK_TARGET,
        BrainMemory::LookTarget => &LOOK_TARGET,
        BrainMemory::AttackTarget => &ATTACK_TARGET,
    }
}
/// The need of a memory status. `Registered` (the brain keeps the memory at all) asks nothing here: the caller checks it against the
/// memories the species declares (`brain::declares`).
fn need(s: MemoryStatus) -> Option<Need> {
    match s {
        MemoryStatus::Present => Some(Need::Present),
        MemoryStatus::Absent => Some(Need::Absent),
        MemoryStatus::Registered => None,
    }
}
/// The brain activity's name, as the species' brain names it (`BrainDef`'s activities).
fn activity_name(a: &Activity) -> &'static str {
    match a {
        Activity::Core => "core",
        Activity::Idle => "idle",
        Activity::Work => "work",
        Activity::Play => "play",
        Activity::Rest => "rest",
        Activity::Meet => "meet",
        Activity::Panic => "panic",
        Activity::Raid => "raid",
        Activity::PreRaid => "pre_raid",
        Activity::Hide => "hide",
        Activity::Fight => "fight",
        Activity::PlayDead => "play_dead",
        Activity::Sniff => "sniff",
        Activity::Dig => "dig",
        Activity::Emerge => "emerge",
        Activity::Roar => "roar",
        Activity::Investigate => "investigate",
    }
}

/// Adds the goals and behaviours the mods' entity parts register for the species `kind` to the entity `e`.
pub fn attach(w: &mut World, e: Entity, kind: &str) {
    if !mod_api::__private::has_entity_parts() {
        return;
    }
    let parts = mod_api::__private::entity_parts();
    for attached in parts.entity_parts(kind) {
        let _guard = enter_native_mod(ModCall { id: attached.mod_id(), hook: "register_goals", part: Some(attached.type_name()) });
        let mut registrar = RegistrarFor { w: &mut *w, e, kind, mod_id: attached.mod_id(), part: attached.type_name() };
        attached.part().register_goals(&mut mod_api::GoalRegistrar::new(&mut registrar));
        let mut behaviours = BehaviourFor { w: &mut *w, e, kind };
        let mut registrar = BehaviorRegistrar::new(&mut behaviours, kind, attached.mod_id(), attached.type_name());
        attached.part().register_behaviors(&mut registrar);
    }
}
