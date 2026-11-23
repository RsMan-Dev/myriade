//! The data and rules of a hostile monster, shared by the `target` and `melee` modules (a species with those entries in
//! its configuration): `NearestAttackableTargetGoal` on the player, `HurtByTargetGoal`, `MeleeAttackGoal`.
//! Event driven like the farm goals: the target is looked for on a geometric deadline (the selector's 1-in-5 ask every
//! other tick, sampled once), the melee goal looks at the target on its 20-tick check grid, and only a monster that
//! has a target, or a melee that runs, is visited by a system. The damage goes through the players' state.
use crate::cow::*;
use crate::goals::*;
use crate::bridge::teams::allied;
use crate::targeting::{self, Target, Who, info};
use bevy_ecs::prelude::*;
use bevy_ecs::system::ParamSet;
use crate::rules::hostile::{attack_damage, burn_chance, cooldown_expired_at_restart, eye_block, failures_before, far_repath_extra, health_after, melee_reach, melee_retry_wait, melee_wait, spread_damage, unreachable_too_long};
use horizon_core::geometry::Vec3;

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
    /// Called (in the next flush) when a strike lands, with its victim and the damage dealt (`doHurtTarget`'s `hurtServer` was true):
    /// the bee's sting, the toss of the golem, the headbutt.
    pub on_hit: Option<fn(&mut World, Entity, Who, f32)>,
    /// Called at every strike, landed or not (what `doHurtTarget` does before the blow): the golem's attack event, the ravager's bite.
    pub on_blow: Option<fn(&mut World, Entity)>,
    /// `IronGolem.doHurtTarget`: the damage is half the attack plus a draw below its integer part, not the fixed value.
    pub spread: bool,
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
    /// Who dealt it, when a creature or a player did.
    pub by: Option<Who>,
}
/// The players' health (20 until set), and the log of the hits they took. `immortal`: the health never drops below
/// 1 (the benches keep the players alive to measure a long chase).
#[derive(Resource, Default)]
pub struct PlayerState {
    pub health: Vec<f32>,
    pub hits: Vec<PlayerHit>,
    /// The second knockback of a blow (`causeExtraKnockback`): the player, (x, z, power) as `Hit.extra`, and the creature that dealt it.
    pub pushes: Vec<(u32, (f64, f64, f64), Entity)>,
    pub immortal: bool,
    /// Players that cannot be targeted (creative, spectator), and the visibility of a player to the monsters (1 by default;
    /// invisible, sneaking and armour scale the range of sight).
    pub creative: Vec<bool>,
    pub spectator: Vec<bool>,
    pub visibility: Vec<f64>,
    /// Sneaking, the helmet worn (an item id, 0 none), the view vector (what the player looks at), by
    /// player: what the creatures read of a player beyond where it is. `peaceful`: the difficulty (monsters do not target).
    pub sneaking: Vec<bool>,
    /// The players standing in water (`isInWater`, read by the animals' anger search).
    pub water: Vec<bool>,
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
    pub fn in_water(&self, id: u32) -> bool {
        self.water.get(id as usize).copied().unwrap_or(false)
    }
    pub fn set_in_water(&mut self, id: u32, on: bool) {
        let i = id as usize;
        if self.water.len() <= i {
            self.water.resize(i + 1, false);
        }
        self.water[i] = on;
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
        self.hits.push(PlayerHit { tick, player, amount, from, by });
    }
    /// The extra push of the blow just dealt to `player` (`Hit.extra`), drained by the host with the hits.
    pub(crate) fn push(&mut self, player: u32, extra: (f64, f64, f64), by: Entity) {
        self.pushes.push((player, extra, by));
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
    s.add_systems((day_sync, sun_eval, sun_burn).chain().after(crate::cow::reindex));
}

/// The rule `sun_burn` (no params): the undead that burn in daylight (`Mob.isSunBurnTick`): the eye height they look at the sky from.
/// A monster by category that is not an `Enemy` in the jar (zombie horse, camel husk, zombie nautilus): the rule `NOT_ENEMY`.
#[derive(Component)]
pub struct NotEnemy;
pub static NOT_ENEMY: crate::rules::Rule = crate::rules::Rule { name: "not_enemy", equip: |w, e, _| { w.entity_mut(e).insert(NotEnemy); }, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
pub static SUN_BURN: crate::rules::Rule = crate::rules::Rule { name: "sun_burn", equip: equip_sun_burn, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
/// The equipment slot whose stack keeps the monster from the sun (`Mob.sunProtectionSlot`): absent, the head (slot 2).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct SunSlot(pub usize);
/// The rule `sun_body`: the body slot keeps the monster from the sun, as `ZombieHorse.sunProtectionSlot` (BODY, slot 6).
pub static SUN_BODY: crate::rules::Rule = crate::rules::Rule { name: "sun_body", equip: |w, e, _| { w.entity_mut(e).insert(SunSlot(6)); }, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
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
        on_blow: None,
        spread: false,
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
    mut q: Query<(Aim, &Bounds, &mut Attack, Option<&Target>, &mut Rng, Has<GrowAt>, Option<(&crate::behaviours::melee::QuitsInLight, &targeting::Eye)>, Option<&Occupied>), With<Meleeing>>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
    gear: Option<Res<crate::melee_gear::MeleeGear>>,
    held: Query<&crate::equip_stack::Equipment>,
    main: Query<&crate::goals::Equipment>,
    kinds: Query<&SpeciesId>,
    species: Res<SpeciesTable>,
    restricted: Query<&crate::behaviours::move_towards_restriction::Restriction>,
    riding: Query<&crate::mount::Riding>,
    boxes: Query<&Bounds>,
    prototype: Option<Res<crate::rules::melee_attack::PrototypeRange>>,
) {
    for ((e, pos, mut look, looking, _following, _asked), bounds, mut a, g, mut rng, baby, quits, busy) in &mut q {
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
        // `MeleeAttackGoal.canContinueToUse` when it follows the target unseen: the target out of the home, or a creative or spectator player, ends it.
        let out_of_home = restricted.get(e).is_ok_and(|r| r.radius != -1 && !crate::rules::move_towards_restriction::within_home(r.center, r.radius, [i.pos.x, i.pos.y, i.pos.z]));
        let barred = matches!(g.who, Who::Player(p) if tg.ps.creative(p) || tg.ps.spectator(p));
        if a.follow_unseen && (out_of_home || barred) {
            reqs.0.push(Req::End(e, a.melee_slot));
            continue;
        }
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
        let vehicle = riding.get(e).ok().and_then(|r| boxes.get(r.0).ok()).map(|b| b.0);
        let held_stack = held.get(e).ok().and_then(|q| q.slots[0].as_ref());
        let proto = held_stack.and_then(|s| prototype.as_ref().and_then(|p| (p.0)(&s.item)));
        if ready && g.seen && crate::rules::melee_attack::within_strike(bounds.0, vehicle, i.bounds, held_stack, proto.as_ref()) {
            let interval = if baby { a.interval_baby.unwrap_or(a.interval) } else { a.interval };
            a.until = interval;
            a.next_strike = clock.0 + interval as u64;
            // `MeleeAttackGoal.checkAndPerformAttack`: `mob.swing(MAIN_HAND)` before the blow.
            cmd.queue(move |w: &mut World| {
                crate::swing::swing(w, e, false);
            });
            if let Some(f) = a.on_blow {
                cmd.queue(move |w: &mut World| f(w, e));
            }
            // `LivingEntity.doHurtTarget` reads ATTACK_DAMAGE, which the held item's modifier adds to the species' base.
            let base = a.damage + crate::items::attack_damage_of(main.get(e).map_or(0, |q| q.main)) as f32;
            let damage = if a.spread { spread_damage(base, |n| rng.next_int(n)) } else { base };
            // `EnchantmentHelper.modifyDamage` of the held weapon, by the host's table, once per blow.
            let damage = match (gear.as_deref(), held.get(e).ok().and_then(|q| q.slots[0].as_ref())) {
                (Some(gear), Some(stack)) => {
                    let name = |s: Option<&SpeciesId>| s.map_or("", |s| species.0[s.0 as usize].name);
                    let victim = match g.who {
                        Who::Player(_) => "minecraft:player",
                        Who::Entity(t) => name(kinds.get(t).ok()),
                    };
                    (gear.0)(stack, name(kinds.get(e).ok()), victim, crate::melee_gear::Blow { damage, ..Default::default() }).damage
                }
                _ => damage,
            };
            match g.who {
                Who::Player(id) => {
                    let (tick, from) = (clock.0, (pos.0.x, pos.0.z));
                    let by = Who::Entity(e);
                    // The host gives no answer for a player: the blow is taken as landed.
                    cmd.queue(move |w: &mut World| {
                        w.resource_mut::<PlayerState>().hurt(tick, id, damage, from, Some(by));
                        landed(w, e, Who::Player(id), damage);
                    });
                }
                // The victim's `apply_damage` tells `landed` the result.
                Who::Entity(t) => dq.add_by_entity(t, damage, (pos.0.x, pos.0.z), e),
            }
        }
    }
}

/// What follows a melee blow that dealt `dealt` to `who` (`doHurtTarget` when `hurtServer` returned true): the effects, the fire, the extra
/// knockback, then the hook of the species. Called by `apply_damage` for a creature, at once for a player.
pub fn landed(w: &mut World, e: Entity, who: Who, dealt: f32) {
    if w.get_entity(e).is_err() {
        return;
    }
    crate::effects::hit_effects(w, e, who);
    ignite_on_hit(w, e, who);
    // The weapon's enchantments: the knockback (`getKnockback`) and Fire Aspect (`doPostAttackEffects`), from the host's table.
    let base_knockback = w.get::<SpeciesId>(e).map_or(0.0, |s| crate::attr_mod::read(w, e, "attack_knockback", w.resource::<SpeciesTable>().0[s.0 as usize].attack_knockback) as f32);
    let blow = crate::melee_gear::blow(w, e, who, crate::melee_gear::Blow { damage: dealt, knockback: base_knockback, ignite_seconds: 0.0 });
    extra_knockback(w, e, who, f64::from(blow.knockback));
    if blow.ignite_seconds > 0.0 {
        let ticks = (blow.ignite_seconds * 20.0) as i32;
        match who {
            Who::Entity(t) => set_on_fire(w, t, ticks as u64),
            Who::Player(player) => w
                .get_resource_or_insert_with(crate::colony_register::MiscOut::default)
                .0
                .push(crate::host_out_misc::MiscEvent::PlayerIgnite { player, ticks }),
        }
    }
    if let Some(f) = w.get::<Attack>(e).and_then(|a| a.on_hit) {
        f(w, e, who, dealt);
    }
    if w.get::<crate::behaviours::contact_damage::ContactDamage>(e).is_some() {
        crate::behaviours::contact_damage::attack_sound(w, e);
    }
    if let Some(bite) = w.get::<crate::rules::bite_sound::BiteSound>(e).map(|b| b.0) {
        if w.get::<crate::behaviours::elder_curse::Silent>(e).is_none() {
            crate::behaviours::act_phase::sound(w, e, bite, 1.0, 1.0);
        }
    }
}
/// `Zombie.doHurtTarget`: a burning attacker with an empty hand sets its target on fire with a chance of 0.3 per point of the effective
/// difficulty, for 2 seconds per whole point. Params of the rule `ignite_on_hit`.
#[derive(Clone, Copy, Debug)]
pub struct IgniteParams {
    pub chance: f32,
    pub seconds: i32,
}
pub static ZOMBIE_FIRE: IgniteParams = IgniteParams { chance: 0.3, seconds: 2 };
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct IgniteOnHit(pub &'static IgniteParams);
pub static IGNITE_ON_HIT: crate::rules::Rule = crate::rules::Rule { name: "ignite_on_hit", equip: equip_ignite_on_hit, attrs: crate::rules::no_attrs, interact: crate::rules::no_interact };
fn equip_ignite_on_hit(w: &mut World, e: Entity, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(IgniteOnHit(crate::rules::params::<IgniteParams>(p)));
}
fn ignite_on_hit(w: &mut World, e: Entity, who: Who) {
    let Some(IgniteOnHit(p)) = w.get::<IgniteOnHit>(e).copied() else { return };
    let now = now_of(w, e);
    if !w.get::<Burning>(e).is_some_and(|b| b.until > now as i64) || crate::equip_stack::get(w, e, 0).is_some() {
        return;
    }
    let effective = crate::effects::effective_difficulty(w, e);
    let draw = w.get_mut::<Rng>(e).map_or(1.0, |mut r| r.next_f64() as f32);
    if draw >= effective * p.chance {
        return;
    }
    let ticks = p.seconds * effective as i32 * 20;
    match who {
        Who::Entity(t) => set_on_fire(w, t, ticks as u64),
        Who::Player(player) => w
            .get_resource_or_insert_with(crate::colony_register::MiscOut::default)
            .0
            .push(crate::host_out_misc::MiscEvent::PlayerIgnite { player, ticks }),
    }
}
/// `LivingEntity.knockback`: the target's velocity is halved and pushed along `(x, z)` by `strength` less its resistance; on the ground it also
/// rises (up to 0.4). A creature only; a player's push goes through the host.
pub fn knock(w: &mut World, target: Entity, strength: f64, x: f64, z: f64) {
    let strength = strength * (1.0 - w.get::<Resist>(target).map_or(0.0, |r| r.0).clamp(0.0, 1.0));
    if strength <= 0.0 {
        return;
    }
    let ground = w.get::<Contact>(target).is_some_and(|c| c.on_ground);
    let l = (x * x + z * z).sqrt();
    let (kx, kz) = if l > 0.0 { (x / l * strength, z / l * strength) } else { (0.0, 0.0) };
    let Some(mut v) = w.get_mut::<Vel>(target) else { return };
    let y = if ground { (v.0.y / 2.0 + strength).min(0.4) } else { v.0.y };
    v.0 = Vec3::new(v.0.x / 2.0 - kx, y, v.0.z / 2.0 - kz);
    w.entity_mut(target).insert(Moving);
}
/// `Mob.doHurtTarget`'s `causeExtraKnockback`: half of `knockback` (`getKnockback`: the species' attribute plus the held weapon's Knockback)
/// along the attacker's facing, and its own motion slowed (x and z by 0.6).
pub fn extra_knockback(w: &mut World, e: Entity, who: Who, knockback: f64) {
    // The hoglin's `doHurtTarget` does not call `Mob.doHurtTarget`: its headbutt throws instead.
    if w.get::<crate::behaviours::headbutt::Headbutt>(e).is_some() {
        return;
    }
    push_along_facing(w, e, who, knockback / 2.0);
}
/// `LivingEntity.causeExtraKnockback(target, power, ..)`: `who` is pushed by `power` along the facing of `e`, whose own motion slows.
pub fn push_along_facing(w: &mut World, e: Entity, who: Who, power: f64) {
    use horizon_core::geometry::trig::Mth;
    if power <= 0.0 {
        return;
    }
    let yaw = w.get::<Yaw>(e).map_or(0.0, |y| y.0).to_radians();
    let (x, z) = (f64::from(yaw.msin()), -f64::from(yaw.mcos()));
    match who {
        Who::Entity(t) => knock(w, t, power, x, z),
        Who::Player(id) => w.resource_mut::<Events>().host.push_player(id, [-x * power, 0.0, -z * power]),
    }
    if let Some(mut v) = w.get_mut::<Vel>(e) {
        v.0 = Vec3::new(v.0.x * 0.6, v.0.y, v.0.z * 0.6);
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
/// The level's day (`Level.isBrightOutside`, which the host gives in `MobWorld.daylight`) into `Daylight`: when it changes, every
/// monster that burns looks at the sun again (a bool compared per tick, nothing else).
fn day_sync(level: Res<Level>, mut day: ResMut<Daylight>, mut dirty: ResMut<SunDirty>, burners: Query<Entity, With<BurnsInDay>>) {
    let now = level.mw.0.daylight;
    if day.0 != now {
        day.0 = now;
        dirty.0.extend(burners.iter());
    }
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
fn wet_by_weather(mw: &myriade_engine_lib::mobs::MobWorld, p: Vec3, bd: &Body) -> bool {
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
    mut q: Query<(&Exposed, &BurnsInDay, &FluidState, Option<&Burning>, &mut Rng, Has<crate::inside_effects::WasInPowder>)>,
    bodies: Query<(&Pos, &Body)>,
    slots: Query<&SunSlot>,
    worn: Query<&crate::equip_stack::Equipment>,
    mut cmd: Commands,
) {
    for e in std::mem::take(&mut fired.0) {
        let Ok((x, b, fs, burning, mut rng, was_in_powder)) = q.get_mut(e) else { continue };
        let now = clock.of(e);
        if x.token != now {
            continue;
        }
        // `Mob.sunProtectionSlot`: the head unless the species names another slot.
        let shield = slots.get(e).map_or(2, |s| s.0);
        let wet = was_in_powder || bodies.get(e).is_ok_and(|(p, bd)| wet_by_weather(level.mw.0, p.0, bd));
        if b.flee {
            // Not done: the gate of 600 ticks since the target was taken (`targetChangeTime`).
            cmd.queue(move |w: &mut World| {
                crate::targeting::clear_target(w, e);
                crate::behaviours::ender_teleport::random(w, e);
            });
        } else if !fs.in_water && !wet && worn.get(e).is_ok_and(|q| q.slots[shield].is_some()) {
            // `Mob.burnUndead`: the stack in the protection slot takes one point of wear and the monster does not burn.
            cmd.queue(move |w: &mut World| {
                crate::equip_stack::damage(w, e, shield, 1);
            });
        } else if !fs.in_water && !wet {
            let until = now as i64 + 160;
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
