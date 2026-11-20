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
