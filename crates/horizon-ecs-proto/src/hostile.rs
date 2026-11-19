//! The data and rules of a hostile monster, shared by the `target` and `melee` modules (a species with those entries in
//! its configuration): `NearestAttackableTargetGoal` on the player, `HurtByTargetGoal`, `MeleeAttackGoal`.
//! Event driven like the farm goals: the target is looked for on a geometric deadline (the selector's 1-in-5 ask every
//! other tick, sampled once), the melee goal looks at the target on its 20-tick check grid, and only a monster that
//! has a target, or a melee that runs, is visited by a system. The damage goes through the players' state.
use crate::cow::*;
use crate::goals::{self, *};
use bevy_ecs::prelude::*;
use bevy_ecs::system::ParamSet;
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
    /// The height of the eyes (the line of sight starts there).
    pub eye: f64,
    /// The tick the monster appeared at: the 20-tick grid of the melee goal's `canUse` is relative to it.
    pub phase: u64,
    pub until: i32,
    pub recalc: i32,
    pub path_target: Vec3,
    /// A check of the melee goal is in the timer wheel.
    pub armed: bool,
    /// The ranks of the melee and target goals in the animal's selector (`u8::MAX`: none).
    pub melee_slot: u8,
    pub target_slot: u8,
}
/// The player this monster hunts (the target selector runs; `Target` goal).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Aggro {
    pub player: u32,
    /// `Mob.sees(target)` as last read (`line_of_sight_for`); re-read by `sight_eval` only when an eye changes block or a
    /// block changes on or around the segment.
    pub seen: bool,
    /// The eye blocks the sight was read for (the monster's, the player's).
    zblock: (i32, i32, i32),
    pblock: (i32, i32, i32),
    /// Evaluations (every other tick) without sight, and how many `TargetGoal.unseenMemoryTicks` allows (30 for the
    /// nearest-player goal, 150 for the hurt-by goal).
    unseen: u16,
    memory: u16,
}
impl Aggro {
    pub fn new(player: u32, memory: u16) -> Self {
        Self { player, seen: false, zblock: (i32::MIN, 0, 0), pblock: (i32::MIN, 0, 0), unseen: 0, memory }
    }
}
/// The blocks that changed this tick (`block_changes` writes, `sight_eval` reads and clears).
#[derive(Resource, Default)]
pub struct ChangedBlocks(pub Vec<(i32, i32, i32)>);
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
    world.insert_resource(ChangedBlocks::default());
    world.insert_resource(Daylight::default());
    world.insert_resource(SunDirty::default());
    world.insert_resource(SunFired::default());
    world.insert_resource(PlayerState::default());
    world.insert_resource(HurtBy::default());
}
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems((sun_eval, sun_burn).chain().after(crate::cow::reindex));
}
/// The systems of the target goal (module `target`): the hits that name a target, the line of sight, `canContinue`.
pub fn target_systems(s: &mut bevy_ecs::schedule::Schedule, slot: GoalSlot) {
    s.add_systems((hurt_by_events, sight_eval, aggro_run).chain().after(apply_damage).in_set(slot));
}
/// The system of the melee goal (module `melee`), after the target's.
pub fn melee_systems(s: &mut bevy_ecs::schedule::Schedule, slot: GoalSlot) {
    s.add_systems(melee_run.in_set(slot));
}

/// The data of a species that attacks (the vanilla values of `Zombie` and its attributes).
pub fn equip(world: &mut World, e: Entity, sp: &Species) {
    if sp.burns_in_day {
        world.entity_mut(e).insert(BurnsInDay { eye: sp.eye_height, block: (i32::MIN, 0, 0) });
        world.resource_mut::<SunDirty>().0.push(e);
    }
}
/// The monster's `Attack` data from its species (made once, by whichever of the target and melee modules is equipped first).
pub fn ensure_attack(w: &mut World, e: Entity) {
    if w.get::<Attack>(e).is_some() {
        return;
    }
    let sp = w.resource::<SpeciesTable>().0[w.get::<SpeciesId>(e).unwrap().0 as usize];
    // The melee check's grid is in the WORLD's time (`MeleeAttackGoal.lastCanUseCheck` against `getGameTime`).
    let now = w.resource::<Clock>().0;
    w.entity_mut(e).insert(Attack {
        damage: if sp.attack_damage <= 0.0 { 1.0 } else { sp.attack_damage },
        interval: 20,
        speed: 1.0,
        reach: 2.04_f32.sqrt() as f64 - 0.6,
        follow: f64::from(sp.follow_range),
        eye: sp.eye_height,
        phase: now,
        until: 0,
        recalc: 0,
        path_target: Vec3::ZERO,
        armed: false,
        melee_slot: u8::MAX,
        target_slot: u8::MAX,
    });
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
    let mw = w.resource::<Level>().mw.0;
    // `TargetingConditions.forCombat` always checks the line of sight, here for the candidates in range only.
    w.resource::<Online>().for_near(pos.x, pos.z, range, |p| {
        let d = d2(pos, Vec3::new(p.x, p.y, p.z));
        if d <= range * range
            && state.alive(p.id)
            && best.is_none_or(|b| d < b.1)
            && mw.line_of_sight_for(Some(pos.y), Vec3::new(pos.x, pos.y + a.eye, pos.z), Vec3::new(p.x, p.eye(), p.z))
        {
            best = Some((p.id, d));
        }
    });
    best.map(|b| b.0)
}
/// The target is acquired: the melee goal will look at it on its next check.
pub fn acquire(w: &mut World, e: Entity, player: u32) {
    w.entity_mut(e).insert(Aggro::new(player, 30));
    arm_melee(w, e);
}
/// `TargetGoal.stop`: no target; the melee goal ends and the selector asks again from a fresh draw.
pub fn lose_target(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<Aggro>();
    if let Some(a) = w.get::<Attack>(e) {
        let slot = a.melee_slot;
        w.resource_mut::<Requests>().0.push(Req::End(e, slot));
    }
    crate::behaviours::target::unarm(w, e);
    arm_timers_of(w, e);
}
fn arm_timers_of(w: &mut World, e: Entity) {
    w.resource_mut::<Requests>().0.push(Req::Arm(e));
}
/// A hit by a player (`HurtByTargetGoal`, priority 1 over the nearest-player goal): the hitter becomes the target.
pub fn hunt(w: &mut World, e: Entity) {
    let Some(slot) = w.get::<Attack>(e).map(|a| a.target_slot) else { return };
    let Some(mut g) = w.get_mut::<GoalSet>(e) else { return };
    if !g.slot_running(slot) {
        g.start(slot);
    }
    arm_melee(w, e);
}
fn hurt_by_events(
    by: ResMut<HurtBy>,
    ps: Res<PlayerState>,
    idx: Res<Index>,
    q: Query<(&Pos, &SpeciesId, Has<Aggro>, &Attack)>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    let mut by = by;
    for (e, p) in std::mem::take(&mut by.0) {
        let Ok((pos, sp, _, a)) = q.get(e) else { continue };
        if !ps.alive(p) {
            continue;
        }
        cmd.entity(e).insert(Aggro::new(p, 150));
        reqs.0.push(Req::Call(e, hunt));
        // `HurtByTargetGoal.alertOthers`: the others of the same type within the follow range (10 blocks up and down)
        // that have no target take the same one.
        idx.for_radius(pos.0.x, pos.0.z, a.follow, |o| {
            if o == e {
                return;
            }
            if let Ok((op, osp, has, _)) = q.get(o)
                && osp == sp
                && !has
                && (op.0.x - pos.0.x).abs() <= a.follow + 1.0
                && (op.0.z - pos.0.z).abs() <= a.follow + 1.0
                && (op.0.y - pos.0.y).abs() <= 11.0
            {
                cmd.entity(o).insert(Aggro::new(p, 30));
                reqs.0.push(Req::Call(o, hunt));
            }
        });
    }
}
/// `TargetGoal.canContinueToUse`, asked every other tick of the monsters that have a target: alive and within the
/// follow range of the monster, else the target is lost.
fn aggro_run(clock: Res<Clock>, on: Res<Online>, ps: Res<PlayerState>, mut q: Query<(Entity, &Pos, &mut Aggro, &Attack)>, mut reqs: ResMut<Requests>) {
    for (e, pos, mut g, a) in &mut q {
        // Every other tick of the monster's own time.
        if clock.of(e) % 2 != 0 {
            continue;
        }
        let ok = on.get(g.player).is_some_and(|p| ps.alive(p.id) && d2(pos.0, Vec3::new(p.x, p.y, p.z)) <= a.follow * a.follow);
        // `TargetGoal.canContinueToUse`: a target out of sight is kept for `unseenMemoryTicks` evaluations.
        if g.seen {
            g.unseen = 0;
        } else {
            g.unseen += 1;
        }
        if !ok || g.unseen > g.memory {
            reqs.0.push(Req::End(e, a.target_slot));
        }
    }
}
/// `Mob.sees` for the monsters that hunt: the line of sight is read again only when the monster's eyes or the player's
/// change block, or a block changes on the box of the segment between them (inflated by a block); a monster that stands
/// still, with a still player and still blocks, costs a comparison.
fn sight_eval(
    level: Res<Level>,
    on: Res<Online>,
    mut changed: ResMut<ChangedBlocks>,
    mut q: Query<(&Pos, &mut Aggro, &Attack)>,
) {
    let mw = level.mw.0;
    for (pos, mut g, a) in &mut q {
        let Some(p) = on.get(g.player) else { continue };
        let (zb, pb) = (eye_block(pos.0, a.eye), eye_block(Vec3::new(p.x, p.y, p.z), 1.62));
        let mut dirty = zb != g.zblock || pb != g.pblock;
        if !dirty && !changed.0.is_empty() {
            let (lo, hi) = ((zb.0.min(pb.0) - 1, zb.1.min(pb.1) - 1, zb.2.min(pb.2) - 1), (zb.0.max(pb.0) + 1, zb.1.max(pb.1) + 1, zb.2.max(pb.2) + 1));
            dirty = changed.0.iter().any(|c| (lo.0..=hi.0).contains(&c.0) && (lo.1..=hi.1).contains(&c.1) && (lo.2..=hi.2).contains(&c.2));
        }
        if dirty {
            (g.zblock, g.pblock) = (zb, pb);
            g.seen = mw.line_of_sight_for(Some(pos.0.y), Vec3::new(pos.0.x, pos.0.y + a.eye, pos.0.z), Vec3::new(p.x, p.eye(), p.z));
        }
    }
    changed.0.clear();
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
    let a_slot = a.melee_slot;
    let wait = 20 - (now.saturating_sub(a.phase) % 20) as i32;
    let slot = a_slot;
    arm(w, e, slot, wait);
}
/// The goal ended or its check failed: `canUse` is asked again at the next selection tick if the last ask is 20 ticks old
/// (the case when the goal ran for a long while: it restarts at once, with a cooldown of 0), else 20 ticks after it.
pub fn arm_melee_after(w: &mut World, e: Entity) {
    let now = w.resource::<Clock>().0;
    let Some(mut a) = w.get_mut::<Attack>(e) else { return };
    if a.armed {
        return;
    }
    a.armed = true;
    let a_slot = a.melee_slot;
    let wait = if now >= a.phase + 20 { 1 } else { (a.phase + 20 - now) as i32 };
    let slot = a_slot;
    arm(w, e, slot, wait);
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
    {
        let mut nav = w.get_mut::<Nav>(e).unwrap();
        nav.speed = speed;
        // `createPath(entity, 0)`: the path goes to the target's block itself.
        nav.reach = 0;
    }
    w.entity_mut(e).insert(NeedsPath);
}
/// `MeleeAttackGoal.stop`: the path is dropped; the next check is on the grid.
pub fn stop_melee(w: &mut World, e: Entity) {
    w.entity_mut(e).remove::<Meleeing>();
    if let Some(mut nav) = w.get_mut::<Nav>(e) {
        nav.reach = 1;
    }
    stop_nav(w, e);
    if w.get::<Aggro>(e).is_some() {
        arm_melee_after(w, e);
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
            reqs.0.push(Req::End(e, a.melee_slot));
            continue;
        };
        let at = Vec3::new(p.x, p.y, p.z);
        aim(&mut cmd, e, &mut look, looking, [p.x, p.eye(), p.z], Some(30.0));
        a.recalc = (a.recalc - 1).max(0);
        let moved = d2(at, a.path_target) >= 1.0;
        // `follow_unseen` is false for the zombie: no new path while the target is not in sight.
        if g.seen && a.recalc <= 0 && (a.path_target == Vec3::ZERO || moved || rng.next_f64() < 0.05) {
            a.path_target = at;
            a.recalc = 4 + rng.next_int(7);
            let d = d2(pos.0, at);
            if d > 1024.0 {
                a.recalc += 10;
            } else if d > 256.0 {
                a.recalc += 5;
            }
            go_to(&mut cmd, e, p.x, p.z, a.speed, 5);
        }
        a.until = (a.until - 1).max(0);
        if a.until <= 0 && g.seen && bounds.0.inflate(Vec3::new(a.reach, 0.0, a.reach)).intersects(player_box(&p)) {
            a.until = a.interval;
            ps.hurt(clock.0, p.id, a.damage, (pos.0.x, pos.0.z));
        }
    }
}

// ---- burning in daylight ----------------------------------------------------------------------------------------
/// `monsters_burn`: the sun is up (the level's day, set by the game).
#[derive(Resource, Default)]
pub struct Daylight(pub bool);
/// Monsters that must look at the sun again: the day came or went, a block changed in their column.
#[derive(Resource, Default)]
pub struct SunDirty(pub Vec<Entity>);
/// Monsters whose `Due::Sun` draw came up this tick.
#[derive(Resource, Default)]
pub struct SunFired(pub Vec<Entity>);
/// A species that burns in daylight (`burns_in_day`): the eye height and the eye block last looked at.
#[derive(Component, Clone, Copy, Debug)]
pub struct BurnsInDay {
    eye: f64,
    block: (i32, i32, i32),
}
/// Daylight, sky seen from the eyes and light enough: each tick the monster burns with probability
/// `(brightness - 0.4) * 2 / 30` (`Mob.isSunBurnTick`); one draw is armed in the wheel, its tick is `token`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Exposed {
    token: u64,
}
fn eye_block(p: Vec3, eye: f64) -> (i32, i32, i32) {
    (p.x.floor() as i32, (p.y + eye).floor() as i32, p.z.floor() as i32)
}
/// The probability per tick of a burn at this block, 0 when the light is too low or the sky is not seen.
fn burn_chance(level: &Level, day: bool, b: (i32, i32, i32)) -> f64 {
    let mw = level.mw.0;
    if !day || !mw.can_see_sky(b.0, b.1, b.2) {
        return 0.0;
    }
    let br = f64::from(mw.brightness(b.0, b.1, b.2));
    if br > 0.5 { ((br - 0.4) * 2.0 / 30.0).min(1.0) } else { 0.0 }
}
/// The number of failed draws before the first success of a 1-in-`1/p` draw each tick.
fn failures_before(rng: &mut Rng, p: f64) -> u64 {
    let u = rng.next_f64().max(1.0e-12);
    (u.ln() / (1.0 - p).ln()).floor().min(4000.0).max(0.0) as u64
}
/// Looks at the sun again for the monsters an event names and those whose eye changed block: no scan of the others.
#[allow(clippy::type_complexity)]
fn sun_eval(
    clock: Res<Clock>,
    level: Res<Level>,
    day: Res<Daylight>,
    mut dirty: ResMut<SunDirty>,
    mut now_fired: ResMut<SunFired>,
    mut wheel: ResMut<Wheel>,
    mut p: ParamSet<(
        Query<(Entity, &Pos, &BurnsInDay), Changed<Bounds>>,
        Query<(&Pos, &mut BurnsInDay, &mut Rng, Has<Exposed>)>,
    )>,
    mut cmd: Commands,
) {
    let mut todo = std::mem::take(&mut dirty.0);
    todo.extend(p.p0().iter().filter(|(_, pos, b)| eye_block(pos.0, b.eye) != b.block).map(|x| x.0));
    todo.sort_unstable();
    todo.dedup();
    let mut watched = p.p1();
    for e in todo {
        let Ok((pos, mut b, mut rng, exposed)) = watched.get_mut(e) else { continue };
        b.block = eye_block(pos.0, b.eye);
        let chance = burn_chance(&level, day.0, b.block);
        if chance > 0.0 && !exposed {
            // The draw of THIS tick (the monster has moved already) is the first: it succeeds at tick now + failures.
            let now = clock.of(e);
            let at = now + failures_before(&mut rng, chance);
            // This tick's own draw came up: the wheel of this tick has been emptied already, so it is fired by hand.
            if at == now {
                now_fired.0.push(e);
            } else {
                wheel.push(clock.at(e, at), e, Due::Sun);
            }
            cmd.entity(e).insert(Exposed { token: at });
        } else if chance == 0.0 && exposed {
            cmd.entity(e).remove::<Exposed>();
        }
    }
}
/// The draw came up: a monster that is not wet catches fire for 160 ticks (`ignite_for_ticks`), and the next draw is
/// armed while it stays exposed.
#[allow(clippy::type_complexity)]
fn sun_burn(
    clock: Res<Clock>,
    level: Res<Level>,
    day: Res<Daylight>,
    mut fired: ResMut<SunFired>,
    mut wheel: ResMut<Wheel>,
    mut q: Query<(&Exposed, &BurnsInDay, &FluidState, Option<&Burning>, &mut Rng)>,
    mut cmd: Commands,
) {
    for e in std::mem::take(&mut fired.0) {
        let Ok((x, b, fs, burning, mut rng)) = q.get_mut(e) else { continue };
        let now = clock.of(e);
        if x.token != now {
            continue;
        }
        if !fs.in_water {
            let until = now + 160;
            match burning {
                None => {
                    wheel.push(clock.at(e, now + 1), e, Due::Burn);
                    cmd.entity(e).insert(Burning { until });
                }
                Some(bn) if bn.until < until => {
                    cmd.entity(e).insert(Burning { until });
                }
                _ => {}
            }
        }
        let chance = burn_chance(&level, day.0, b.block);
        if chance > 0.0 {
            // The next draw is the next tick's.
            let at = now + 1 + failures_before(&mut rng, chance);
            wheel.push(clock.at(e, at), e, Due::Sun);
            cmd.entity(e).insert(Exposed { token: at });
        } else {
            cmd.entity(e).remove::<Exposed>();
        }
    }
}
