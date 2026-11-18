//! The behaviours of a hostile monster, written once by behaviour and assigned by data (a species with a `Target` and a
//! `Melee` slot in its `GoalDef`): `NearestAttackableTargetGoal` on the player, `HurtByTargetGoal`, `MeleeAttackGoal`.
//! Event driven like the farm goals: the target is looked for on a geometric deadline (the selector's 1-in-5 ask every
//! other tick, sampled once), the melee goal looks at the target on its 20-tick check grid, and only a monster that
//! has a target, or a melee that runs, is visited by a system. The damage goes through the players' state.
use crate::cow::*;
use crate::goals::{self, *};
use bevy_ecs::prelude::*;
use horizon_core::geometry::{Aabb, Vec3};

/// `MeleeAttackGoal`'s data (from the species: damage, interval, speed) and state.
#[derive(Component, Clone, Copy, Debug)]
pub struct Attack {
    pub damage: f32,
    /// Ticks between two strikes.
    pub interval: i32,
    pub speed: f64,
    /// `Mob.isWithinMeleeAttackRange`: the box widened by this on x and z meets the target's box.
    pub reach: f64,
    /// `TargetGoal.getFollowDistance`: the target is lost beyond it.
    pub follow: f64,
    /// The tick the monster appeared at: the 20-tick grid of the melee goal's `canUse` is relative to it.
    pub phase: u64,
    pub until: i32,
    pub recalc: i32,
    pub path_target: Vec3,
    /// A check of the melee goal is in the timer wheel.
    pub armed: bool,
}
/// The player this monster hunts (the target selector runs; `Target` goal).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Aggro {
    pub player: u32,
}
/// The melee goal runs: its system visits this monster each tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Meleeing;

/// A hit a monster did to a player (the game applies the armour, the effects and the knockback on its side).
#[derive(Clone, Copy, Debug)]
pub struct PlayerHit {
    pub tick: u64,
    pub player: u32,
    pub amount: f32,
    pub from: (f64, f64),
}
/// The players' health (20 until set), and the log of the hits they took. `immortal`: the health never drops below
/// 1 (the benches keep the players alive to measure a long chase).
#[derive(Resource, Default)]
pub struct PlayerState {
    pub health: Vec<f32>,
    pub hits: Vec<PlayerHit>,
    pub immortal: bool,
}
impl PlayerState {
    pub fn health_of(&self, id: u32) -> f32 {
        self.health.get(id as usize).copied().unwrap_or(20.0)
    }
    pub fn alive(&self, id: u32) -> bool {
        self.health_of(id) > 0.0
    }
    fn hurt(&mut self, tick: u64, player: u32, amount: f32, from: (f64, f64)) {
        let i = player as usize;
        if self.health.len() <= i {
            self.health.resize(i + 1, 20.0);
        }
        self.health[i] -= amount;
        if self.immortal {
            self.health[i] = self.health[i].max(1.0);
        }
        self.hits.push(PlayerHit { tick, player, amount, from });
    }
}
/// The monsters hurt by a player this step: (monster, player), written by `apply_damage`.
#[derive(Resource, Default)]
pub struct HurtBy(pub Vec<(Entity, u32)>);

pub fn insert_resources(world: &mut World) {
    world.insert_resource(PlayerState::default());
    world.insert_resource(HurtBy::default());
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems((hurt_by_events, aggro_run, melee_run).chain().after(goals::hurt_events).before(arbitrate));
}

/// The data of a species that attacks (the vanilla values of `Zombie` and its attributes).
pub fn equip(world: &mut World, e: Entity, sp: &Species) {
    if sp.goals.slots.iter().any(|s| s.0 == Kind::Melee) {
        let now = world.resource::<Clock>().0;
        world.entity_mut(e).insert(Attack {
            damage: if sp.attack_damage <= 0.0 { 1.0 } else { sp.attack_damage },
            interval: 20,
            speed: 1.0,
            reach: 2.04_f32.sqrt() as f64 - 0.6,
            follow: f64::from(sp.follow_range),
            phase: now,
            until: 0,
            recalc: 0,
            path_target: Vec3::ZERO,
            armed: false,
        });
    }
}

// ---- the target goal --------------------------------------------------------------------------------------------
/// `NearestAttackableTargetGoal.canUse` once the 1-in-5 draw passed: the nearest living player within the follow range
/// (at least 2 blocks). The line of sight is not computed (the bench world is open).
pub fn pick_target(w: &mut World, e: Entity) -> Option<u32> {
    let pos = w.get::<Pos>(e)?.0;
    let a = *w.get::<Attack>(e)?;
    let range = a.follow.max(2.0);
    let mut best: Option<(u32, f64)> = None;
    let state = w.resource::<PlayerState>();
    w.resource::<Online>().for_near(pos.x, pos.z, range, |p| {
        let d = d2(pos, Vec3::new(p.x, p.y, p.z));
        if d <= range * range && state.alive(p.id) && best.is_none_or(|b| d < b.1) {
            best = Some((p.id, d));
        }
    });
    best.map(|b| b.0)
}
/// The target is acquired: the melee goal will look at it on its next check.
pub fn acquire(w: &mut World, e: Entity, player: u32) {
    w.entity_mut(e).insert(Aggro { player });
    arm_melee(w, e);
}
/// `TargetGoal.stop`: no target; the melee goal ends and the selector asks again from a fresh draw.
pub fn lose_target(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<Aggro>();
    w.resource_mut::<Requests>().0.push(Req::End(e, Kind::Melee));
    if let Some(mut t) = w.get_mut::<GoalTimers>(e) {
        t.target = false;
    }
    arm_timers_of(w, e);
}
fn arm_timers_of(w: &mut World, e: Entity) {
    w.resource_mut::<Requests>().0.push(Req::Arm(e));
}
/// A hit by a player (`HurtByTargetGoal`, priority 1 over the nearest-player goal): the hitter becomes the target.
pub fn hunt(w: &mut World, e: Entity) {
    let Some(mut g) = w.get_mut::<GoalSet>(e) else { return };
    if !g.is_running(Kind::Target) {
        g.start(Kind::Target);
    }
    arm_melee(w, e);
}
fn hurt_by_events(mut by: ResMut<HurtBy>, ps: Res<PlayerState>, q: Query<(), With<Attack>>, mut reqs: ResMut<Requests>, mut cmd: Commands) {
    for (e, p) in std::mem::take(&mut by.0) {
        if q.get(e).is_ok() && ps.alive(p) {
            cmd.entity(e).insert(Aggro { player: p });
            reqs.0.push(Req::Hunt(e));
        }
    }
}
/// `TargetGoal.canContinueToUse`, asked every other tick of the monsters that have a target: alive and within the
/// follow range of the monster, else the target is lost.
fn aggro_run(clock: Res<Clock>, on: Res<Online>, ps: Res<PlayerState>, q: Query<(Entity, &Pos, &Aggro, &Attack)>, mut reqs: ResMut<Requests>) {
    if clock.0 % 2 != 0 {
        return;
    }
    for (e, pos, g, a) in &q {
        let ok = on.get(g.player).is_some_and(|p| ps.alive(p.id) && d2(pos.0, Vec3::new(p.x, p.y, p.z)) <= a.follow * a.follow);
        if !ok {
            reqs.0.push(Req::End(e, Kind::Target));
        }
    }
}

// ---- the melee goal ---------------------------------------------------------------------------------------------
/// `MeleeAttackGoal.canUse` runs only on a 20-tick grid: the next check after now is armed in the wheel.
pub fn arm_melee(w: &mut World, e: Entity) {
    let now = w.resource::<Clock>().0;
    let Some(mut a) = w.get_mut::<Attack>(e) else { return };
    if a.armed {
        return;
    }
    a.armed = true;
    let wait = 20 - (now.saturating_sub(a.phase) % 20) as i32;
    arm(w, e, Kind::Melee, wait);
}
/// `canUse`: a living target and a path to it or within reach (the path is made by the pathfinder at the start; the
/// case of no path is not fed back).
pub fn melee_target(w: &mut World, e: Entity) -> Option<u32> {
    let g = *w.get::<Aggro>(e)?;
    w.resource::<PlayerState>().alive(g.player).then_some(g.player)
}
/// `MeleeAttackGoal.start`: a path to the target, the cooldown and the repath timer reset.
pub fn start_melee(w: &mut World, e: Entity, player: u32) {
    let Some(p) = w.resource::<Online>().get(player) else { return };
    let speed = {
        let mut a = w.get_mut::<Attack>(e).unwrap();
        (a.recalc, a.until, a.path_target) = (0, 0, Vec3::ZERO);
        a.speed
    };
    w.entity_mut(e).insert(Meleeing);
    if let Some(mut s) = w.get_mut::<Stroll>(e) {
        (s.tx, s.tz) = (p.x, p.z);
    }
    w.get_mut::<Nav>(e).unwrap().speed = speed;
    w.entity_mut(e).insert(NeedsPath);
}
/// `MeleeAttackGoal.stop`: the path is dropped; the next check is on the grid.
pub fn stop_melee(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<Meleeing>();
    stop_nav(w, e);
    if w.get::<Aggro>(e).is_some() {
        arm_melee(w, e);
    }
}
fn player_box(p: &Player) -> Aabb {
    Aabb::new(Vec3::new(p.x - 0.3, p.y, p.z - 0.3), Vec3::new(p.x + 0.3, p.y + 1.8, p.z + 0.3))
}
/// `MeleeAttackGoal.tick` (every tick): look at the target, ask for a new path every 4 to 10 ticks (plus 5 or 10 when
/// far) when the target moved a block or by 1 chance in 20, and strike when the cooldown is over and the target is
/// within reach.
#[allow(clippy::type_complexity)]
fn melee_run(
    clock: Res<Clock>,
    on: Res<Online>,
    mut ps: ResMut<PlayerState>,
    mut q: Query<(Aim, &Bounds, &mut Attack, &Aggro, &mut Rng), With<Meleeing>>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, _following, _asked), bounds, mut a, g, mut rng) in &mut q {
        let Some(p) = on.get(g.player).filter(|p| ps.alive(p.id)) else {
            reqs.0.push(Req::End(e, Kind::Melee));
            continue;
        };
        let at = Vec3::new(p.x, p.y, p.z);
        aim(&mut cmd, e, &mut look, looking, [p.x, p.eye(), p.z], Some(30.0));
        a.recalc = (a.recalc - 1).max(0);
        let moved = d2(at, a.path_target) >= 1.0;
        if a.recalc <= 0 && (a.path_target == Vec3::ZERO || moved || rng.next_f64() < 0.05) {
            a.path_target = at;
            a.recalc = 4 + rng.next_int(7);
            let d = d2(pos.0, at);
            if d > 1024.0 {
                a.recalc += 10;
            } else if d > 256.0 {
                a.recalc += 5;
            }
            go_to(&mut cmd, e, p.x, p.z, a.speed);
        }
        a.until = (a.until - 1).max(0);
        if a.until <= 0 && bounds.0.inflate(Vec3::new(a.reach, 0.0, a.reach)).intersects(player_box(&p)) {
            a.until = a.interval;
            ps.hurt(clock.0, p.id, a.damage, (pos.0.x, pos.0.z));
        }
    }
}
