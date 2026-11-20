//! The free flight of a ghast (`HORIZON_SPECIES=minecraft:ghast`) against a real `Mob` ghast: `GhastMoveControl` and `travelFlying`, with the
//! spot and the push counter the `Mob` has imposed on the ecs ghast each tick (the draws of the two generators are not the same). One
//! ghast per level: the push between neighbours is not what is measured here.
use crate::common;
use common::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*, locomotion::fly::FloatState};

#[test]
fn a_ghast_floats_as_a_mob_does_when_its_spot_is_imposed() {
    if !have_data() {
        return;
    }
    let _ = flat_pair();
    if process_species_name() != "minecraft:ghast" {
        return;
    }
    let spots = [(-12.5, 8.5, -8.5), (3.5, 14.5, 7.5), (-4.5, 10.5, -3.5), (8.5, 12.5, 5.5), (0.5, 9.5, -7.5), (-10.5, 12.5, 4.5), (6.5, 20.5, -4.5), (-2.5, 6.5, -9.5)];
    let mut worst = [0.0_f64; 3];
    let mut moved = 0;
    for (k, &(x0, y0, z0)) in spots.iter().enumerate() {
        let (mut r, mut h) = flat_pair();
        let (i, e) = (r.add(Vec3::new(x0, y0, z0)), ecs_cow(&mut h, x0, y0, z0, 900 + k as u64));
        // The ecs goal would draw its own spots: here the `Mob` draws them.
        h.world.entity_mut(e).remove::<GoalSet>();
        for t in 1..=300_usize {
            let before = r.mobs[i].extra.counter2;
            r.step(None);
            let m = &r.mobs[i];
            let on = m.ai.mov.operation == horizon_client_lib::mobs::control::Operation::MoveTo;
            let [x, y, z] = m.ai.mov.wanted;
            *h.world.get_mut::<MoveTo>(e).unwrap() = MoveTo { active: on, x, y, z, speed_modifier: 1.0 };
            h.world.get_mut::<FloatState>(e).unwrap().0 = before;
            h.tick();
            let (a, b) = (r.mobs[i].position, h.world.get::<Pos>(e).unwrap().0);
            let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt();
            if d > 0.01 && std::env::var_os("TRACE2").is_some() {
                eprintln!("ghast {k} t={t} gap {d:.3} MOB ({:.3},{:.3},{:.3}) op {:?} | ECS ({:.3},{:.3},{:.3})", a.x, a.y, a.z, r.mobs[i].ai.mov.operation, b.x, b.y, b.z);
            }
            for (n, lim) in [20, 100, 300].into_iter().enumerate() {
                if t <= lim && d > worst[n] {
                    worst[n] = d;
                }
            }
            if t == 300 && (a.x - x0).abs() + (a.y - y0).abs() + (a.z - z0).abs() > 1.0 {
                moved += 1;
            }
        }
    }
    eprintln!("ghast float: worst gap to +20 {:.4}, +100 {:.4}, +300 {:.4}; {moved}/{} moved", worst[0], worst[1], worst[2], spots.len());
    assert!(moved >= 5, "the ghasts hardly moved");
    assert!(worst[0] < 0.1 && worst[2] < 0.5, "{worst:?}");
}
