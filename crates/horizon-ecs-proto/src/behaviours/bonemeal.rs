//! Le villageois utilise la poudre d'os sur une culture non mûre à portée (`UseBonemeal`, `villager_brain.rs:1713-1745`,
//! tick `3044-3075`) : contrôle toutes les 10 ticks, pas dans les 160 ticks d'une session. La pousse est un paramètre
//! (`grow`, état suivant de la culture, interface supposée) ; la poudre sort de l'inventaire (`inventory::take`).
use bevy_ecs::prelude::Entity;
use bevy_ecs::prelude::World;
use super::block_reach::{block_at, look_block, pick_in_reach, walk_to, Pred};
use super::inventory;
use crate::blocks;
use crate::brain::*;
use crate::cow::now_of;
use bevy_ecs::prelude::Component;
use horizon_client_lib::mobs::MobWorld;
use horizon_world::registry::BlockStateId;

pub static BEHAVIOUR: Behaviour = Behaviour {
    name: "minecraft:villager_use_bonemeal",
    mems: &[&WALK_TARGET, &LOOK_TARGET],
    can_start,
    start,
    tick,
    stop,
    ..Behaviour::DEFAULT
};

const BONE_MEAL: &str = "minecraft:bone_meal";

pub struct Params {
    /// A culture that bone meal can still grow (`bonemeal_valid`: a crop below its maximum age).
    pub growable: Pred,
    pub grow: fn(&'static MobWorld<'static>, [i32; 3]) -> Option<BlockStateId>,
}

/// The last session (tick, 0 before the first) and the crop it works on.
#[derive(Component, Default, Clone, Copy)]
pub struct Session {
    pub last: u64,
}

fn can_start(w: &mut World, e: Entity, c: &Ctx) -> bool {
    if walk_target(w, e).is_some() || look_target(w, e).is_some() {
        return false;
    }
    let now = now_of(w, e);
    if now % 10 != 0 {
        return false;
    }
    let last = w.get::<Session>(e).map_or(0, |s| s.last);
    if last != 0 && last + 160 > now {
        return false;
    }
    if inventory::count(w, e, BONE_MEAL) == 0 {
        return false;
    }
    pick_in_reach(w, e, c.get::<Params>().growable).is_some()
}

fn start(w: &mut World, e: Entity, c: &Ctx) {
    let Some(q) = pick_in_reach(w, e, c.get::<Params>().growable) else { return };
    walk_to(w, e, q, 0.5, 1);
    look_block(w, e, q);
}

/// Within reach of the crop: take one bone meal and grow it, then the session ends.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(WalkTarget { target: Tracker::Block(q), .. }) = walk_target(w, e) else { return false };
    let mw = blocks::world_of(w);
    if !(p.growable)(mw, q) {
        return false;
    }
    let Some(here) = block_at(w, e) else { return false };
    if (0..3).all(|i| (here[i] - q[i]).abs() <= 1) {
        if inventory::take(w, e, BONE_MEAL, 1) {
            if let Some(id) = (p.grow)(mw, q) {
                blocks::set_state_id(w, q, id);
            }
        }
        let now = now_of(w, e);
        w.entity_mut(e).insert(Session { last: now });
        return false;
    }
    true
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    erase(w, e, &WALK_TARGET);
    erase(w, e, &LOOK_TARGET);
}
