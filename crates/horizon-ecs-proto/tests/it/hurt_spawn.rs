//! Infested (`InfestedMobEffect.onMobHurt`): a bearer that is hurt lets 1 or 2 silverfish out with a chance of 0.1, and a bearer
//! without the effect lets none out.
use bevy_ecs::prelude::*;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::effects::{self, Kind};
use horizon_ecs_proto::hurt_spawn;

fn silverfish(h: &mut Herd) -> usize {
    h.world.query::<&TypeName>().iter(&h.world).filter(|t| t.0 == "minecraft:silverfish").count()
}

fn bearer(h: &mut Herd) -> Entity {
    let sp = species_table().0.into_iter().find(|s| s.name == "minecraft:zombie").expect("zombie species");
    spawn_species(&mut h.world, &sp, 0.5, 0.5, 5)
}

#[test]
fn infested_bearer_lets_silverfish_out_and_others_do_not() {
    if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, true, false);
    let plain = bearer(&mut h);
    for _ in 0..300 {
        hurt_spawn::on_hurt(&mut h.world, plain);
    }
    assert_eq!(silverfish(&mut h), 0, "no effect, no silverfish");
    let e = bearer(&mut h);
    effects::add(&mut h.world, e, Kind::Infested, 0, 6000);
    for _ in 0..300 {
        hurt_spawn::on_hurt(&mut h.world, e);
    }
    let n = silverfish(&mut h);
    // 300 rolls at 0.1 give 30 successes of 1.5 silverfish on average: far from 0 and from 2 per roll.
    assert!((10..=120).contains(&n), "silverfish out: {n}");
}

#[test]
fn thrown_velocity_follows_the_look_scaled_and_turned() {
    use horizon_core::geometry::Vec3;
    let v = hurt_spawn::thrown(Vec3::new(0.0, 0.0, 1.0), 0.0);
    assert!((v.z - 0.3).abs() < 1e-6 && v.x.abs() < 1e-6 && v.y.abs() < 1e-6);
    let up = hurt_spawn::thrown(Vec3::new(0.0, 1.0, 0.0), 1.0);
    assert!((up.y - 0.45).abs() < 1e-6, "the y component is scaled by 0.3 x 1.5");
}
