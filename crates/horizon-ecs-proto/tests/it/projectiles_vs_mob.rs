//! The projectile capability against the real arrow of `Mob` (`arrows::tick`) in the same world (`HORIZON_TEST_RESOURCES`
//! set): the trajectory tick by tick (position and velocity identical), the block it sticks in, its life, the fall when the
//! block goes, the damage on a creature, and a second kind (a snowball) from the same data.
use crate::common;
use common::*;
use horizon_client_lib::{
    arrows::{self, ArrowScene},
    mobs::Who as MWho,
    projectiles::{Hit, Projectile, ProjectileKind, ProjectileWorld},
};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{cow::*, projectile::*, targeting::Who};

fn arrow(at: Vec3, v: Vec3) -> Projectile {
    let mut p = Projectile::new(ProjectileKind::Arrow { base_damage: 2.0, critical: false, effect: None }, at, None);
    p.velocity = v;
    p
}
/// One tick of the real arrow, in the world of `r`.
fn tick(r: &MobRef, p: &mut Projectile, targets: &[(MWho, Aabb)]) -> (bool, Hit) {
    let mw = r.w;
    let raining = |_: horizon_core::position::BlockPos| false;
    let scene = ArrowScene {
        world: mw.world,
        registry: mw.registry,
        facts: mw.facts,
        collision: mw.collision,
        fluids: mw.fluid_tables,
        loop_context: mw.loop_context,
        raining: &raining,
        mob_griefing: true,
        tnt_explodes: true,
        mob_hooks: None,
        owners: &(),
    };
    arrows::tick(p, &scene, targets)
}
fn flying(h: &Herd, e: Entity) -> bool {
    h.world.get::<Flying>(e).is_some()
}
use bevy_ecs::prelude::*;

/// Shot over the flat ground at a few angles: the same position and velocity at every tick, the same tick and place of the
/// hit on the ground, then stuck without a visit, and gone at the same tick (1 200 ticks of life).
#[test]
fn an_arrow_flies_sticks_and_expires_as_a_mob_s_does() {
    if !have_data() {
        return;
    }
    for (k, v) in [Vec3::new(0.7, 0.45, 0.2), Vec3::new(-0.5, 0.2, 0.6), Vec3::new(0.05, 0.6, 0.0)].into_iter().enumerate() {
        let (r, mut h) = flat_pair();
        let at = Vec3::new(0.5, 1.7, 0.5);
        let mut m = arrow(at, v);
        let e = launch(&mut h.world, Flight::ARROW, at, v, None);
        let (mut stuck_at, mut gone_at) = (None, None);
        for t in 1..=1300i64 {
            let (keep, hit) = tick(&r, &mut m, &[]);
            h.tick();
            if stuck_at.is_none() && matches!(hit, Hit::Block { .. }) {
                stuck_at = Some(t);
            }
            if !keep && gone_at.is_none() {
                gone_at = Some(t);
            }
            if gone_at.is_none() {
                let (p, ev) = (h.world.get::<Pos>(e).unwrap().0, h.world.get::<Vel>(e).unwrap().0);
                assert!((p.x - m.position.x).abs() + (p.y - m.position.y).abs() + (p.z - m.position.z).abs() < 1e-12, "arrow {k} t={t}: pos {p:?} / mob {:?}", m.position);
                assert!((ev.x - m.velocity.x).abs() + (ev.y - m.velocity.y).abs() + (ev.z - m.velocity.z).abs() < 1e-12, "arrow {k} t={t}: velocity {ev:?} / mob {:?}", m.velocity);
                assert_eq!(flying(&h, e), stuck_at.is_none(), "arrow {k} t={t}: in flight only until it sticks");
            } else {
                assert!(h.world.get_entity(e).is_err(), "arrow {k}: the ECS arrow is gone at the Mob's tick {t}");
                break;
            }
        }
        eprintln!("arrow {k}: sticks at tick {stuck_at:?}, gone at {gone_at:?}");
        assert!(stuck_at.is_some() && gone_at == stuck_at.map(|s| s + 1200));
    }
}

/// The block it is stuck in goes: both fall from rest, the same positions on every tick after.
#[test]
fn a_stuck_arrow_falls_when_its_block_goes() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    // A stone wall at x = 6: the arrow flies into its face.
    for y in 1..=4 {
        for z in -2..=2 {
            r.set_block(6, y, z, "minecraft:stone");
            h.set_block(6, y, z, "minecraft:stone");
        }
    }
    let (at, v) = (Vec3::new(0.5, 2.5, 0.5), Vec3::new(1.5, 0.0, 0.0));
    let mut m = arrow(at, v);
    let e = launch(&mut h.world, Flight::ARROW, at, v, None);
    let mut stuck_at = None;
    for t in 1..=60i64 {
        if t == 30 {
            assert!(stuck_at.is_some(), "stuck before the wall goes");
            r.set_block(5 + 1, 3, 0, "minecraft:air");
            h.set_block(6, 3, 0, "minecraft:air");
            assert!(!flying(&h, e));
        }
        let (_, hit) = tick(&r, &mut m, &[]);
        h.tick();
        if stuck_at.is_none() && matches!(hit, Hit::Block { .. }) {
            stuck_at = Some(t);
        }
        let (p, mp) = (h.world.get::<Pos>(e).unwrap().0, m.position);
        assert!((p.x - mp.x).abs() + (p.y - mp.y).abs() + (p.z - mp.z).abs() < 1e-12, "t={t}: pos {p:?} / mob {mp:?}");
        if t == 38 {
            assert!(p.y < 2.4, "it has fallen: {p:?}");
        }
    }
    assert!(h.world.get::<Stuck>(e).is_some(), "it fell and stuck again in the ground, as the Mob's did");
}

/// A cow in its way: the same tick and place of the hit, and the damage is `ceil(speed * base)` on the cow, with the
/// shooter as the source.
#[test]
fn an_arrow_hurts_a_creature_as_a_mob_s_does() {
    if horizon_ecs_proto::cow::skip_unless_small_body("an_arrow_hurts_a_creature_as_a_mob_s_does") {
        return;
    }
    if horizon_ecs_proto::cow::skip_unless_walker("an_arrow_hurts_a_creature_as_a_mob_s_does") {
        return;
    }
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    let pi = r.add(Vec3::new(8.5, 1.0, 0.5));
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let cow = *table.iter().find(|s| s.name == species().name).unwrap();
    let ce = spawn_species(&mut h.world, &cow, 8.5, 0.5, 11);
    h.world.get_mut::<ActiveUntil>(ce).unwrap().0 = 0;
    let shooter = spawn_species(&mut h.world, &cow, -8.5, 0.5, 12);
    h.world.get_mut::<ActiveUntil>(shooter).unwrap().0 = 0;
    let health0 = h.world.get::<Health>(ce).unwrap().0;
    let (at, v) = (Vec3::new(0.5, 1.4, 0.5), Vec3::new(2.0, 0.1, 0.0));
    let mut m = arrow(at, v);
    let e = launch(&mut h.world, Flight::ARROW, at, v, Some(Who::Entity(shooter)));
    let mut mob_hit = None;
    let mut ecs_gone = None;
    let mut mob_damage = 0.0;
    for t in 1..=20i64 {
        let target = (MWho::Mob(r.mobs[pi].id), r.mobs[pi].seen(None, r.w.items.unwrap()).bounds);
        let speed_before = m.velocity;
        let (_, hit) = tick(&r, &mut m, &[target]);
        h.tick();
        if mob_hit.is_none() && matches!(hit, Hit::Entity { .. }) {
            mob_hit = Some(t);
            m.velocity = speed_before;
            mob_damage = m.arrow_damage();
        }
        if ecs_gone.is_none() && h.world.get_entity(e).is_err() {
            ecs_gone = Some(t);
        }
    }
    eprintln!("hit at tick {mob_hit:?} (Mob) / gone at {ecs_gone:?} (ECS); damage {mob_damage}; cow {health0} -> {:?}", h.world.get::<Health>(ce).map(|x| x.0));
    assert!(mob_hit.is_some() && mob_hit == ecs_gone, "mob {mob_hit:?}, ecs {ecs_gone:?}");
    let lost = health0 - h.world.get::<Health>(ce).map_or(0.0, |x| x.0);
    assert_eq!(lost, mob_damage.min(health0), "the cow loses what the Mob's arrow would deal");
}

struct Open<'a>(&'a MobRef, Vec<(MWho, Aabb)>);
impl ProjectileWorld for Open<'_> {
    fn clip(&self, from: Vec3, to: Vec3) -> Option<(Vec3, [i32; 3], [i32; 3])> {
        let mw = self.0.w;
        let movement = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z);
        arrows::clip_block_hit(mw.world, mw.facts, mw.collision, from, movement).map(|(at, p, n)| (at, [p.x, p.y, p.z], n))
    }
    fn in_water(&self, _: Aabb) -> bool {
        false
    }
    fn targets(&self) -> &[(MWho, Aabb)] {
        &self.1
    }
}
/// A second kind from the same capability: a snowball (gravity 0.03, gone on a block), the same flight as `Mob`'s.
#[test]
fn a_snowball_is_the_same_capability_with_other_data() {
    if !have_data() {
        return;
    }
    let (r, mut h) = flat_pair();
    let (at, v) = (Vec3::new(0.5, 1.9, 0.5), Vec3::new(0.5, 0.25, -0.1));
    let mut m = Projectile::new(ProjectileKind::Snowball, at, None);
    m.velocity = v;
    let e = launch(&mut h.world, Flight::SNOWBALL, at, v, None);
    let world = Open(&r, Vec::new());
    for t in 1..=40i64 {
        let (keep, _) = m.tick(&world);
        h.tick();
        if !keep {
            assert!(h.world.get_entity(e).is_err(), "gone together at tick {t}");
            eprintln!("snowball gone at tick {t}");
            return;
        }
        let p = h.world.get::<Pos>(e).unwrap().0;
        assert!((p.x - m.position.x).abs() + (p.y - m.position.y).abs() + (p.z - m.position.z).abs() < 1e-9, "t={t}: pos {p:?} / mob {:?}", m.position);
    }
    panic!("the snowball never landed");
}
