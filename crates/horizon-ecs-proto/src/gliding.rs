//! `LivingEntity.setFallFlying` (key `FallFlying`): the creature glides with its elytra. The component stores the world's tick of the
//! start; `fall_fly_ticks` is the time since, read on demand (no count per tick).
use crate::cow::{now_of, Contact, Drive, Pos};
use crate::equipment_wear::{wear_glider, Wear};
use crate::goals::{equipped, with_rng, Slot};
use crate::mount::Riding;
use bevy_ecs::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct FallFlying {
    pub since: u32,
}

/// Starts (`on`) or stops the glide, from the entity's own time now.
pub fn set(w: &mut World, e: Entity, on: bool) {
    if on {
        let since = now_of(w, e) as u32;
        w.entity_mut(e).insert(FallFlying { since });
    } else {
        w.entity_mut(e).remove::<FallFlying>();
    }
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.fall_flying = on;
    }
}

/// The ticks of the glide so far; 0 when not gliding.
pub fn ticks(w: &World, e: Entity) -> i32 {
    w.get::<FallFlying>(e).map_or(0, |f| (now_of(w, e) as u32).wrapping_sub(f.since) as i32)
}

/// `LivingEntity.canGlide`: the glide holds only in the air, unridden, not levitating, with a glider worn.
pub fn keeps_gliding(on_ground: bool, riding: bool, levitating: bool, glider: bool) -> bool {
    glider && !on_ground && !riding && !levitating
}

/// `canGlideUsing`'s wear limit: a glider whose next point of damage would break it does not glide (`max` 0: never breaks).
pub fn durable_enough(damage: i32, max: i32) -> bool {
    max == 0 || damage.saturating_add(1) < max
}

/// `updateFallFlying`: the glider takes a point of wear on the 20th and 40th... glide tick (`ticks` counts this one).
pub fn wears_on(ticks: i32) -> bool {
    ticks % 20 == 0
}

/// `ELYTRA_GLIDE` is posted on every 10th glide tick, after the wear (`ticks` counts this one).
pub fn glide_event_on(ticks: i32) -> bool {
    ticks % 10 == 0
}

/// `Util.getRandom(slotsWithGliders, random)` on a wear tick: one draw from the entity's own random over the usable gliders (`canGlideUsing`:
/// a glider's equippable slot is the chest, so the list has one slot), which takes the wear.
pub fn pick_glider_slot(w: &mut World, e: Entity) -> Option<Slot> {
    let usable = [Slot::Chest];
    let i = with_rng(w, e, |r| r.next_int(usable.len() as i32));
    usable.get(i as usize).copied()
}

pub fn register(s: &mut Schedule) {
    s.add_systems(glide_tick.run_if(|w: &World| crate::goals::sparse_any::<FallFlying>(w)));
}

/// `LivingEntity.updateFallFlying` on the gliding entities only: a glide that cannot hold stops; else the glider takes its wear, and
/// the ELYTRA_GLIDE event comes on every tenth tick.
fn glide_tick(w: &mut World) {
    let gliding: Vec<Entity> = crate::queries::entities::<With<FallFlying>>(w);
    for e in gliding {
        let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
        let riding = w.get::<Riding>(e).is_some();
        let levitating = crate::effects::has(w, e, crate::effects::Kind::Levitation);
        let chest = equipped(w, e, Slot::Chest);
        let glider = crate::equipment_items::is_glider(chest);
        let damage = w.get::<Wear>(e).map_or(0, |wear| wear.0[1]);
        if !keeps_gliding(on_ground, riding, levitating, glider) || !durable_enough(damage, crate::equipment_items::durability_of(chest)) {
            set(w, e, false);
            continue;
        }
        let t = ticks(w, e);
        if wears_on(t) && pick_glider_slot(w, e).is_some() {
            wear_glider(w, e);
        }
        if glide_event_on(t) {
            let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x as f64, p.0.y as f64, p.0.z as f64]);
            w.resource_mut::<crate::goals::Events>().host.game_event_by(e, "minecraft:elytra_glide", at, false, false, false);
        }
    }
}
