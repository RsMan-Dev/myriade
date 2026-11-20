//! The dragon's death (`D/death.rs`, `tick_dragon_death`; `dragon_killed` D:3035). Two hundred ticks of the dragon's own time, one
//! re-arm per tick while `Dying` is on it: rays at 180..=200, orbs from tick 150 every 5 ticks, the rise, the level event 1028 at tick 1,
//! and at tick 200 the last orbs, the removal and, for the fight's dragon, `dragon_killed`.
use crate::cow::*;
use crate::dragon::fight::{DragonKilled, EndFight, FightEvents};
use crate::goals::Events;
use crate::orb::spawn_orb;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

const DEATH_TICKS: u32 = 200;

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Dying {
    pub t: u32,
}

/// The experience of the death: the fight's first kill gives 12 000, any other 500 (`tick_dragon_death`).
fn xp_of(w: &World, e: Entity) -> i32 {
    let f = w.resource::<EndFight>();
    if f.dragon == Some(e) && !f.previously_killed { 12_000 } else { 500 }
}

/// The death starts (the dragon stops flying elsewhere; L2/L3 check `Dying`).
pub fn begin(w: &mut World, e: Entity) {
    w.entity_mut(e).insert(Dying::default());
    after(w, e, 1, tick);
}

/// `at` moved by a draw in -1..1 times `half` on each axis (the dragon's own random).
fn jitter(w: &mut World, e: Entity, at: Vec3, half: [f64; 3]) -> [f64; 3] {
    let mut d = [0.0; 3];
    for (i, v) in d.iter_mut().enumerate() {
        let r = w.get_mut::<Rng>(e).map_or(0.5, |mut r| r.next_f64());
        *v = (r * 2.0 - 1.0) * half[i];
    }
    [at.x + d[0], at.y + d[1], at.z + d[2]]
}

fn tick(w: &mut World, e: Entity) {
    let Some(t) = w.get::<Dying>(e).map(|d| d.t + 1) else { return };
    w.get_mut::<Dying>(e).unwrap().t = t;
    let at = w.get::<Pos>(e).unwrap().0;
    if t == 1 {
        w.resource_mut::<Events>().host.level_event(1028, [at.x, at.y, at.z], 0);
    }
    if (180..=DEATH_TICKS).contains(&t) {
        let p = jitter(w, e, at, [4.0, 2.0, 4.0]);
        w.resource_mut::<Events>().host.particles("explosion_emitter", p, [0.0; 3], 0.0, 1);
    }
    w.get_mut::<Pos>(e).unwrap().0.y += 0.1;
    let xp = xp_of(w, e);
    let drops = w.resource::<Rules>().mob_drops;
    if drops && t > 150 && t % 5 == 0 {
        spawn_orb(w, (xp as f32 * 0.08) as i32, 1, at, Vec3::ZERO);
    }
    if t < DEATH_TICKS {
        after(w, e, 1, tick);
        return;
    }
    if drops {
        spawn_orb(w, (xp as f32 * 0.2) as i32, 1, at, Vec3::ZERO);
    }
    w.despawn(e);
    if w.resource::<EndFight>().dragon == Some(e) {
        dragon_killed(w, at);
    }
}

/// The fight's dragon is dead (`dragon_killed`): the exit portal opens, a gateway is popped and placed, the egg appears on the first
/// kill, and the fight is told.
fn dragon_killed(w: &mut World, at: Vec3) {
    crate::dragon::portal::activate_exit_portal(w);
    if let Some(g) = w.resource_mut::<EndFight>().gateways.pop() {
        let a = 2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 20.0 * f64::from(g));
        let p = [(96.0 * a.cos()).floor() as i32, 75, (96.0 * a.sin()).floor() as i32];
        crate::dragon::portal::place_gateway(w, p);
    }
    w.resource_mut::<Events>().host.level_event(3000, [at.x, at.y, at.z], 0);
    let first = !w.resource::<EndFight>().previously_killed;
    if first {
        crate::dragon::portal::place_egg(w);
    }
    let (by, last_blow) = {
        let f = w.resource_mut::<EndFight>().into_inner();
        f.killed = true;
        f.previously_killed = true;
        f.dragon = None;
        (f.blast_by, f.last_blow)
    };
    w.resource_mut::<FightEvents>().killed.push(DragonKilled { by, last_blow });
}
