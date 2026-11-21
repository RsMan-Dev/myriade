//! The projectiles of `projectile_more.rs` against `Mob`'s (`Projectile::tick`) over the flat ground: the same position on every
//! tick, exactly, and gone at the same tick as the Mob's.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::projectiles::{Projectile, ProjectileKind, ProjectileWorld};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{cow::*, projectile::*, projectile_more::*};

struct Open<'a>(&'a MobRef, Vec<(horizon_client_lib::mobs::Who, Aabb)>);
impl ProjectileWorld for Open<'_> {
    fn clip(&self, from: Vec3, to: Vec3) -> Option<(Vec3, [i32; 3], [i32; 3])> {
        let mw = self.0.w;
        let movement = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z);
        horizon_client_lib::arrows::clip_block_hit(mw.world, mw.facts, mw.collision, from, movement).map(|(at, p, n)| (at, [p.x, p.y, p.z], n))
    }
    fn in_water(&self, _: Aabb) -> bool {
        false
    }
    fn targets(&self) -> &[(horizon_client_lib::mobs::Who, Aabb)] {
        &self.1
    }
}

/// Each kind thrown over the flat ground: the same position on every tick, and gone at the same tick as the Mob's.
#[test]
fn thrown_and_shot_kinds_fly_as_the_mob_s_do() {
    if !have_data() {
        return;
    }
    let kinds = [
        (ProjectileKind::Egg, EGG),
        (ProjectileKind::EnderPearl, ENDER_PEARL),
        (ProjectileKind::ExperienceBottle, EXPERIENCE_BOTTLE),
        (ProjectileKind::DragonFireball, DRAGON_FIREBALL),
        (ProjectileKind::WindCharge, WIND_CHARGE),
        (ProjectileKind::LlamaSpit, Flight::SPIT),
    ];
    for (k, (kind, flight)) in kinds.into_iter().enumerate() {
        let (r, mut h) = flat_pair();
        let (at, v) = (Vec3::new(0.5, 1.9, 0.5), Vec3::new(0.5, -0.25, -0.1));
        let mut m = Projectile::new(kind, at, None);
        m.velocity = v;
        let e = launch(&mut h.world, flight, at, v, None);
        let world = Open(&r, Vec::new());
        let mut gone = None;
        for t in 1..=200i64 {
            let (keep, _) = m.tick(&world);
            h.tick();
            if !keep {
                assert!(h.world.get_entity(e).is_err(), "kind {k}: gone together at tick {t}");
                gone = Some(t);
                break;
            }
            let p = h.world.get::<Pos>(e).unwrap().0;
            assert!(p.x == m.position.x && p.y == m.position.y && p.z == m.position.z, "kind {k} t={t}: pos {p:?} / mob {:?}", m.position);
        }
        eprintln!("kind {k}: gone at {gone:?}");
        assert!(gone.is_some(), "kind {k} never landed");
    }
}

/// A wither skull with no owner: magic 5 at the creature it hits, and the wither effect on it (the blow hurts).
#[test]
fn a_wither_skull_without_owner_hurts_by_5_and_withers_the_creature() {
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let cow = ecs_cow(&mut h, 0.5, 1.0, 2.5, 5);
    h.tick();
    let before = health(&h, cow);
    let e = launch(&mut h.world, Flight::WITHER_SKULL, Vec3::new(0.5, 1.5, 0.5), Vec3::new(0.0, 0.0, 0.6), None);
    for _ in 0..40 {
        h.tick();
        if h.world.get_entity(e).is_err() {
            break;
        }
    }
    assert!(h.world.get_entity(e).is_err(), "the skull is gone at the creature");
    assert_eq!(before - health(&h, cow), 5.0, "magic 5 without an owner");
    assert!(h.world.get::<horizon_ecs_proto::effects::Effects>(cow).is_some_and(|f| !f.list.is_empty()), "the wither effect");
}

/// A flask with its `PotionId` splashes its table's brews where it breaks: the creature near takes poison, the far one nothing.
#[test]
fn a_potion_with_its_id_splashes_its_brews_by_distance() {
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let near = ecs_cow(&mut h, 2.5, 1.0, 0.5, 6);
    let far = ecs_cow(&mut h, 9.5, 1.0, 0.5, 7);
    h.tick();
    let e = launch(&mut h.world, Flight::POTION, Vec3::new(0.5, 1.2, 0.5), Vec3::new(0.0, -0.1, 0.0), None);
    h.world.entity_mut(e).insert(horizon_ecs_proto::effects::PotionId("minecraft:poison"));
    for _ in 0..60 {
        h.tick();
        if h.world.get_entity(e).is_err() {
            break;
        }
    }
    assert!(h.world.get_entity(e).is_err(), "the flask is gone where it broke");
    assert!(h.world.get::<horizon_ecs_proto::effects::Effects>(near).is_some_and(|f| !f.list.is_empty()), "poison near");
    assert!(h.world.get::<horizon_ecs_proto::effects::Effects>(far).is_none_or(|f| f.list.is_empty()), "nothing far");
}

/// A lingering flask breaks into one area cloud of its potion where it lands, with the original's radius and duration.
#[test]
fn a_lingering_potion_leaves_one_cloud_where_it_lands() {
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let e = launch(&mut h.world, Flight::LINGERING_POTION, Vec3::new(0.5, 1.2, 0.5), Vec3::new(0.0, -0.1, 0.0), None);
    h.world.entity_mut(e).insert(horizon_ecs_proto::effects::PotionId("minecraft:poison"));
    for _ in 0..60 {
        h.tick();
        if h.world.get_entity(e).is_err() {
            break;
        }
    }
    let clouds: Vec<horizon_ecs_proto::effect_cloud::EffectCloud> = h.world.query::<&horizon_ecs_proto::effect_cloud::EffectCloud>().iter(&h.world).copied().collect();
    assert_eq!(clouds.len(), 1, "one cloud");
    assert!(clouds[0].radius == 3.0 && clouds[0].duration == 600 && clouds[0].wait == 10, "{:?}", clouds[0]);
}

/// A blue skull blows through the obsidian shell that a normal one stops at: the wither caps its resistance at 0.8.
#[test]
fn a_blue_skull_breaks_obsidian_that_a_normal_one_does_not() {
    if !have_data() {
        return;
    }
    let mut counts = Vec::new();
    for blue in [false, true] {
        let (mut r, mut h) = flat_pair();
        for x in -4..=4 {
            for y in 0..=8 {
                for z in -4..=4 {
                    let norm = x.abs().max((y - 4).abs()).max(z.abs());
                    r.set_block(x, y, z, if norm == 2 { "minecraft:obsidian" } else { "minecraft:dirt" });
                }
            }
        }
        r.apply_block_events();
        h.tick();
        let blast = horizon_ecs_proto::projectile::Blast { at: Vec3::new(0.5, 4.5, 0.5), radius: 3.0, fire: false, owner: None, source: None };
        horizon_ecs_proto::explosion::explode_wither(&mut h.world, blast, blue);
        let shell = h.world.resource::<BlockChanges>().0.iter().filter(|p| p.0.abs().max((p.1 - 4).abs()).max(p.2.abs()) == 2).count();
        counts.push(shell);
    }
    assert_eq!(counts[0], 0, "a normal skull stops at the obsidian");
    assert!(counts[1] > 0, "a blue skull breaks obsidian");
}

/// A blue skull never breaks a block the wither cannot destroy (bedrock), though it breaks the dirt around it.
#[test]
fn a_blue_skull_leaves_a_block_the_wither_cannot_break() {
    if !have_data() {
        return;
    }
    let (mut r, mut h) = flat_pair();
    for x in -4..=4 {
        for y in 0..=8 {
            for z in -4..=4 {
                let norm = x.abs().max((y - 4).abs()).max(z.abs());
                r.set_block(x, y, z, if norm == 2 { "minecraft:bedrock" } else { "minecraft:dirt" });
            }
        }
    }
    r.apply_block_events();
    h.tick();
    let blast = horizon_ecs_proto::projectile::Blast { at: Vec3::new(0.5, 4.5, 0.5), radius: 3.0, fire: false, owner: None, source: None };
    horizon_ecs_proto::explosion::explode_wither(&mut h.world, blast, true);
    let broken = h.world.resource::<BlockChanges>().0.clone();
    assert!(!broken.is_empty(), "the dirt breaks");
    assert!(broken.iter().all(|p| p.0.abs().max((p.1 - 4).abs()).max(p.2.abs()) != 2), "bedrock is not broken: {broken:?}");
}

/// A second skull inside the hurt window, for the same magic 5, does nothing: no health lost and no wither again.
#[test]
fn a_skull_that_does_not_hurt_does_not_wither() {
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let cow = ecs_cow(&mut h, 0.5, 1.0, 2.5, 5);
    h.tick();
    for _ in 0..2 {
        let e = launch(&mut h.world, Flight::WITHER_SKULL, Vec3::new(0.5, 1.5, 0.5), Vec3::new(0.0, 0.0, 0.6), None);
        for _ in 0..40 {
            h.tick();
            if h.world.get_entity(e).is_err() {
                break;
            }
        }
        if h.world.get::<horizon_ecs_proto::effects::Effects>(cow).is_some_and(|f| !f.list.is_empty()) {
            h.world.entity_mut(cow).remove::<horizon_ecs_proto::effects::Effects>();
            let before = health(&h, cow);
            let e = launch(&mut h.world, Flight::WITHER_SKULL, Vec3::new(0.5, 1.5, 0.5), Vec3::new(0.0, 0.0, 0.6), None);
            for _ in 0..40 {
                h.tick();
                if h.world.get_entity(e).is_err() {
                    break;
                }
            }
            assert_eq!(before - health(&h, cow), 0.0, "the second skull does not hurt in the window");
            assert!(h.world.get::<horizon_ecs_proto::effects::Effects>(cow).is_none(), "no wither on a blow that does not hurt");
            return;
        }
    }
    panic!("the first skull withers the creature");
}
