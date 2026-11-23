//! The experience orb of the frame against Horizon's own (`ExperienceOrb.tick` run on every orb every tick, with the player it follows),
//! in the same world: its fall and bounce, the attraction by a player, the merge of orbs of one value, the 6 000 ticks, and the award of
//! an amount in orbs. `HORIZON_TEST_RESOURCES` set.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use myriade_engine_lib::entities::ExperienceOrb;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::goals::*;
use horizon_ecs_proto::item_reference::*;
use horizon_ecs_proto::orb::*;

struct Scene {
    h: Herd,
    o: OrbsOriginal,
    ecs: Vec<Entity>,
}
fn scene(player: Option<Vec3>) -> Scene {
    let mut h = Herd::with_slab_awake(16, true, false);
    let mw = h.world.resource::<Level>().mw.0;
    let mut players = Vec::new();
    if let Some(p) = player {
        h.world.resource_mut::<Online>().set(Player { id: 1, x: p.x, y: p.y, z: p.z, main: 0, off: 0, ..Default::default() });
        players.push((1, p));
    }
    Scene { h, o: OrbsOriginal { mw, orbs: Vec::new(), players }, ecs: Vec::new() }
}
impl Scene {
    fn orb(&mut self, value: i32, at: Vec3, v: Vec3) {
        let id = self.o.orbs.len() as u32 + 1;
        self.o.orbs.push(ExperienceOrb::new(id, at, v, value));
        self.ecs.push(spawn_orb(&mut self.h.world, value, 1, at, v));
    }
    fn step(&mut self) {
        self.o.tick();
        self.h.tick();
    }
    fn alive(&self) -> Vec<(u32, u32, Vec3)> {
        let mut v: Vec<_> = self.ecs.iter().filter_map(|&e| self.h.world.get::<Orb>(e).map(|o| (o.id, o.count, self.h.world.get::<Pos>(e).unwrap().0))).collect();
        v.sort_by_key(|x| x.0);
        v
    }
    fn originals(&self) -> Vec<(u32, u32, Vec3)> {
        let mut v: Vec<_> = self.o.orbs.iter().map(|o| (o.id, o.count, o.position)).collect();
        v.sort_by_key(|x| x.0);
        v
    }
}
fn dist(a: Vec3, b: Vec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// An orb thrown with a player 6 blocks away: it falls, bounces and is drawn to the player; the two paths agree tick by tick.
#[test]
fn an_orb_falls_and_is_drawn_to_a_player_as_the_original() {
    if !have_data() {
        return;
    }
    let mut s = scene(Some(Vec3::new(5.5, 1.0, 0.5)));
    s.orb(3, Vec3::new(0.5, 4.0, 0.5), Vec3::new(0.1, 0.2, 0.0));
    let mut worst = 0.0_f64;
    for t in 1..=80 {
        s.step();
        let (a, b) = (s.alive(), s.originals());
        assert_eq!(a.len(), b.len(), "tick {t}");
        worst = worst.max(dist(a[0].2, b[0].2));
        assert!(dist(a[0].2, b[0].2) < 2e-2, "tick {t}: ecs {:?} original {:?}", a[0].2, b[0].2);
    }
    eprintln!("worst gap of an orb over 80 ticks: {worst:.5}");
}

/// Two orbs of one value whose ids are 40 apart, side by side: they merge at the tick the original merges them (the scan of tick 1 modulo 20).
#[test]
fn two_orbs_merge_at_the_tick_the_original_does() {
    if !have_data() {
        return;
    }
    let mut s = scene(None);
    s.orb(7, Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO);
    for k in 1..40 {
        s.orb(1, Vec3::new(-10.0 + 0.6 * f64::from(k), 1.0, 8.0), Vec3::ZERO);
    }
    s.orb(7, Vec3::new(0.7, 1.0, 0.5), Vec3::ZERO);
    let mut when = (None, None);
    for t in 1..=60 {
        s.step();
        if when.0.is_none() && s.originals().iter().any(|o| o.1 == 2) {
            when.0 = Some(t);
        }
        if when.1.is_none() && s.alive().iter().any(|o| o.1 == 2) {
            when.1 = Some(t);
        }
    }
    eprintln!("merged at tick: original {:?}, ecs {:?}", when.0, when.1);
    assert!(when.0.is_some() && when.0 == when.1, "{when:?}");
}

/// An orb at rest costs nothing and is gone at 6 000 ticks of age, at the tick the original removes it.
#[test]
fn an_orb_expires_when_the_original_does() {
    if !have_data() {
        return;
    }
    let mut s = scene(None);
    s.orb(3, Vec3::new(0.5, 1.0, 0.5), Vec3::ZERO);
    let (mut a, mut b) = (None, None);
    for t in 1..=6100 {
        s.step();
        if t == 100 {
            assert!(s.h.world.get::<Moving>(s.ecs[0]).is_none(), "the orb at rest still moves");
        }
        if b.is_none() && s.originals().is_empty() {
            b = Some(t);
        }
        if a.is_none() && s.alive().is_empty() {
            a = Some(t);
        }
    }
    eprintln!("expired at tick: original {b:?}, ecs {a:?}");
    assert!(a.is_some() && a == b, "{a:?} {b:?}");
}

/// `ExperienceOrb.award`: an amount is split into orbs of the largest values (`getExperienceValue`), and a player collects what an orb is worth.
#[test]
fn an_amount_is_awarded_in_orbs_of_the_original_sizes() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let mut r = horizon_core::random::LegacyRandom::new(3);
    award(&mut h.world, Vec3::new(2.5, 1.0, 2.5), 100, &mut r);
    let mut values: Vec<i32> = h.world.query::<&Orb>().iter(&h.world).map(|o| o.value).collect();
    values.sort_unstable();
    let (mut want, mut left) = (Vec::new(), 100u32);
    while left > 0 {
        let v = ExperienceOrb::split(left);
        left -= v;
        want.push(v as i32);
    }
    want.sort_unstable();
    assert_eq!(values, want);
    let orb = h.world.query::<(Entity, &Orb)>().iter(&h.world).find(|(_, o)| o.value == 73).map(|x| x.0).unwrap();
    assert_eq!(collect(&mut h.world, orb), Some(73));
    assert!(h.world.get::<Orb>(orb).is_none(), "an orb of one is gone once collected");
}

/// An animal's death is worth 1 + nextInt(3) experience: the orbs of a killed cow add up to 1, 2 or 3 with one chance in three each.
#[test]
fn a_killed_animal_leaves_one_to_three_experience() {
    if !have_data() || wrong_species(&["minecraft:cow"]) {
        return;
    }
    let mut h = Herd::with_slab_awake(16, true, false);
    let mut tally = [0usize; 4];
    for k in 0..300 {
        let e = spawn_species(&mut h.world, species(), 0.5, 0.5, 21 + k as u64);
        // An animal leaves experience only when a player hurt it lately (`lastHurtByPlayerMemory`).
        h.world.entity_mut(e).insert(horizon_ecs_proto::credit::PlayerCredit { player: Some(1), uuid: None, until: u64::MAX });
        h.hit(e, 1000.0, (0.0, 0.0));
        h.tick();
        let total: i32 = h.world.query::<&Orb>().iter(&h.world).map(|o| o.value * o.count as i32).sum();
        tally[total.clamp(0, 3) as usize] += 1;
        let ids: Vec<Entity> = h.world.query_filtered::<Entity, With<Orb>>().iter(&h.world).collect();
        for o in ids {
            h.world.entity_mut(o).despawn();
        }
    }
    eprintln!("experience of 300 deaths: {tally:?}");
    assert!(tally[0] == 0 && tally[1] > 60 && tally[2] > 60 && tally[3] > 60, "{tally:?}");
}
