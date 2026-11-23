//! The wheel of deadlines (own time, `arm_at`, `due`).
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
use myriade_engine_lib::mobs::{
    MobWorld,
    nav::Navigation,
    path::{Evaluator, Finder, Path, PathMob, PATH_TYPES},
};
#[allow(unused_imports)]
use myriade_level::{chunk::Chunk, collisions::SceneCollisions};
#[allow(unused_imports)]
use horizon_core::geometry::{Aabb, Vec3, trig};
#[allow(unused_imports)]
use std::sync::Arc;
use super::*;

// ---- resources -------------------------------------------------------------------------------------------
/// The tick counter of the world (advanced by `due`) and the time each entity has not lived: `.0` is the WORLD's time (the
/// timer wheel, the day, the log of the hits); an entity's own time is `of(e)`: the world's minus what it spent
/// asleep (`zone.rs`), as a `Mob`'s `tickCount` does not count the ticks it is not ticked. Every deadline and every date a
/// module or the frame keeps on an entity is in its OWN time; the wheel is in the world's (`at`, `arm`).
#[derive(Resource, Default)]
pub struct Clock(pub u64, pub(crate) Vec<u64>, Option<i64>);

impl Clock {
    /// The time of entity `e`: ticks it has been awake for since the world began (its `age`, with the world's start).
    #[inline]
    pub fn of(&self, e: Entity) -> u64 {
        self.0 - self.1.get(e.index().index() as usize).copied().unwrap_or(0)
    }
    /// The world's tick at which the entity's own time is `local`.
    #[inline]
    pub fn at(&self, e: Entity, local: u64) -> u64 {
        local + self.1.get(e.index().index() as usize).copied().unwrap_or(0)
    }
    /// The entity slept for `d` ticks of the world: its time stood still.
    pub(crate) fn sleep_for(&mut self, e: Entity, d: u64) {
        let i = e.index().index() as usize;
        if self.1.len() <= i {
            self.1.resize(i + 1, 0);
        }
        self.1[i] += d;
    }
    /// A new entity's own time starts at 0 now (`tickCount` counts from its creation); its slot may have been an older entity's.
    pub(crate) fn start(&mut self, e: Entity) {
        let i = e.index().index() as usize;
        if self.1.len() <= i {
            self.1.resize(i + 1, 0);
        }
        self.1[i] = self.0;
    }
}

impl Clock {
    /// The game time (`getGameTime`) of the world's tick 0, noted by the host at each step (`mw.game_time` less the tick): one
    /// field written per step, nothing per entity. Dates vanilla keeps in game time (`Camel.LAST_POSE_CHANGE_TICK`) are read against it.
    pub fn set_game_base(&mut self, base: i64) {
        self.2 = Some(base);
    }
    pub fn has_game_base(&self) -> bool {
        self.2.is_some()
    }
    /// The game time now (`Level.getGameTime`): 0 until the host has noted a base.
    #[inline]
    pub fn game_now(&self) -> i64 {
        self.0 as i64 + self.2.unwrap_or(0)
    }
}

/// The game time at which the own time `local` of `e` falls (its sleeps counted out, as its own time does).
pub fn game_time_at(w: &World, e: Entity, local: i64) -> i64 {
    game_now(w) - now_of(w, e) as i64 + local
}

/// The game time now, for code with the world.
pub fn game_now(w: &World) -> i64 {
    w.get_resource::<Clock>().map_or(0, Clock::game_now)
}

/// On every animal: its own time starts with its creation (see `Clock`).
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(on_add = start_own_time)]
pub struct OwnTime;

pub(crate) fn start_own_time(mut world: bevy_ecs::world::DeferredWorld, ctx: bevy_ecs::lifecycle::HookContext) {
    world.resource_mut::<Clock>().start(ctx.entity);
}

/// Arms a wheel entry for an entity at its own time `local`.
pub fn arm_at(w: &mut World, e: Entity, local: u64, d: Due) {
    let at = w.resource::<Clock>().at(e, local);
    w.resource_mut::<Wheel>().push(at, e, d);
}

/// The functions `after` can call, each registered once.
#[derive(Resource, Default)]
pub struct Callbacks(pub(crate) Vec<fn(&mut World, Entity)>);

/// Calls `f(w, e)` once, `ticks` ticks of the entity's own time from now: a deadline on the wheel, no tick in between.
pub fn after(w: &mut World, e: Entity, ticks: i32, f: fn(&mut World, Entity)) {
    let cb = &mut w.get_resource_or_insert_with(Callbacks::default).0;
    let i = cb.iter().position(|&g| g as usize == f as usize).unwrap_or_else(|| {
        cb.push(f);
        cb.len() - 1
    });
    let at = now_of(w, e) + ticks.max(0) as u64;
    arm_at(w, e, at, Due::Call(i as u16));
}

/// The entity's own time.
pub fn now_of(w: &World, e: Entity) -> u64 {
    w.resource::<Clock>().of(e)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Due {
    Stroll,
    Grow,
    Goal(u8),
    /// The hen's egg timer (`farm.rs`): a rule of the species, not a goal.
    Egg,
    /// A stuck projectile's life is over (`projectile.rs`).
    Expire,
    /// A sensor of the brain is due (its rank in the plan), and a memory's time is up (its slot and the token it had).
    Sensor(u8),
    Forget(u16, u32),
    /// A module's function runs for the entity (`after`): the index in `Callbacks`.
    Call(u16),
    /// The brain has something to do (a timed behaviour's end).
    BrainWake,
    /// A resting item's deadline (its last tick, its merge check), with the token it was armed with (`item.rs`).
    ItemExpire(u32),
    /// A merge check of an item at its own multiple of 40 ticks (`item.rs`).
    ItemCheck(u32),
    /// An orb's end of life and its merge check (`orb.rs`).
    /// A hopping body's wait is over (`locomotion/hop.rs`).
    Hop,
    OrbExpire(u32),
    OrbCheck(u32),
    Burn,
    /// A status effect acts or ends (`effects.rs`).
    Effect,
    /// A creeper's fuse may be at its end (`explosion.rs`).
    Fuse,
    /// `Mob.isSunBurnTick`'s draw came up (`hostile.rs`).
    Sun,
    /// The mob may go away from now on (`despawn.rs`).
    Despawn,
    /// An allay's tenth tick: it heals one point (`allay_heal.rs`).
    AllayHeal,
    /// A wandering trader's delay runs out, or one tick before (`despawn_delay.rs`).
    DespawnDelay,
}

pub(crate) const WHEEL: usize = 4096;

/// A timer wheel: what is due at each tick, so that a sleeping or growing cow costs nothing until then.
#[derive(Resource)]
pub struct Wheel {
    pub(crate) slots: Vec<Vec<(Entity, Due)>>,
    /// What is due further than the wheel's horizon (`WHEEL` ticks): by tick, moved into the slots when it comes within the
    /// horizon. A deadline of ANY length can be pushed: no caller knows the width of the wheel.
    pub(crate) far: std::collections::BTreeMap<u64, Vec<(Entity, Due)>>,
    /// The tick the wheel is at (set by `advance`, each tick, before the slot is read).
    pub(crate) now: u64,
    /// With the simulation zone on (`zone.rs`): what each entity has in the wheel (by entity index), so that a creature
    /// that leaves the zone takes its entries out without a scan of the wheel.
    pub(crate) track: bool,
    pub(crate) owned: Vec<Vec<(Entity, u64, Due)>>,
    /// How many entries have fired since the world began (the tests of the rest cost count the wakes).
    pub fired_total: u64,
}

impl Default for Wheel {
    fn default() -> Self {
        Self { slots: vec![Vec::new(); WHEEL], far: Default::default(), now: 0, track: false, owned: Vec::new(), fired_total: 0 }
    }
}

impl Wheel {
    /// The wheel is at tick `now`: what has come within its horizon leaves the far list for its slot (called by `due`).
    pub(crate) fn advance(&mut self, now: u64) {
        self.now = now;
        while let Some(entry) = self.far.first_entry() {
            if *entry.key() >= now + WHEEL as u64 {
                break;
            }
            let (at, list) = entry.remove_entry();
            self.slots[(at % WHEEL as u64) as usize].extend(list);
        }
    }
    pub fn push(&mut self, at: u64, e: Entity, d: Due) {
        if at >= self.now + WHEEL as u64 {
            self.far.entry(at).or_default().push((e, d));
        } else {
            self.slots[(at % WHEEL as u64) as usize].push((e, d));
        }
        if self.track {
            self.own(e, at, d);
        }
    }
    pub(crate) fn own(&mut self, e: Entity, at: u64, d: Due) {
        let i = e.index().index() as usize;
        if self.owned.len() <= i {
            self.owned.resize_with(i + 1, Vec::new);
        }
        self.owned[i].push((e, at, d));
    }
    pub fn enable_tracking_pub(&mut self) {
        self.enable_tracking();
    }
    /// Starts to keep the entries of each entity (reads the ones already there).
    pub(crate) fn enable_tracking(&mut self) {
        self.track = true;
        self.owned.clear();
        for at in 0..WHEEL {
            for k in 0..self.slots[at].len() {
                let (e, d) = self.slots[at][k];
                // The slot of tick `at` holds the entries of that tick (an entry is at most WHEEL - 1 ticks ahead).
                self.own(e, at as u64, d);
            }
        }
    }
    /// An entry fired at tick `at`: it is no longer the entity's.
    pub(crate) fn fired(&mut self, e: Entity, at: u64, d: Due) {
        if let Some(list) = self.owned.get_mut(e.index().index() as usize)
            && let Some(i) = list.iter().position(|o| o.0 == e && o.2 == d && o.1 % WHEEL as u64 == at % WHEEL as u64)
        {
            list.swap_remove(i);
        }
    }
    /// Takes the entries of `e` out of the wheel: `(tick, what)`.
    pub(crate) fn take_entries(&mut self, e: Entity) -> Vec<(u64, Due)> {
        let Some(list) = self.owned.get_mut(e.index().index() as usize) else { return Vec::new() };
        let mine: Vec<(Entity, u64, Due)> = list.iter().copied().filter(|o| o.0 == e).collect();
        list.retain(|o| o.0 != e);
        let mut out = Vec::with_capacity(mine.len());
        for (_, at, d) in mine {
            let slot = if at >= self.now + WHEEL as u64 { self.far.entry(at).or_default() } else { &mut self.slots[(at % WHEEL as u64) as usize] };
            if let Some(i) = slot.iter().position(|o| o.0 == e && o.1 == d) {
                slot.swap_remove(i);
                out.push((at, d));
            }
        }
        out
    }
    /// How many entries `e` has in the wheel (tracking on).
    pub fn owned_by(&self, e: Entity) -> usize {
        self.owned.get(e.index().index() as usize).map_or(0, |l| l.iter().filter(|o| o.0 == e).count())
    }
    /// How many entries the wheel holds in all.
    pub fn len(&self) -> usize {
        self.slots.iter().map(Vec::len).sum::<usize>() + self.far.values().map(Vec::len).sum::<usize>()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// How many entries the slot of tick `at` holds.
    pub fn due_len(&self, at: u64) -> usize {
        self.slots[(at % WHEEL as u64) as usize].len()
    }
}

/// The width of an index column: a box (at most 1.4 blocks wide here) overlaps only entities within the 3 x 3 columns around it.
pub(crate) const CELL: f64 = 1.0;

/// What is due at this tick in the timer wheel: a stroll attempt (a spot is picked, or the attempt fails and is
/// armed again, or the goal is refused for lack of a player and is not re-armed), a baby grows up.
#[allow(clippy::type_complexity)]
pub(crate) fn due(
    mut clock: ResMut<Clock>,
    mut wheel: ResMut<Wheel>,
    mut fired: ResMut<crate::goals::Fired>,
    mut grown: ResMut<crate::farm::Grown>,
    mut events: ResMut<crate::goals::Events>,
    fight: Option<ResMut<crate::dragon::fight::FightEvents>>,
    mut players: ResMut<crate::hostile::PlayerState>,
    mut sun: ResMut<crate::hostile::SunFired>,
    mut babies: Query<(&mut Body, &Sizes, &GrowAt, &Pos, &mut Bounds), Without<crate::age_lock::AgeLocked>>,
    burners: Query<(Entity, &Burning, Option<&Immune>)>,
    mut queue: ResMut<DamageQueue>,
    mut commands: Commands,
) {
    clock.0 += 1;
    events.swap();
    if let Some(mut f) = fight {
        f.swap();
    }
    wheel.advance(clock.0);
    for id in std::mem::take(&mut players.gaze_dirty) {
        events.gaze_changed.push(id);
    }
    let slot = (clock.0 % WHEEL as u64) as usize;
    let list = std::mem::take(&mut wheel.slots[slot]);
    wheel.fired_total += list.len() as u64;
    if wheel.track {
        for &(e, d) in &list {
            wheel.fired(e, clock.0, d);
        }
    }
    for (e, d) in list {
        match d {
            // The goal selector (`goals.rs`) owns the stroll attempt, like every other goal's deadline.
            Due::Stroll => fired.0.push((e, crate::behaviours::stroll::FIRED)),
            Due::Goal(k) => fired.0.push((e, k)),
            Due::Egg => commands.queue(move |w: &mut World| crate::farm::lay_deadline(w, e)),
            Due::Expire => commands.queue(move |w: &mut World| crate::projectile::expire(w, e)),
            Due::Sensor(i) => commands.queue(move |w: &mut World| crate::brain::sensor_due(w, e, i)),
            Due::ItemExpire(t) => commands.queue(move |w: &mut World| crate::item::expire_due(w, e, t)),
            Due::Hop => commands.queue(move |w: &mut World| crate::locomotion::hop::due(w, e)),
            Due::OrbExpire(t) => commands.queue(move |w: &mut World| crate::orb::expire_due(w, e, t)),
            Due::OrbCheck(t) => commands.queue(move |w: &mut World| crate::orb::check_due(w, e, t)),
            Due::ItemCheck(t) => commands.queue(move |w: &mut World| crate::item::check_due(w, e, t)),
            Due::BrainWake => commands.queue(move |w: &mut World| crate::brain::wake(w, e)),
            Due::Call(i) => commands.queue(move |w: &mut World| {
                let f = w.resource::<Callbacks>().0[usize::from(i)];
                f(w, e);
            }),
            Due::Forget(l, t) => commands.queue(move |w: &mut World| crate::brain::forget_due(w, e, l, t)),
            Due::Sun => sun.0.push(e),
            Due::Effect => commands.queue(move |w: &mut World| crate::effects::due(w, e)),
            Due::Fuse => commands.queue(move |w: &mut World| crate::explosion::fuse_due(w, e)),
            Due::Despawn => commands.queue(move |w: &mut World| crate::despawn::due(w, e)),
            Due::DespawnDelay => commands.queue(move |w: &mut World| crate::despawn_delay::due(w, e)),
            Due::AllayHeal => commands.queue(move |w: &mut World| crate::allay_heal::due(w, e)),
            Due::Burn => {
                if let Ok((e2, burning, imm)) = burners.get(e) {
                    let now = clock.of(e);
                    if (now as i64) < burning.until {
                        if !imm.is_some_and(|i| i.has(crate::effects::IMMUNE_FIRE)) {
                            queue.add(e2, 1.0, Cause::OnFire);
                        }
                        wheel.push(clock.at(e, now + 20), e, Due::Burn);
                    } else {
                        commands.entity(e).remove::<Burning>();
                        events.extinguished.push(e);
                    }
                }
            }
            Due::Grow => {
                if let Ok((mut b, sizes, g, p, mut bb)) = babies.get_mut(e) {
                    let now = clock.of(e);
                    if g.0 > now {
                        // A wait longer than the wheel is put back (a calf grows in 24 000 ticks).
                        wheel.push(clock.at(e, g.0), e, Due::Grow);
                    } else if g.0 == now {
                        b.width = sizes.adult.0;
                        b.height = sizes.adult.1;
                        bb.0 = box_at(p.0, sizes.adult.0, sizes.adult.1);
                        commands.entity(e).remove::<GrowAt>().insert(Recheck);
                        commands.queue(move |w: &mut World| crate::attr_mod::grow_up(w, e));
                        grown.0.push(e);
                        events.grew.push(e);
                    }
                }
            }
        }
    }
}
