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
use crate::goals::with_rng;
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
    /// Grows the crop by `ages` steps (the module draws 2 to 5, `BoneMealItem.growCrop`).
    pub grow: fn(&'static MobWorld<'static>, [i32; 3], i32) -> Option<BlockStateId>,
}

/// The last session's end (tick), the work cycles done (80 at most) and the next cycle's tick.
#[derive(Component, Default, Clone, Copy)]
pub struct Session {
    pub last: u64,
    pub worked: i32,
    pub next_work: u64,
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
    let last = w.get::<Session>(e).map_or(0, |s| s.last);
    w.entity_mut(e).insert(Session { last, worked: 0, next_work: 0 });
}

/// Within reach of the crop, at each cycle's tick (`UseBonemeal.tick`): a bone meal grows it by 2 to 5 ages and the
/// next cycle waits 40 ticks; without one the cycle is spent. The session ends after 80 cycles or when the brain stops it.
fn tick(w: &mut World, e: Entity, c: &Ctx) -> bool {
    let p = c.get::<Params>();
    let Some(WalkTarget { target: Tracker::Block(q), .. }) = walk_target(w, e) else { return false };
    let mw = blocks::world_of(w);
    let now = now_of(w, e);
    let mut s = w.get::<Session>(e).copied().unwrap_or_default();
    if s.worked >= 80 || !(p.growable)(mw, q) {
        w.entity_mut(e).insert(Session { last: now, ..s });
        return false;
    }
    let Some(here) = block_at(w, e) else { return false };
    if now >= s.next_work && (0..3).all(|i| (here[i] - q[i]).abs() <= 1) {
        if inventory::take(w, e, BONE_MEAL, 1) {
            let ages = with_rng(w, e, |r| r.next_int(4)) + 2;
            if let Some(id) = (p.grow)(mw, q, ages) {
                blocks::set_state_id(w, q, id);
            }
            s.next_work = now + 40;
        }
        s.worked += 1;
        w.entity_mut(e).insert(s);
    }
    true
}

fn stop(w: &mut World, e: Entity, _: &Ctx) {
    // `UseBonemeal.stop`: the session's end is noted whatever stopped it (duration, 80 cycles, no crop).
    let (now, s) = (now_of(w, e), w.get::<Session>(e).copied().unwrap_or_default());
    w.entity_mut(e).insert(Session { last: now, ..s });
    erase(w, e, &WALK_TARGET);
    erase(w, e, &LOOK_TARGET);
}
