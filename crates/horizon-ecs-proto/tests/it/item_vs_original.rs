//! The item on the ground of the frame against Horizon's own (`ItemEntity.tick`, run on every item every tick, in the order the game runs
//! it: the fluid push, the move, the bubble column, the drags, the burning and the lava, the merge, the 6 000 ticks), in the same world.
//! `HORIZON_TEST_RESOURCES` set. Plain blocks, water and lava: slime, honey, cobwebs and the bubble columns are Horizon's `ItemEntity`
//! alone.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::item::*;
use horizon_ecs_proto::item_reference::*;

struct Scene {
    h: Herd,
    o: Original,
    ecs: Vec<Entity>,
}
fn scene() -> Scene {
    let h = Herd::with_slab_awake(16, true, false);
    let mw = h.world.resource::<Level>().mw.0;
    Scene { h, o: Original { mw, items: Vec::new() }, ecs: Vec::new() }
}
impl Scene {
    fn drop(&mut self, name: &str, count: u32, at: Vec3, v: Vec3, delay: i64) {
        let id = self.o.items.len() as u32 + 1;
        self.o.items.push(original(id, ItemStack::new(name, count), at, v, delay));
        let e = drop_item(&mut self.h.world, ItemStack::new(name, count), at, v, delay);
        self.ecs.push(e);
    }
    fn step(&mut self) {
        self.o.tick();
        self.h.tick();
    }
    /// The ECS items still there, by id.
    fn alive(&self) -> Vec<(u32, u32, Vec3)> {
        let mut v: Vec<_> = self.ecs.iter().filter_map(|&e| self.h.world.get::<Item>(e).map(|i| (i.id, i.stack.count, self.h.world.get::<Pos>(e).unwrap().0))).collect();
        v.sort_by_key(|x| x.0);
        v
    }
    fn originals(&self) -> Vec<(u32, u32, Vec3)> {
        let mut v: Vec<_> = self.o.items.iter().map(|i| (i.id, i.stack.count, i.position)).collect();
        v.sort_by_key(|x| x.0);
        v
    }
}
fn dist(a: Vec3, b: Vec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// A dropped item falls, lands, bounces and slides to a stop: the position at each tick is Horizon's.
#[test]
fn a_dropped_item_follows_the_original_tick_by_tick() {
    if !have_data() {
        return;
    }
    let mut s = scene();
    s.drop("minecraft:cobblestone", 1, Vec3::new(0.5, 6.0, 0.5), Vec3::new(0.12, 0.2, -0.05), 10);
    let mut worst = 0.0_f64;
    for t in 1..=120 {
        s.step();
        let (a, b) = (s.alive(), s.originals());
        assert_eq!(a.len(), b.len(), "tick {t}");
        let d = dist(a[0].2, b[0].2);
        worst = worst.max(d);
        assert!(d < 5e-3, "tick {t}: ecs {:?} original {:?}", a[0].2, b[0].2);
    }
    eprintln!("worst gap of a dropped item over 120 ticks: {worst:.6}");
}

/// Two stacks that lie within half a block merge into one at the same tick, with the same count.
#[test]
fn two_stacks_merge_at_the_tick_the_original_merges_them() {
    if !have_data() {
        return;
    }
    let mut s = scene();
    s.drop("minecraft:sand", 10, Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 0);
    s.drop("minecraft:sand", 20, Vec3::new(0.7, 1.0, 0.5), Vec3::ZERO, 0);
    let mut when = (None, None);
    for t in 1..=100 {
        s.step();
        if when.0.is_none() && s.originals().len() == 1 {
            when.0 = Some(t);
        }
        if when.1.is_none() && s.alive().len() == 1 {
            when.1 = Some(t);
        }
    }
    eprintln!("merged at tick: original {:?}, ecs {:?}", when.0, when.1);
    assert!(when.0.is_some() && when.0 == when.1, "{when:?}");
    assert_eq!(s.alive()[0].1, 30);
    assert_eq!(s.originals()[0].1, 30);
}

/// A stack far from the rest never merges, and a resting one costs nothing: no item of the world is in motion.
#[test]
fn an_item_alone_sleeps() {
    if !have_data() {
        return;
    }
    let mut s = scene();
    s.drop("minecraft:sand", 10, Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 0);
    for _ in 0..60 {
        s.step();
    }
    let e = s.ecs[0];
    assert!(s.h.world.get::<Moving>(e).is_none(), "the item at rest still moves");
    assert!(dist(s.alive()[0].2, s.originals()[0].2) < 5e-3);
}

/// At 6 000 ticks of age the item is gone, at the tick the original removes it.
#[test]
fn an_item_expires_when_the_original_does() {
    if !have_data() {
        return;
    }
    let mut s = scene();
    s.drop("minecraft:stick", 1, Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO, 0);
    let mut when = (None, None);
    for t in 1..=6100 {
        s.step();
        if when.0.is_none() && s.originals().is_empty() {
            when.0 = Some(t);
        }
        if when.1.is_none() && s.alive().is_empty() {
            when.1 = Some(t);
        }
    }
    eprintln!("expired at tick: original {:?}, ecs {:?}", when.0, when.1);
    assert!(when.0.is_some() && when.0 == when.1, "{when:?}");
}

/// In water: slowed, lifted, pushed by the current; in lava: destroyed when its 5 health are gone.
#[test]
fn an_item_in_water_and_in_lava_follows_the_original() {
    if !have_data() {
        return;
    }
    let mut s = scene();
    for y in 1..=3 {
        for x in 3..=6 {
            for z in -1..=1 {
                s.h.set_block(x, y, z, "minecraft:water");
            }
        }
    }
    for x in 10..=12 {
        for z in -1..=1 {
            s.h.set_block(x, 1, z, "minecraft:lava");
        }
    }
    s.h.tick();
    s.drop("minecraft:cobblestone", 1, Vec3::new(4.5, 6.0, 0.5), Vec3::new(0.05, 0.0, 0.0), 0);
    s.drop("minecraft:oak_log", 1, Vec3::new(11.5, 4.0, 0.5), Vec3::ZERO, 0);
    let mut worst = 0.0_f64;
    let mut gone = (None, None);
    for t in 1..=100 {
        s.step();
        let (a, b) = (s.alive(), s.originals());
        if gone.0.is_none() && !b.iter().any(|x| x.0 == 2) {
            gone.0 = Some(t);
        }
        if gone.1.is_none() && !a.iter().any(|x| x.0 == 2) {
            gone.1 = Some(t);
        }
        if let (Some(x), Some(y)) = (a.iter().find(|x| x.0 == 1), b.iter().find(|x| x.0 == 1)) {
            worst = worst.max(dist(x.2, y.2));
        }
    }
    eprintln!("water: worst gap {worst:.5}; the log in lava is gone at tick: original {:?}, ecs {:?}", gone.0, gone.1);
    assert!(worst < 2e-2, "the item in water: {worst}");
    assert!(gone.0.is_some() && gone.1.is_some(), "{gone:?}");
    assert!((gone.0.unwrap() as i64 - gone.1.unwrap() as i64).abs() <= 2, "{gone:?}");
}
