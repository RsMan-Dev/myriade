//! Règle du frottement de sol, reprise de `ice_friction_tests.rs` (valeurs inchangées) : un animal au sol glisse selon le frottement
//! de son bloc de support (sous la boîte), pas selon celui de la colonne sous son centre. Farmland (0.6) sous le centre, glace bleue
//! (0.989) sous le reste de la boîte : 0.5 x 0.989 x 0.91 = 0.449995.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;

#[test]
fn a_creature_slides_on_its_supporting_blocks_friction_not_on_its_centres() {
    if !have_data() {
        return;
    }
    let mut h = Herd::with_slab_awake(60, false, false);
    h.set_block(0, 0, 0, "minecraft:farmland");
    h.set_block(1, 0, 0, "minecraft:blue_ice");
    let e = ecs_cow(&mut h, 0.9, 1.0, 0.5, 1);
    h.tick();
    assert!(h.world.get::<Contact>(e).unwrap().on_ground, "the cow stands on the ice");
    h.world.get_mut::<Vel>(e).unwrap().0 = Vec3::new(0.5, 0.0, 0.0);
    h.tick();
    let vx = h.world.get::<Vel>(e).unwrap().0.x;
    assert!((vx - 0.449_995).abs() < 1.0e-3, "the ice's 0.989, not the farmland's 0.6: {vx}");
}
