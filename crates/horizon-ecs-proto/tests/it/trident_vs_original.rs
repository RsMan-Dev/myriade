//! A thrown trident against the `Mob`'s trident projectile (`Projectile::tick`, `ProjectileKind::Trident`) over the flat ground: the
//! same position on every tick of the flight, and stuck in the same tick. The blow, the Loyalty return and the pickup have no oracle
//! in `Mob` (the mob's trident does not return): they are checked by the notes, not here.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::projectiles::{Projectile, ProjectileKind, ProjectileWorld};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{cow::*, projectile::*, trident::*};

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

/// Thrown over the flat ground at a few angles: the same position on every tick until the Mob's trident sticks, and stuck on that
/// same tick (the ECS trident is planted on it, and its position is the Mob's).
#[test]
fn thrown_trident_flies_and_sticks_as_the_mob_s() {
    if !have_data() {
        return;
    }
    let throws = [Vec3::new(0.5, 0.25, -0.1), Vec3::new(0.0, -0.4, 0.6), Vec3::new(0.3, 1.2, 0.0)];
    for (k, v) in throws.into_iter().enumerate() {
        let (r, mut h) = flat_pair();
        let at = Vec3::new(0.5, 1.9, 0.5);
        let mut m = Projectile::new(ProjectileKind::Trident, at, None);
        m.velocity = v;
        let e = throw(&mut h.world, ItemStack::default(), at, v, None, 0, Pickup::Allowed);
        let world = Open(&r, Vec::new());
        let mut stuck = None;
        for t in 1..=200i64 {
            let (keep, _) = m.tick(&world);
            h.tick();
            let p = h.world.get::<Pos>(e).unwrap().0;
            assert!(p.x == m.position.x && p.y == m.position.y && p.z == m.position.z, "throw {k} t={t}: pos {p:?} / mob {:?}", m.position);
            if !keep {
                let planted = h.world.get::<Planted>(e).is_some();
                assert!(planted, "throw {k}: the mob's trident stuck at tick {t}, the ECS one did not");
                stuck = Some(t);
                break;
            }
        }
        eprintln!("throw {k}: stuck at {stuck:?}");
        assert!(stuck.is_some(), "throw {k} never stuck");
    }
}
