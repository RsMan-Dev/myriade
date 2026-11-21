//! The temptation cooldown against the real one (`HORIZON_TEST_RESOURCES` set, process species a tempted one): four mobs, awake, stand
//! around a player who holds the tempt item for 1200 ticks. A temptation that stops must wait for its cooldown count
//! (`CountDownTemptation`, 60 ticks at least) before it starts again: the count of tempted starts (rising edges) must be the `Mob`'s.
//! No skip by species name: a species the item does not tempt starts no temptation on either side, and the assert on the Mob's starts says so.
//! The copper golem's door and transport against the real ones: a row of doors between two golems, and a copper chest the golem empties.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::MobEvent;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::host_out::HostEvent;
use horizon_ecs_proto::{blocks, brain::*, cow::*, goals::*};
use std::sync::Arc;

fn skip() -> bool {
    !have_data()
}

fn skip_golem() -> bool {
    !have_data() || wrong_species(&["minecraft:copper_golem"])
}

/// A copper golem of the table, awake as the `Mob`'s (`brain_vs_mob`'s pair).
fn ecs_golem(h: &mut Herd, x: f64, z: f64, seed: u64) -> Entity {
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let e = spawn_species(&mut h.world, &sp, x, z, seed);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    e
}

#[test]
fn the_temptation_restarts_after_its_cooldown_as_the_mob_does() {
    if skip() {
        return;
    }
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    h.tick();
    // A species without goals (a minecart, an item, a display) has nothing to tempt.
    if species().goals.entries.is_empty() {
        return;
    }
    let mut ids = Vec::new();
    for k in 0..4usize {
        let (x, z) = (-6.0 + 4.0 * k as f64, -4.0);
        let i = r.add(Vec3::new(x, 1.0, z));
        let e = ecs_cow(&mut h, x, 1.0, z, 9 + k as u64);
        ids.push((i, e));
    }
    h.tick();
    let (item, name) = tempt_item();
    let (mut mob_starts, mut ecs_starts) = (0usize, 0usize);
    let mut was = vec![(false, false); ids.len()];
    for _ in 1..=1200 {
        let p = player_view(Vec3::new(0.0, 1.0, 0.0), Some(name));
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 1.0, z: 0.0, main: item, off: 0, ..Default::default() });
        r.step(Some(&p));
        h.tick();
        for (k, &(i, e)) in ids.iter().enumerate() {
            let now = (r.names(i).contains(&"Tempt"), h.world.get::<Tempting>(e).is_some());
            if now.0 && !was[k].0 {
                mob_starts += 1;
            }
            if now.1 && !was[k].1 {
                ecs_starts += 1;
            }
            was[k] = now;
        }
    }
    // A species whose goals hold no tempt goal starts none on either side; the Mob's count then says it, not a skip by name.
    let tempted = species().goals.entries.iter().any(|en| std::ptr::eq(en.module, &horizon_ecs_proto::behaviours::tempt::MODULE));
    // `tempt_item` gives the item of the cow, the pig and the chicken: for these the Mob must be tempted. For another species the item
    // may not be the one it likes (a bee wants flowers): the Mob starts none and the equality below then asks the ECS for none too.
    if tempted && matches!(species().name, "minecraft:cow" | "minecraft:pig" | "minecraft:chicken") {
        assert!(mob_starts > 0, "the mobs were never tempted by the item");
    }
    assert_eq!(ecs_starts, mob_starts);
}

/// Two golems on each side of a row of closed doors (x = 0, z in -4..=4) open some of them on their way, on the `Mob` and in the ECS.
#[test]
fn a_golem_opens_a_door_on_its_way_as_the_mob_does() {
    if skip_golem() {
        return;
    }
    const DOOR: &[(&str, &str)] = &[("facing", "east"), ("half", "lower"), ("hinge", "left"), ("open", "false"), ("powered", "false")];
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    for z in -4..=4 {
        r.set_block_props(0, 1, z, "minecraft:oak_door", DOOR);
        h.set_block_state(0, 1, z, "minecraft:oak_door", DOOR);
    }
    h.tick();
    for z in [-2.0, 0.0, 2.0] {
        r.add(Vec3::new(-3.0, 1.0, z));
        r.add(Vec3::new(3.0, 1.0, z));
        ecs_golem(&mut h, -3.0, z, 11 + z as u64);
        ecs_golem(&mut h, 3.0, z, 21 + z as u64);
    }
    let (mut mob_was, mut ecs_was) = (vec![false; 9], vec![false; 9]);
    let (mut mob_opens, mut ecs_opens) = (0usize, 0usize);
    for _ in 0..1200 {
        r.step(None);
        h.tick();
        for (k, z) in (-4..=4).enumerate() {
            let mob = r.prop(0, 1, z, "open").as_deref() == Some("true");
            let ecs = blocks::property(blocks::world_of(&h.world), [0, 1, z], "open").as_deref() == Some("true");
            mob_opens += usize::from(mob && !mob_was[k]);
            ecs_opens += usize::from(ecs && !ecs_was[k]);
            mob_was[k] = mob;
            ecs_was[k] = ecs;
        }
    }
    assert!(mob_opens > 0, "the mobs never opened a door");
    assert!(ecs_opens > 0, "the golems never opened a door");
}

/// A copper chest of 8 cobblestones at 2 0 with an empty hand: the golem takes from it on both sides (a `ChestSlot` event each).
#[test]
fn a_golem_takes_from_a_copper_chest_as_the_mob_does() {
    if skip_golem() {
        return;
    }
    let stack = || ItemStack { item: Arc::from("minecraft:cobblestone"), count: 8, damage: 0, extra: None };
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab_awake(60, true, false);
    h.tick();
    r.set_block(2, 1, 0, "minecraft:copper_chest");
    set_chest(r.level, 2, 1, 0, vec![Some(stack()), None, None]);
    h.set_block(2, 1, 0, "minecraft:copper_chest");
    h.tick();
    set_chest(*h.world.resource::<Level>(), 2, 1, 0, vec![Some(stack()), None, None]);
    r.add(Vec3::new(-1.0, 1.0, 0.5));
    ecs_golem(&mut h, -1.0, 0.5, 31);
    let (mut mob_took, mut ecs_took, mut mob_seen) = (false, false, 0usize);
    for _ in 0..600 {
        r.step(None);
        h.tick();
        mob_took |= r.events.get(mob_seen..).unwrap_or_default().iter().any(|e| matches!(e, MobEvent::ChestSlot { .. }));
        mob_seen = r.events.len();
        ecs_took |= h.world.resource_mut::<Events>().host.drain().any(|e| matches!(e, HostEvent::ChestSlot { .. }));
    }
    assert!(mob_took, "the Mob golem never took from the chest");
    assert!(ecs_took, "the ECS golem never took from the chest");
}

/// 30000 chests far from the golem (500 chunks away) cost it nothing: its search reads the chunks of its box from the index of chests,
/// never the whole store. The time of 1500 ticks of 8 golems that search is printed (before the index: a walk of 30000 entries per search).
#[test]
fn a_golem_search_does_not_walk_the_block_entities() {
    if skip_golem() {
        return;
    }
    let stack = || ItemStack { item: Arc::from("minecraft:cobblestone"), count: 8, damage: 0, extra: None };
    let mut h = Herd::with_slab_awake(60, true, false);
    h.tick();
    h.set_block(2, 1, 0, "minecraft:copper_chest");
    set_chest(*h.world.resource::<Level>(), 2, 1, 0, vec![Some(stack()), None, None]);
    for i in 0..30000 {
        set_chest(*h.world.resource::<Level>(), 8000 + (i % 200) * 3, 1, 8000 + (i / 200) * 3, vec![None; 3]);
    }
    for k in 0..8u64 {
        ecs_golem(&mut h, -1.0 - k as f64 * 0.01, 0.5, 31 + k);
    }
    let mut took = 0usize;
    let t = std::time::Instant::now();
    for _ in 0..1500 {
        h.tick();
        took += h.world.resource_mut::<Events>().host.drain().filter(|e| matches!(e, HostEvent::ChestSlot { .. })).count();
    }
    eprintln!("golem search with 30000 far chests: 1500 ticks in {:?}, {took} chest slot events", t.elapsed());
    assert!(took > 0, "no golem took from the near chest");
}
