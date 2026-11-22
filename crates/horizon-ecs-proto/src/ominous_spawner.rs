//! The ominous item spawner (`OminousItemSpawner`): an entity over a trial spawner's fight that, after 60 to 120 ticks, drops its item
//! (an item entity, or a projectile shot straight down) and goes. Its life is two deadlines of the wheel (`after`): the warning sound
//! 36 ticks before the drop and the drop itself. Its particles are a chain of wake-ups on the game ticks that are multiples of 5 while
//! it lives; nothing runs per tick, and a spawner that has ended costs nothing.
use crate::cow::{after, now_of, Clock, OwnTime, Pos};
use crate::goals::{next_uid, Events, Uid};
use crate::item::spawn_at_location;
use crate::projectile::launch;
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
    // `SoundSource.NEUTRAL` (index 6).
    w.resource_mut::<Events>().host.sound_from(6, "block.trial_spawner.about_to_spawn_item", c, 1.0, 1.0);
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
            // A potion's item (its data) names the brews it splashes or lingers.
            if let Ok(mut m) = w.get_entity_mut(e) {
                m.insert(crate::arrow_more::ThrownItem(item.clone()));
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


#[cfg(test)]
mod restore_format {
    use crate::cow::Herd;
    use horizon_core::nbt::{parse_snbt, Tag};
    /// A spawner as vanilla saves it (`Entity.saveWithoutId` plus `OminousItemSpawner.addAdditionalSaveData`).
    const SAVED: &str = r#"{id:"minecraft:ominous_item_spawner",UUID:[I;128,0,0,1],Pos:[6.25d,102.75d,4.875d],item:{id:"minecraft:diamond",count:1},spawn_item_after_ticks:40L}"#;

    fn restored(snbt: &str) -> Option<(String, u32)> {
        let mut h = Herd::with_slab_awake(16, true, false);
        let Tag::Compound(c) = parse_snbt(snbt).unwrap().0 else { panic!("a compound") };
        let e = crate::nbt::restore(&mut h.world, &c)?;
        let s = h.world.get::<super::OminousSpawner>(e)?;
        Some((s.item.item.to_string(), s.after))
    }

    /// The saved spawner restores its drop and its delay; an absent `item` is empty (no drop), as the load of vanilla reads it.
    #[test]
    fn a_saved_spawner_restores_its_item_and_delay_and_an_empty_one_drops_nothing() {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            return;
        }
        assert_eq!(restored(SAVED), Some(("minecraft:diamond".to_string(), 40)));
        let empty = SAVED.replace(",item:{id:\"minecraft:diamond\",count:1}", "");
        assert_eq!(restored(&empty), Some(("minecraft:air".to_string(), 40)));
    }
}
