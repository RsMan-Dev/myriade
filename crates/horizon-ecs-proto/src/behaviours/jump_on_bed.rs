//! Le bébé saute sur le lit le plus proche (`JumpOnBed`, `villager_brain.rs:1553-1580`, tick `2756-2776`) : 100 ticks,
//! 3 à 6 sauts, 5 ticks de pause entre deux sauts, arrêt si le lit disparaît ou si le bébé grandit.
use bevy_ecs::prelude::Entity;
use bevy_ecs::prelude::World;
use super::block_reach::{block_at, walk_to};
use crate::behaviours::bed::NEAREST_BED;
use crate::blocks;
use crate::brain::*;
use crate::goals::{with_rng, Jump};
use bevy_ecs::prelude::Component;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_jump_on_bed",
    mems: &[&WALK_TARGET, &NEAREST_BED],
    one_shot: true,
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

pub struct Params {
    /// True for a baby (`is_baby`), the only villager that plays here.
    pub baby: fn(&World, Entity) -> bool,
}

/// The running session; absent outside it.
#[derive(Component, Clone, Copy, Debug)]
pub struct Bounce {
    pub bed: [i32; 3],
    pub time_left: i32,
    pub jumps: i32,
    pub cooldown: i32,
}

fn bed(w: &World, e: Entity) -> Option<[i32; 3]> {
    match get(w, e, &NEAREST_BED) {
        Some(MemValue::Block(p)) => Some(*p),
        _ => None,
    }
}

fn is_bed(w: &World, p: [i32; 3]) -> bool {
    blocks::world_of(w).block_name(p[0], p[1], p[2]).ends_with("_bed")
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if !(c.get::<Params>().baby)(w, e) || present(w, e, &WALK_TARGET) {
        return false;
    }
    bed(w, e).is_some()
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    let Some(p) = bed(w, e) else { return };
    let jumps = 3 + with_rng(w, e, |r| r.next_int(4));
    w.entity_mut(e).insert(Bounce { bed: p, time_left: 100, jumps, cooldown: 0 });
    walk_to(w, e, p, 0.5, 0);
}

/// Stops when the bed is gone, the entity grew up, or the session is out of time or jumps.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let Some(mut b) = w.get::<Bounce>(e).copied() else { return false };
    let baby = (c.get::<Params>().baby)(w, e);
    let here = block_at(w, e).unwrap_or([0, 0, 0]);
    let on_or_over = is_bed(w, here) || is_bed(w, [here[0], here[1] - 1, here[2]]);
    if !baby || !is_bed(w, b.bed) || !(on_or_over || b.time_left > 0) || (on_or_over && b.jumps <= 0) {
        return false;
    }
    if !on_or_over {
        b.time_left -= 1;
    } else if b.cooldown > 0 {
        b.cooldown -= 1;
    } else if is_bed(w, here) {
        w.entity_mut(e).insert(Jump(true));
        b.jumps -= 1;
        b.cooldown = 5;
    }
    w.entity_mut(e).insert(b);
    true
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    w.entity_mut(e).remove::<Bounce>();
    erase(w, e, &WALK_TARGET);
}
