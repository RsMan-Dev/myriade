//! The dragon's death (`D/death.rs`, `tick_dragon_death`; `dragon_killed` D:3035). Two hundred ticks driven by the flight tick (health 0): rays at 180..=200, orbs from tick 150 every 5 ticks, the rise, the level event 1028 at tick 1,
//! and at tick 200 the last orbs, the removal and, for the fight's dragon, `dragon_killed`.
use crate::cow::*;
use crate::dragon::fight::{DragonKilled, EndFight, FightEvents};
use crate::goals::Events;
use crate::dragon::flight::DragonRng;
use crate::rules::dragon_death::{self as rule, DEATH_TICKS};
use bevy_ecs::prelude::*;

/// `death_time` of Horizon: the ticks the dragon has been dying (saved as `DragonDeathTime`); rare state, so a sparse component.
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct DeathTime(pub i32);

/// The experience of the death: the fight's first kill gives 12 000, any other 500 (`tick_dragon_death`).
fn xp_of(w: &World, e: Entity) -> u32 {
    let f = w.resource::<EndFight>();
    rule::death_xp(f.dragon == Some(e), f.previously_killed)
}

/// `tick_dragon_death`, called by the flight in place of the flight while the health is 0: one tick of the 200, the rise included.
pub fn tick(w: &mut World, e: Entity) {
    let t = w.get::<DeathTime>(e).map_or(0, |d| d.0) + 1;
    w.entity_mut(e).insert(DeathTime(t));
    let at = w.get::<Pos>(e).unwrap().0;
    let mut rng = w.entity_mut(e).take::<DragonRng>().expect("dragon rng");
    if rule::is_blast_tick(t) {
        let o = rule::jitter(&mut rng.0, [8.0, 4.0, 8.0]);
        w.resource_mut::<Events>().host.particles("explosion_emitter", [at.x + o[0], at.y + 2.0 + o[1], at.z + o[2]], [0.0; 3], 0.0, 1);
    }
    let xp = xp_of(w, e);
    let drops = crate::blocks::world_of(w).mob_drops;
    if rule::is_orb_tick(t) && drops {
        crate::orb::award(w, at, rule::orb_share(xp, 0.08), &mut rng.1);
    }
    if t == 1 {
        w.resource_mut::<Events>().host.level_event(1028, [at.x, at.y, at.z].map(|v| v.floor() as i32), 0);
    }
    w.entity_mut(e).insert(rng);
    let rise = f64::from(0.1_f32);
    w.get_mut::<Pos>(e).unwrap().0.y += rise;
    if let Some(mut parts) = w.get_mut::<super::parts::Parts>(e) {
        parts.0.iter_mut().for_each(|p| p.pos[1] += rise);
    }
    if t < DEATH_TICKS {
        return;
    }
    if drops {
        let mut rng = w.entity_mut(e).take::<DragonRng>().unwrap();
        crate::orb::award(w, at, rule::orb_share(xp, 0.2), &mut rng.1);
    }
    let fight = w.resource::<EndFight>().dragon == Some(e);
    w.despawn(e);
    if fight {
        dragon_killed(w);
    }
}

/// The fight's dragon is dead (`dragon_killed`): the exit portal opens, a gateway is popped and placed, the egg appears on the first
/// kill, and the fight is told.
fn dragon_killed(w: &mut World) {
    crate::dragon::portal::activate_exit_portal(w);
    if let Some(g) = w.resource_mut::<EndFight>().gateways.pop() {
        let a = 2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 20.0 * f64::from(g));
        let p = [(96.0 * a.cos()).floor() as i32, 75, (96.0 * a.sin()).floor() as i32];
        w.resource_mut::<Events>().host.level_event(3000, p, 0);
        crate::dragon::portal::place_gateway(w, p);
    }
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
