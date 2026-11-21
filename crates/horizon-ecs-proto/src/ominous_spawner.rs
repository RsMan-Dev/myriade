//! The ominous item spawner (`OminousItemSpawner`): an entity over a trial spawner's fight that, after 60 to 120 ticks, drops its item
//! (an item entity, or a projectile shot straight down) and goes. Its life is two deadlines of the wheel (`after`): the warning sound
//! 36 ticks before the drop and the drop itself. Its particles are a chain of wake-ups on the game ticks that are multiples of 5 while
//! it lives; nothing runs per tick, and a spawner that has ended costs nothing.
use crate::cow::{after, now_of, Clock, OwnTime, Pos};
use crate::goals::{next_uid, Events, Uid};
use crate::item::spawn_at_location;
use crate::projectile::{launch, Flight};
use crate::projectile_more;
use crate::rules::ominous_spawner::{about_to_spawn_delay, drop_delay, particle_count, particle_wait, shoot_down};
use crate::spawn_spawner::SpawnerRng;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
/// The level event of the spawn (`spawnItem`), and the game event that follows it.
const SPAWN_EVENT: i32 = 3021;

/// The spawner: the item it drops (`minecraft:air`: nothing) and the tick of its own life at which it does.
#[derive(Component, Clone, Debug)]
pub struct OminousSpawner {
    pub item: ItemStack,
    pub after: u32,
}

/// `OminousItemSpawner.create` at `at`, dropping `item` after `after_ticks` (the trial draws 60 to 120).
pub fn spawn(w: &mut World, item: ItemStack, at: Vec3, after_ticks: u32) -> Entity {
    crate::spawn_spawner::ensure(w, 0);
    let e = w.spawn((Pos(at), OwnTime, Uid(next_uid()), OminousSpawner { item, after: after_ticks }))
.id();
    crate::nbt::mark_saved(w, e, crate::species::lists::OMINOUS_ITEM_SPAWNER);
    w.entity_mut(e).insert(crate::cow::TypeName(crate::species::lists::OMINOUS_ITEM_SPAWNER));
    if let Some(delay) = about_to_spawn_delay(after_ticks) {
        after(w, e, delay, about_to_spawn);
    }
    after(w, e, drop_delay(after_ticks), drop_now);
    arm_particles(w, e);
    e
}

/// The block a position is in (`BlockPos.containing`).
fn block(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

/// Position, drop tick and own tick of a spawner that is still there; None once it has gone.
fn live(w: &World, e: Entity) -> Option<(Vec3, u32, u64)> {
    let s = w.get::<OminousSpawner>(e)?;
    Some((w.get::<Pos>(e)?.0, s.after.max(1), now_of(w, e)))
}

/// `tickServer` at `tickCount == after - 36`: the warning sound, at the centre of the block.
fn about_to_spawn(w: &mut World, e: Entity) {
    let Some((p, _, _)) = live(w, e) else { return };
    let c = block(p).map(|v| f64::from(v) + 0.5);
    w.resource_mut::<Events>().host.sound("block.trial_spawner.about_to_spawn_item", c, 1.0, 1.0);
}

/// `tickServer` at `tickCount >= after`: `spawnItem`, then the spawner is removed.
fn drop_now(w: &mut World, e: Entity) {
    let Some((p, _, _)) = live(w, e) else { return };
    let item = w.get::<OminousSpawner>(e).expect("live spawner").item.clone();
    w.despawn(e);
    if &*item.item == "minecraft:air" {
        return;
    }
    let at = block(p);
    match crate::species::lists::shot(&item.item) {
        Some((flight, power, uncertainty, event)) => {
            if let Some(event) = event {
                w.resource_mut::<Events>().host.level_event(event, at, 0);
            }
            let v = shoot_down(&mut w.resource_mut::<SpawnerRng>().0, power, uncertainty);
            let e = launch(w, flight, p, v, None);
            // A potion's contents (its item data) name the brews it splashes or lingers.
            if let (Some(potion), Ok(mut m)) = (item.potion().and_then(crate::effects::potion_id), w.get_entity_mut(e)) {
                m.insert(potion);
            }
        }
        None => {
            w.resource_scope(|w, mut rng: Mut<SpawnerRng>| {
                spawn_at_location(w, item, p, &mut rng.0);
            });
        }
    }
    let mut events = w.resource_mut::<Events>();
    events.host.level_event(SPAWN_EVENT, at, 1);
    events.host.game_event("minecraft:entity_place", [p.x, p.y, p.z]);
}

/// The projectile items of vanilla (`ProjectileItem` implementers): flight, power, uncertainty and the dispense event they set.
/// Eggs and the trident take the default (power 1.1, uncertainty 6.0); the bottles and the potions take the default with
/// power x1.25 and uncertainty x0.5. Any other item falls as an item.

/// `OminousItemSpawner.addParticles`, on a wake-up of the chain: the particles at the spawner's position, then the next wake-up.
fn particles(w: &mut World, e: Entity) {
    let Some((p, _, local)) = live(w, e) else { return };
    let count = particle_count(local);
    w.resource_mut::<Events>().host.particles("ominous_spawning", [p.x, p.y, p.z], [0.4; 3], 0.0, count);
    arm_particles(w, e);
}

/// The next wake-up of the particle chain: the next game tick that is a multiple of 5, if the drop has not come by then.
fn arm_particles(w: &mut World, e: Entity) {
    let Some((_, drop_at, local)) = live(w, e) else { return };
    let wait = particle_wait(w.resource::<Clock>().0);
    if u64::from(local) + wait < u64::from(drop_at) {
        after(w, e, wait as i32, particles);
    }
}
