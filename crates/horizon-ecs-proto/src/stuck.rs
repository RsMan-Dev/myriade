//! The arrows and stingers stuck in a creature (`arrowCount`, `stingerCount`) and the ticks before each falls out (`remove_*_time`).
//! Vanilla writes no NBT key for them, so they are not saved.
use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Stuck {
    pub arrows: i32,
    pub stingers: i32,
    pub remove_arrow_time: i32,
    pub remove_stinger_time: i32,
}

/// Sets both counts, keeping the times; two zeros remove the component.
pub fn set(w: &mut World, e: Entity, arrows: i32, stingers: i32) {
    let s = Stuck { arrows, stingers, ..get(w, e) };
    put(w, e, s);
}

/// The counts and times; the default without the component.
pub fn get(w: &World, e: Entity) -> Stuck {
    w.get::<Stuck>(e).copied().unwrap_or_default()
}

/// Writes the whole state; two zero counts remove the component.
pub fn put(w: &mut World, e: Entity, s: Stuck) {
    if s.arrows == 0 && s.stingers == 0 {
        w.entity_mut(e).remove::<Stuck>();
    } else {
        w.entity_mut(e).insert(s);
    }
}

pub fn register(s: &mut Schedule) {
    s.add_systems(tick.run_if(|w: &World| crate::goals::sparse_any::<Stuck>(w)));
}

/// `Mob.Stuck.tick` on the stuck entities only: one in a count's worth of `20 * (30 - count)` ticks falls out.
fn tick(mut q: Query<(Entity, &mut Stuck)>, mut commands: Commands) {
    for (e, mut s) in &mut q {
        let s = &mut *s;
        decay(&mut s.arrows, &mut s.remove_arrow_time);
        decay(&mut s.stingers, &mut s.remove_stinger_time);
        if s.arrows == 0 && s.stingers == 0 {
            commands.entity(e).remove::<Stuck>();
        }
    }
}

fn decay(count: &mut i32, remove_time: &mut i32) {
    if *count > 0 {
        if *remove_time <= 0 {
            *remove_time = 20i32.wrapping_mul(30i32.wrapping_sub(*count));
        }
        *remove_time = remove_time.wrapping_sub(1);
        if *remove_time <= 0 {
            *count -= 1;
        }
    }
}
