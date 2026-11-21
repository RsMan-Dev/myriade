//! Le villageois sonne la cloche à portée (`RingBell`, `villager_brain.rs:2447-2463`). La cloche est un bloc
//! `minecraft:bell` à moins de 3 blocs (9 en carré) ; chaque sonnerie appelle `bell_heard::ring` et le son part par `events.host` (nom du son à confirmer, voir notes).
use super::bell_heard::ring;
use super::block_reach::block_at;
use crate::rules::ring_bell as rules;
use crate::blocks;
use crate::brain::*;
use bevy_ecs::prelude::*;
use crate::cow::Pos;
use crate::goals::{with_rng, Events};

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_ring_bell",
    mems: &[],
    one_shot: true,
    can_start,
    start,
    ..Behaviour::DEFAULT
};

const BELL: &str = "minecraft:bell";

/// Nearest bell within 3 blocks; a bell is rung with a 1 in 20 chance per check (`RingBell`).
fn bell_near(w: &World, e: Entity) -> Option<[i32; 3]> {
    let at = w.get::<Pos>(e)?.0;
    let c = block_at(w, e)?;
    let mw = blocks::world_of(w);
    let cells = (-3..=3).flat_map(|x| (-3..=3).flat_map(move |y| (-3..=3).map(move |z| [c[0] + x, c[1] + y, c[2] + z])));
    rules::nearest_within(cells.filter(|&q| mw.block_name(q[0], q[1], q[2]) == BELL), at, 9.0)
}

fn can_start(w: &mut World, e: Entity, _: &Ctx) -> bool {
    bell_near(w, e).is_some() && with_rng(w, e, |r| r.next_int(20)) == 0
}

fn start(w: &mut World, e: Entity, _: &Ctx) {
    let Some(p) = bell_near(w, e) else { return };
    ring(w, p);
    let at = [f64::from(p[0]) + 0.5, f64::from(p[1]) + 0.5, f64::from(p[2]) + 0.5];
    w.resource_mut::<Events>().host.sound("minecraft:block.bell.use", at, 2.0, 1.0);
}
