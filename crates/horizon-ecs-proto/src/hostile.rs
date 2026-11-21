//! The data and rules of a hostile monster, shared by the `target` and `melee` modules (a species with those entries in
//! its configuration): `NearestAttackableTargetGoal` on the player, `HurtByTargetGoal`, `MeleeAttackGoal`.
//! Event driven like the farm goals: the target is looked for on a geometric deadline (the selector's 1-in-5 ask every
//! other tick, sampled once), the melee goal looks at the target on its 20-tick check grid, and only a monster that
//! has a target, or a melee that runs, is visited by a system. The damage goes through the players' state.
use crate::cow::*;
use crate::goals::*;
use crate::bridge::teams::allied;
use crate::targeting::{self, Target, Who, info, player_box};
use bevy_ecs::prelude::*;
use bevy_ecs::system::ParamSet;
use crate::rules::hostile::{attack_damage, burn_chance, cooldown_expired_at_restart, eye_block, failures_before, far_repath_extra, health_after, melee_reach, melee_retry_wait, melee_wait, unreachable_too_long};
use horizon_core::geometry::{Aabb, Vec3};

/// `MeleeAttackGoal`'s data (from the species: damage, interval, speed) and state.
#[derive(Component, Clone, Copy, Debug)]
pub struct Attack {
    pub damage: f32,
    /// Ticks between two strikes, and for a baby (`MeleeAttackGoal.intervals(adult, baby)`).
    pub interval: i32,
    pub interval_baby: Option<i32>,
    /// `MeleeAttackGoal.followingTargetEvenIfNotSeen`: the goal does not end with its path, it goes on while the target lives, and its
    /// cooldown runs on through a restart (`stopped_at`: the world tick the goal last stopped at).
    pub follow_unseen: bool,
    pub stopped_at: u64,
    /// For the goal that follows unseen: the world tick of the next strike (a restart at the end of a path loses nothing).
    pub next_strike: u64,
    /// The world tick the goal found the body busy at (`Ravager.isImmobile` stops the goals' ticks): 0 when it is free.
    pub frozen_since: u64,
    pub speed: f64,
    /// `Mob.isWithinMeleeAttackRange`: the box widened by this on x and z meets the target's box.
    pub reach: f64,
    /// The tick the monster appeared at: the 20-tick grid of the melee goal's `canUse` is relative to it.
    pub phase: u64,
    pub until: i32,
    pub recalc: i32,
    pub path_target: Vec3,
    /// A check of the melee goal is in the timer wheel.
    pub armed: bool,
    /// The rank of the melee goal in the animal's selector (`u8::MAX`: none).
    pub melee_slot: u8,
    /// Called (in the next flush) when a strike lands: the bee's sting, which costs it its life.
    pub on_hit: Option<fn(&mut World, Entity)>,
    /// `StopAttackingIfTargetInvalid`: the target is forgotten when the last path to it has not reached it for this many ticks (`Nav.cant_reach_since`); the brain animals' rule.
    pub forget_unreachable: Option<i32>,
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
    /// Players that cannot be targeted (creative, spectator), and the visibility of a player to the monsters (1 by default;
    /// invisible, sneaking and armour scale the range of sight).
    pub creative: Vec<bool>,
    pub spectator: Vec<bool>,
    pub visibility: Vec<f64>,
    /// Sneaking, the helmet worn (an item id, 0 none), the view vector (what the player looks at), by
    /// player: what the creatures read of a player beyond where it is. `peaceful`: the difficulty (monsters do not target).
    pub sneaking: Vec<bool>,
    pub helmet: Vec<u32>,
    pub gaze: Vec<[f64; 3]>,
    /// The players whose view vector changed since the last tick (drained into `Events.gaze_changed`).
    pub gaze_dirty: Vec<u32>,
    pub peaceful: bool,
    /// `getLastHurtByMob` and `getLastHurtMob` of each player (who, the world tick): what the pets of a player answer (`tame.rs`).
    pub hurt_by: Vec<Option<(Who, u64)>>,
    pub last_hurt: Vec<Option<(Who, u64)>>,
    /// The players whose combat record changed since the last tick (`tame::owner_events` drains it).
    pub combat_dirty: Vec<u32>,
}
impl PlayerState {
    pub fn creative(&self, id: u32) -> bool {
        self.creative.get(id as usize).copied().unwrap_or(false)
    }
    pub fn spectator(&self, id: u32) -> bool {
        self.spectator.get(id as usize).copied().unwrap_or(false)
    }
    pub fn visibility(&self, id: u32) -> f64 {
        self.visibility.get(id as usize).copied().unwrap_or(1.0)
    }
    pub fn set_visibility(&mut self, id: u32, v: f64) {
        let i = id as usize;
        if self.visibility.len() <= i {
            self.visibility.resize(i + 1, 1.0);
        }
        self.visibility[i] = v;
    }
    pub fn sneaking(&self, id: u32) -> bool {
        self.sneaking.get(id as usize).copied().unwrap_or(false)
    }
    pub fn helmet(&self, id: u32) -> u32 {
        self.helmet.get(id as usize).copied().unwrap_or(0)
    }
    /// The view vector of the player (`getViewVector`), the south by default.
    pub fn gaze(&self, id: u32) -> [f64; 3] {
        self.gaze.get(id as usize).copied().unwrap_or([0.0, 0.0, 1.0])
    }
    /// The single rule of "a monster may target / flee this player": alive, not creative, not spectator, not Peaceful.
    pub fn targetable(&self, id: u32) -> bool {
        self.alive(id) && !self.creative(id) && !self.spectator(id) && !self.peaceful
    }
    pub fn set_sneaking(&mut self, id: u32, on: bool) {
        let i = id as usize;
        if self.sneaking.len() <= i {
            self.sneaking.resize(i + 1, false);
        }
        self.sneaking[i] = on;
    }
    pub fn set_helmet(&mut self, id: u32, item: u32) {
        let i = id as usize;
        if self.helmet.len() <= i {
            self.helmet.resize(i + 1, 0);
        }
        self.helmet[i] = item;
    }
    pub fn set_gaze(&mut self, id: u32, v: [f64; 3]) {
        let i = id as usize;
        if self.gaze.len() <= i {
            self.gaze.resize(i + 1, [0.0, 0.0, 1.0]);
        }
        if self.gaze[i] != v {
            self.gaze_dirty.push(id);
        }
        self.gaze[i] = v;
    }
    pub fn set_creative(&mut self, id: u32, on: bool) {
        let i = id as usize;
        if self.creative.len() <= i {
            self.creative.resize(i + 1, false);
        }
        self.creative[i] = on;
    }
    pub fn health_of(&self, id: u32) -> f32 {
        self.health.get(id as usize).copied().unwrap_or(20.0)
    }
    pub fn alive(&self, id: u32) -> bool {
        self.health_of(id) > 0.0
    }
    pub fn set_hurt_by(&mut self, player: u32, who: Who, tick: u64) {
        let i = player as usize;
        if self.hurt_by.len() <= i {
            self.hurt_by.resize(i + 1, None);
        }
        self.hurt_by[i] = Some((who, tick));
        self.combat_dirty.push(player);
    }
    pub fn set_last_hurt(&mut self, player: u32, who: Who, tick: u64) {
        let i = player as usize;
        if self.last_hurt.len() <= i {
            self.last_hurt.resize(i + 1, None);
        }
        self.last_hurt[i] = Some((who, tick));
        self.combat_dirty.push(player);
    }
    pub(crate) fn hurt(&mut self, tick: u64, player: u32, amount: f32, from: (f64, f64), by: Option<Who>) {
        if let Some(w) = by {
            self.set_hurt_by(player, w, tick);
        }
        let i = player as usize;
        if self.health.len() <= i {
            self.health.resize(i + 1, 20.0);
        }
        self.health[i] = health_after(self.health[i], amount, self.immortal);
        self.hits.push(PlayerHit { tick, player, amount, from });
    }
}
/// The creatures hurt this step by a player or a creature: (victim, attacker), written by `apply_damage`.
#[derive(Resource, Default)]
pub struct HurtBy(pub Vec<(Entity, Who)>);

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

/// The rule `sun_burn` (no params): the undead that burn in daylight (`Mob.isSunBurnTick`): the eye height they look at the sky from.
pub static SUN_BURN: crate::rules::Rule = crate::rules::Rule { name: "sun_burn", equip: equip_sun_burn, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn equip_sun_burn(world: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync)) {
    let eye = world.get::<FluidParams>(e).map_or(0.0, |f| f.eye_height);
    world.entity_mut(e).insert(BurnsInDay { eye, block: (i32::MIN, 0, 0), flee: false });
    world.resource_mut::<SunDirty>().0.push(e);
}
/// The rule `sun_flee`: the creature that leaves in daylight instead of burning (the enderman: the same draw, a teleport at the hit).
pub static SUN_FLEE: crate::rules::Rule = crate::rules::Rule { name: "sun_flee", equip: equip_sun_flee, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn equip_sun_flee(world: &mut World, e: Entity, _: &'static (dyn std::any::Any + Sync)) {
    let eye = world.get::<FluidParams>(e).map_or(0.0, |f| f.eye_height);
    world.entity_mut(e).insert(BurnsInDay { eye, block: (i32::MIN, 0, 0), flee: true });
    world.resource_mut::<SunDirty>().0.push(e);
}
/// The monster's `Attack` data from its species (the melee module's).
pub fn ensure_attack(w: &mut World, e: Entity) {
    if w.get::<Attack>(e).is_some() {
        return;
    }
    let damage = w.get::<AttackDamage>(e).map_or(0.0, |d| d.0);
    // The melee check's grid is in the WORLD's time (`MeleeAttackGoal.lastCanUseCheck` against `getGameTime`).
    let now = w.resource::<Clock>().0;
    w.entity_mut(e).insert(Attack {
        damage: attack_damage(damage),
        interval: 20,
        interval_baby: None,
        follow_unseen: false,
        stopped_at: 0,
        next_strike: 0,
        frozen_since: 0,
        speed: 1.0,
        reach: melee_reach(),
        phase: now,
        until: 0,
        recalc: 0,
        path_target: Vec3::ZERO,
        armed: false,
        melee_slot: u8::MAX,
        on_hit: None,
        forget_unreachable: None,
    });
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
    let wait = melee_wait(now, a.phase);
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
    let wait = melee_retry_wait(now, a.phase, a.follow_unseen);
    let slot = a_slot;
    arm(w, e, slot, wait);
}
/// `canUse`: a living target (a path to it or within reach: the path is made by the pathfinder at the start; the case of
/// no path is not fed back).
pub fn melee_target(w: &mut World, e: Entity) -> Option<Who> {
    let who = w.get::<Target>(e)?.who;
    (info(w, who).is_some() && !allied(w, Who::Entity(e), who)).then_some(who)
}
/// `MeleeAttackGoal.start`: a path to the target, the cooldown and the repath timer reset.
pub fn start_melee(w: &mut World, e: Entity, who: Who) {
    let Some(p) = info(w, who).map(|i| i.pos) else { return };
    let now = w.resource::<Clock>().0;
    let speed = {
        let mut a = w.get_mut::<Attack>(e).unwrap();
        (a.recalc, a.path_target) = (0, Vec3::ZERO);
        // The goal that follows its target unseen is restarted by the frame at the end of each path: its cooldown went on meanwhile.
        if a.follow_unseen {
            // A restart within a few ticks of the stop is the frame's (the path ended): the cooldown is a date and goes on.
            if cooldown_expired_at_restart(now, a.stopped_at) {
                a.next_strike = 0;
            }
        } else {
            a.until = 0;
        }
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
    let now = w.resource::<Clock>().0;
    if let Some(mut a) = w.get_mut::<Attack>(e) {
        a.stopped_at = now;
    }
    w.entity_mut(e).remove::<Meleeing>();
    if let Some(mut nav) = w.get_mut::<Nav>(e) {
        nav.reach = 1;
    }
    // `StopAttackingIfTargetInvalid`: no path has reached the target for the allowed time, it is forgotten.
    let forget = w.get::<Attack>(e).and_then(|a| a.forget_unreachable).zip(w.get::<Nav>(e)).is_some_and(|(n, nav)| unreachable_too_long(now_of(w, e), nav.cant_reach_since, n));
    stop_nav(w, e);
    if forget {
        targeting::clear_target(w, e);
        return;
    }
    if w.get::<Target>(e).is_some() {
        arm_melee_after(w, e);
    }
}
/// `MeleeAttackGoal.tick` (every tick): look at the target, ask for a new path every 4 to 10 ticks (plus 5 or 10 when
/// far) when the target moved a block or by 1 chance in 20, and strike when the cooldown is over and the target is
/// within reach.
#[allow(clippy::type_complexity)]
pub fn melee_run(
    clock: Res<Clock>,
    level: Res<Level>,
    tg: targeting::Targets,
    mut dq: ResMut<DamageQueue>,
    mut q: Query<(Aim, &Bounds, &mut Attack, Option<&Target>, &mut Rng, Has<GrowAt>, Has<crate::effects::OnHit>, Option<(&crate::behaviours::melee::QuitsInLight, &targeting::Eye)>, Option<&Occupied>), With<Meleeing>>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, _following, _asked), bounds, mut a, g, mut rng, baby, effects, quits, busy) in &mut q {
        // `Ravager.isImmobile`: the goals do not tick while it lasts, so the strike cooldown stands still.
        // The goal is restarted by the frame, so the pause is measured in world time: from the first run seen busy to the first one free.
        if busy.is_some_and(|o| o.0 & crate::behaviours::hunt_speed::BUSY != 0) {
            if a.frozen_since == 0 {
                a.frozen_since = clock.0;
            }
            continue;
        }
        if a.frozen_since != 0 {
            a.next_strike += clock.0 - a.frozen_since;
            a.frozen_since = 0;
        }
        // `SpiderAttackGoal.canContinue`: in light, one chance in n per tick to give up and forget the target.
        if let Some((n, eye)) = quits {
            let (x, y, z) = eye_block(pos.0, eye.0);
            if level.mw.0.brightness(x, y, z) >= crate::behaviours::melee::BRIGHT && rng.next_int(n.0) == 0 {
                cmd.queue(move |w: &mut World| targeting::clear_target(w, e));
                reqs.0.push(Req::End(e, a.melee_slot));
                continue;
            }
        }
        let Some((g, i)) = g.and_then(|g| tg.info(g.who).map(|i| (g, i))) else {
            reqs.0.push(Req::End(e, a.melee_slot));
            continue;
        };
        let at = i.pos;
        aim(&mut cmd, e, &mut look, looking, [at.x, i.eye, at.z], Some(30.0));
        a.recalc = (a.recalc - 1).max(0);
        let moved = d2(at, a.path_target) >= 1.0;
        // `follow_unseen` is false for the zombie: no new path while the target is not in sight.
        if (g.seen || a.follow_unseen) && a.recalc <= 0 && (a.path_target == Vec3::ZERO || moved || rng.next_f64() < 0.05) {
            a.path_target = at;
            a.recalc = 4 + rng.next_int(7);
            a.recalc += far_repath_extra(d2(pos.0, at));
            go_to(&mut cmd, e, at.x, at.z, a.speed, 5);
        }
        a.until = (a.until - 1).max(0);
        let ready = if a.follow_unseen { clock.0 >= a.next_strike } else { a.until <= 0 };
        if ready && g.seen && bounds.0.inflate(Vec3::new(a.reach, 0.0, a.reach)).intersects(i.bounds) {
            let interval = if baby { a.interval_baby.unwrap_or(a.interval) } else { a.interval };
            a.until = interval;
            a.next_strike = clock.0 + interval as u64;
            if let Some(f) = a.on_hit {
                cmd.queue(move |w: &mut World| f(w, e));
            }
            match g.who {
                Who::Player(id) => {
                    let (tick, amount, from) = (clock.0, a.damage, (pos.0.x, pos.0.z));
                    let by = Who::Entity(e);
                    cmd.queue(move |w: &mut World| w.resource_mut::<PlayerState>().hurt(tick, id, amount, from, Some(by)));
                    if effects {
                        cmd.queue(move |w: &mut World| crate::effects::hit_effects(w, e, Who::Player(id)));
                    }
                }
                Who::Entity(t) => {
                    dq.add_by_entity(t, a.damage, (pos.0.x, pos.0.z), e);
                    if effects {
                        cmd.queue(move |w: &mut World| crate::effects::hit_effects(w, e, Who::Entity(t)));
                    }
                }
            }
        }
    }
}

/// One blow of `e` on `who` for `amount`, with its knock-back: the damage queue for a creature, the player's state for a player.
pub fn strike(w: &mut World, e: Entity, who: Who, amount: f32) {
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return };
    match who {
        Who::Player(id) => {
            let tick = w.resource::<Clock>().0;
            w.resource_mut::<PlayerState>().hurt(tick, id, amount, (p.x, p.z), Some(Who::Entity(e)));
        }
        Who::Entity(t) => w.resource_mut::<DamageQueue>().add_by_entity(t, amount, (p.x, p.z), e),
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
    /// The sun does not set it alight: it teleports away (`Enderman.customServerAiStep`).
    flee: bool,
}
/// Daylight, sky seen from the eyes and light enough: each tick the monster burns with probability
/// `(brightness - 0.4) * 2 / 30` (`Mob.isSunBurnTick`); one draw is armed in the wheel, its tick is `token`.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Exposed {
    token: u64,
}
/// Looks at the sun again for the monsters an event names and those whose eye changed block: only the animals whose physics is
/// active (`Moving`, a sparse marker) are looked at, not every burner each tick (`Changed<Bounds>` scanned them all).
#[allow(clippy::type_complexity)]
fn sun_eval(
    clock: Res<Clock>,
    level: Res<Level>,
    day: Res<Daylight>,
    mut dirty: ResMut<SunDirty>,
    mut now_fired: ResMut<SunFired>,
    mut wheel: ResMut<Wheel>,
    mut p: ParamSet<(
        Query<(Entity, &Pos, &BurnsInDay), With<Moving>>,
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
        let chance = burn_chance(level.mw.0, day.0, b.block);
        if chance > 0.0 && !exposed {
            // The draw of THIS tick (the monster has moved already) is the first: it succeeds at tick now + failures.
            let now = clock.of(e);
            let at = now + failures_before(rng.next_f64(), chance);
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
/// `Mob.isSunBurnTick`'s other wet cases: rain at the feet or at the top of the box (`Entity.isInRain`), or the box in powder snow.
fn wet_by_weather(mw: &horizon_client_lib::mobs::MobWorld, p: Vec3, bd: &Body) -> bool {
    let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    if mw.raining_at(x, y, z) || mw.raining_at(x, (p.y + f64::from(bd.height)).floor() as i32, z) {
        return true;
    }
    let r = f64::from(bd.width) / 2.0 - 1.0e-5;
    let (x0, x1, z0, z1) = ((p.x - r).floor() as i32, (p.x + r).floor() as i32, (p.z - r).floor() as i32, (p.z + r).floor() as i32);
    let (y0, y1) = ((p.y + 1.0e-5).floor() as i32, (p.y + f64::from(bd.height) - 1.0e-5).floor() as i32);
    (x0..=x1).any(|x| (y0..=y1).any(|y| (z0..=z1).any(|z| &*mw.facts.get(mw.world.block(x, y, z)).name == "minecraft:powder_snow")))
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
    mut events: ResMut<crate::goals::Events>,
    mut q: Query<(&Exposed, &BurnsInDay, &FluidState, Option<&Burning>, &mut Rng)>,
    bodies: Query<(&Pos, &Body)>,
    mut cmd: Commands,
) {
    for e in std::mem::take(&mut fired.0) {
        let Ok((x, b, fs, burning, mut rng)) = q.get_mut(e) else { continue };
        let now = clock.of(e);
        if x.token != now {
            continue;
        }
        if b.flee {
            // Not done: the gate of 600 ticks since the target was taken (`targetChangeTime`).
            cmd.queue(move |w: &mut World| {
                crate::targeting::clear_target(w, e);
                crate::behaviours::ender_teleport::random(w, e);
            });
        } else if !fs.in_water && !bodies.get(e).is_ok_and(|(p, bd)| wet_by_weather(level.mw.0, p.0, bd)) {
            let until = now + 160;
            match burning {
                None => {
                    wheel.push(clock.at(e, now + 1), e, Due::Burn);
                    cmd.entity(e).insert(Burning { until });
                    events.ignited.push(e);
                }
                Some(bn) if bn.until < until => {
                    cmd.entity(e).insert(Burning { until });
                }
                _ => {}
            }
        }
        let chance = burn_chance(level.mw.0, day.0, b.block);
        if chance > 0.0 {
            // The next draw is the next tick's.
            let at = now + 1 + failures_before(rng.next_f64(), chance);
            wheel.push(clock.at(e, at), e, Due::Sun);
            cmd.entity(e).insert(Exposed { token: at });
        } else {
            cmd.entity(e).remove::<Exposed>();
        }
    }
}
