//! A shulker (`HORIZON_SPECIES=minecraft:shulker`) under a stone ceiling with a player 8 blocks away, against the `Mob` shulker in the same
//! scene. Compared: the lid's opening tick by tick, the first shot and the cadence of the shots, and the teleports after hits once the
//! shulker is below half health. The Mob's lid is read through `peek_height_scale`, its shots through the shoot sound.
use crate::common;
use common::*;
use horizon_client_lib::mobs::{MobEvent, shulker::peek_height_scale};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::targeting::{set_target, Target};
use horizon_ecs_proto::{behaviours::attached_face::Attached, cow::*, goals::*, projectile::*};

fn skip() -> bool {
    !have_data() || process_species_name() != "minecraft:shulker"
}

/// The Mob and the ECS shulker, the same spot, the player 12 blocks away on the same side (out of the range of the random look, which would interrupt the volley on one side only).
fn scene() -> (MobRef, Herd, usize, Entity) {
    let at = Vec3::new(0.5, 1.0, 0.5);
    let mut r = MobRef::new(40, false);
    let mut h = Herd::with_slab(40, true, false);
    let mob = r.add(at);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let ecs = spawn_species(&mut h.world, &sp, at.x, at.z, 77);
    h.world.get_mut::<ActiveUntil>(ecs).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 12.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    h.world.resource_mut::<horizon_ecs_proto::hostile::PlayerState>().immortal = true;
    r.mobs[mob].ai.target = Some(horizon_client_lib::mobs::Who::Player);
    set_target(&mut h.world, ecs, Target::player(1, 30));
    (r, h, mob, ecs)
}

/// Opening and shots: over 200 ticks the two lids agree tick by tick, the first shot comes at the same tick (within 2), and the shots keep
/// the same gap.
#[test]
fn the_shulker_opens_and_fires_as_a_mob_does() {
    if skip() {
        return;
    }
    let (mut r, mut h, mob, ecs) = scene();
    let view = player_view(Vec3::new(12.5, 1.0, 0.5), None);
    let (mut mob_shots, mut ecs_shots, mut seen) = (Vec::new(), Vec::new(), std::collections::HashSet::new());
    // Both answer the target on the same tick (measured: the lids are equal tick by tick).
    let mut worst_lid = 0.0_f32;
    for t in 1..=200i64 {
        let from = r.events.len();
        r.step(Some(&view));
        h.tick();
        let mob_lid = peek_height_scale(&r.mobs[mob]).map_or(0.0, |s| s - 1.0);
        let ecs_lid = h.world.get::<Attached>(ecs).unwrap().eased;
        worst_lid = worst_lid.max((mob_lid - ecs_lid).abs());
        if r.events[from..].iter().any(|e| matches!(e, MobEvent::Sound(n, ..) if *n == "entity.shulker.shoot")) {
            mob_shots.push(t);
        }
        let mut q = h.world.query::<(Entity, &Projectile)>();
        let new: Vec<Entity> = q.iter(&h.world).map(|x| x.0).filter(|e| !seen.contains(e)).collect();
        seen.extend(new.iter().copied());
        if !new.is_empty() {
            ecs_shots.push(t);
        }
    }
    eprintln!("shots: mob {mob_shots:?}, ecs {ecs_shots:?}; worst lid gap {worst_lid:.4}");
    assert!(worst_lid < 0.02, "lid gap {worst_lid}");
    assert!(mob_shots.len() >= 3 && ecs_shots.len() >= 3, "mob {mob_shots:?}, ecs {ecs_shots:?}");
    assert!((mob_shots[0] - ecs_shots[0]).abs() <= 2, "first shot: mob {}, ecs {}", mob_shots[0], ecs_shots[0]);
    let gaps = |v: &[i64]| v.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>();
    // The wait is `20 + 10 x nextInt(10)` on both sides, from their own random source: the gaps are drawn, not equal.
    for g in gaps(&mob_shots).into_iter().chain(gaps(&ecs_shots)) {
        assert!((30..=110).contains(&g) && g % 10 == 0, "a gap of {g}: not 20 + 10 x n");
    }
}

/// Teleports after hits: once below half health, each hit has one chance in four to move the shulker, on both sides. Hits every 11 ticks
/// (the invulnerability), 0.5 each, the health kept at 14 of 30, 130 hits. Both must move at least once.
#[test]
fn the_shulker_teleports_after_hits_below_half_health_as_a_mob_does() {
    if skip() {
        return;
    }
    let (mut r, mut h, mob, ecs) = scene();
    let view = player_view(Vec3::new(12.5, 1.0, 0.5), None);
    let (mut mob_moves, mut ecs_moves) = (0, 0);
    for k in 0..130 {
        for _ in 0..10 {
            r.step(Some(&view));
            h.tick();
        }
        // Kept below half health (a hit does 0.5): the chance is then 1 in 4 per hit, and a spot is found in 5 tries only now and then.
        r.mobs[mob].health = 14.0;
        h.world.get_mut::<Health>(ecs).unwrap().0 = 14.0;
        let (mp, ep) = (r.mobs[mob].position, h.world.get::<Pos>(ecs).unwrap().0);
        r.hit(mob, 0.5, [12.5, 1.0, 0.5]);
        h.hit(ecs, 0.5, (12.5, 0.5));
        // The Mob settles a teleport the hurt left on its next step.
        r.step(Some(&view));
        h.tick();
        let (mp2, ep2) = (r.mobs[mob].position, h.world.get::<Pos>(ecs).unwrap().0);
        let moved_m = xz(mp, mp2) > 1.0;
        let moved_e = xz(ep, ep2) > 1.0;
        {
            mob_moves += usize::from(moved_m);
            ecs_moves += usize::from(moved_e);
        }
    }
    eprintln!("teleports after half health: mob {mob_moves}, ecs {ecs_moves}");
    assert!(mob_moves > 0 && ecs_moves > 0, "mob {mob_moves}, ecs {ecs_moves}");
}

struct Open<'a>(&'a MobRef, Vec<(horizon_client_lib::mobs::Who, horizon_core::geometry::Aabb)>);
impl horizon_client_lib::projectiles::ProjectileWorld for Open<'_> {
    fn clip(&self, from: Vec3, to: Vec3) -> Option<(Vec3, [i32; 3], [i32; 3])> {
        let mw = self.0.w;
        let movement = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z);
        horizon_client_lib::arrows::clip_block_hit(mw.world, mw.facts, mw.collision, from, movement).map(|(at, p, n)| (at, [p.x, p.y, p.z], n))
    }
    fn in_water(&self, _: horizon_core::geometry::Aabb) -> bool {
        false
    }
    fn targets(&self) -> &[(horizon_client_lib::mobs::Who, horizon_core::geometry::Aabb)] {
        &self.1
    }
}

/// The first bullet, tick by tick: the ECS bullet stays on the Mob's bullet's path toward the player's box (under 0.05 apart), and ends
/// within a tick of it.
#[test]
#[ignore = "the bullet redraws its direction and its step count from its own unseeded Entity.random (jar: ShulkerBullet uses this.random, seeded by RandomSource.create(), never the level's); the Mob's bullet and the ECS's draw different seeds, so the paths split at the first redraw (identical for the 19 ticks before it, measured). Not comparable without a shared seed."]
fn the_bullet_homes_on_the_player_tick_by_tick_as_the_mob_s_does() {
    if skip() {
        return;
    }
    let (mut r, mut h, _, _) = scene();
    let view = player_view(Vec3::new(12.5, 1.0, 0.5), None);
    let player = Player { id: 1, x: 12.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() };
    let boxes = vec![(horizon_client_lib::mobs::Who::Player, horizon_ecs_proto::targeting::player_box(&player))];
    let (mut mob, mut ecs) = (None, None);
    for _ in 0..200 {
        let from = r.events.len();
        r.step(Some(&view));
        h.tick();
        // Read in place: `apply_block_events` counts the events it has applied, and a drain would take some of them away.
        mob = r.events[from..].iter().find_map(|e| match e {
            MobEvent::Projectile(p) => Some(p.clone()),
            _ => None,
        });
        ecs = h.world.query::<(Entity, &horizon_ecs_proto::projectile::Projectile)>().iter(&h.world).map(|x| x.0).next();
        if mob.is_some() {
            break;
        }
    }
    let mut mob = mob.expect("the Mob fires a bullet");
    let ecs = ecs.expect("the ECS fires a bullet");
    let (mut mob_end, mut ecs_end, mut worst) = (None, None, 0.0_f64);
    for t in 1..=60i64 {
        r.step(Some(&view));
        h.tick();
        let (keep, _) = mob.tick(&Open(&r, boxes.clone()));
        if !keep && mob_end.is_none() {
            mob_end = Some(t);
        }
        match h.world.get::<horizon_ecs_proto::cow::Pos>(ecs).filter(|_| h.world.get::<horizon_ecs_proto::projectile::Projectile>(ecs).is_some()) {
            Some(p) if ecs_end.is_none() => worst = worst.max((p.0.x - mob.position.x).abs().max((p.0.y - mob.position.y).abs()).max((p.0.z - mob.position.z).abs())),
            Some(_) => {}
            None => {
                ecs_end.get_or_insert(t);
            }
        }
    }
    eprintln!("bullet: worst gap {worst:.4}, ends mob {mob_end:?} ecs {ecs_end:?}");
    assert!(worst < 0.05, "the path drifts by {worst}");
    assert!(mob_end.is_some() && ecs_end.is_some(), "both end: mob {mob_end:?}, ecs {ecs_end:?}");
    assert!((mob_end.unwrap() - ecs_end.unwrap()).abs() <= 1, "the end: mob {mob_end:?}, ecs {ecs_end:?}");
}
