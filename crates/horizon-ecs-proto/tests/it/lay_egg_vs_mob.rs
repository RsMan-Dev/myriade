//! The egg laying of a turtle against a real `Mob` turtle in the same scene (`HORIZON_TEST_RESOURCES`, process species
//! `minecraft:turtle`, whose goals carry `lay_egg` on the ECS side): the walk to the spot, the digging, the tick and the
//! position of the egg set, and the load dropped after it. The Mob's digging starts at its `counter` (set to 1 when it digs),
//! the ECS's at `Lay.dug`.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::MobEvent;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::{lay_egg::{self, Carrying, Lay}, turtle_go_home},
    cow::*,
    goals::*,
    host_out::HostEvent,
};

/// The turtle of the run on a floor of sand, with the egg (the Mob's `flag`, the ECS's `Carrying`) and its anchor where it is
/// born on both sides (the spawn point is the home).
fn scene(at: Vec3) -> (MobRef, Herd, usize, Entity) {
    let (mut r, mut h) = flat_pair();
    for x in -10..=10 {
        for z in -10..=10 {
            r.set_block(x, 0, z, "minecraft:sand");
            h.set_block(x, 0, z, "minecraft:sand");
        }
    }
    h.tick();
    let i = r.add(at);
    r.mobs[i].extra.flag = true;
    // Spawning, hatching and loading always set a home (`homePos`); a Mob without one is at home wherever it stands, which sends its
    // go-home goal wandering and keeps it from ever staying on the spot.
    r.mobs[i].extra.home = Some([at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32]);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let e = spawn_species(&mut h.world, &sp, at.x, at.z, 92);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    lay_egg::load(&mut h.world, e);
    (r, h, i, e)
}

#[test]
fn a_turtle_with_an_egg_walks_digs_and_lays_it_as_a_mob_does() {
    if !have_data() || process_species_name() != "minecraft:turtle" {
        return;
    }
    let at = Vec3::new(2.5, 1.0, 0.5);
    let (mut r, mut h, i, e) = scene(at);
    // The go-home goal draws its spots from the random stream of each side: the Mob's draws are imposed on the ECS (the spot, as
    // the block the Mob's navigation heads for), so the interference of that goal is compared at the tick.
    h.world.insert_resource(turtle_go_home::ImposedSpots::default());
    let (mut last_target, lay_block) = (None, [2, 0, 0]);
    let (mut m_dig, mut m_laid): (Option<i64>, Option<(i64, [i32; 3])>) = (None, None);
    let (mut e_dig, mut e_laid): (Option<i64>, Option<(i64, [i32; 3])>) = (None, None);
    for t in 1..=900i64 {
        let from = r.events.len();
        r.step(None);
        if m_dig.is_none() && r.mobs[i].extra.counter >= 1 {
            m_dig = Some(t);
        }
        for ev in &r.events[from..] {
            if let MobEvent::SetBlock { pos, .. } = ev {
                m_laid.get_or_insert((t, *pos));
            }
        }
        let target = r.mobs[i].ai.nav.target_pos;
        if target != last_target && target.is_some_and(|b| b != lay_block) {
            let b = target.unwrap();
            h.world.resource_mut::<turtle_go_home::ImposedSpots>().0.push_back([f64::from(b[0]) + 0.5, f64::from(b[1]), f64::from(b[2]) + 0.5]);
        }
        last_target = target;
        h.tick();
        if e_dig.is_none() && h.world.get::<Lay>(e).is_some_and(|l| l.dug >= 1) {
            e_dig = Some(t);
        }
        for ev in h.drain_host_out() {
            if let HostEvent::SetBlock { pos, .. } = ev {
                e_laid.get_or_insert((t, pos));
            }
        }
        if m_laid.is_some() && e_laid.is_some() {
            break;
        }
    }
    let ((m_t, m_pos), (e_t, e_pos)) = (m_laid.expect("the Mob never laid its egg"), e_laid.expect("the ECS never laid its egg"));
    let (m_dig, e_dig) = (m_dig.expect("the Mob never dug"), e_dig.expect("the ECS never dug"));
    eprintln!("dig: mob from {m_dig}, ecs from {e_dig}; laid: mob {m_t} at {m_pos:?}, ecs {e_t} at {e_pos:?}");
    // The walk to the spot: the same tick to within the path's own variations.
    assert!((m_dig - e_dig).abs() <= 10, "walk: mob {m_dig}, ecs {e_dig}");
    // The Mob's go-home goal moves the turtle away from the spot while it has the egg, and the counter pauses: the same laying tick.
    assert!((m_t - e_t).abs() <= 2, "laying tick: mob {m_t}, ecs {e_t}");
    // The digging is 200 counted ticks on both sides (`counter > 200`, `dug > 200`); with the egg and the home within 7 blocks the
    // go-home goal leads the turtle away from the spot and stops the count (oracle corrected: the old 195..=205 span assumed no
    // interference and the Mob itself takes 277), so what is compared is the tick of the laying, to within the 2 ticks of the path.
    // The egg block: one block above a sand block, within 9 blocks of the anchor on the ground.
    for pos in [m_pos, e_pos] {
        assert_eq!(pos[1], 1, "the egg is one block above the sand: {pos:?}");
        assert!((f64::from(pos[0]) - at.x).abs() <= 9.0 && (f64::from(pos[2]) - at.z).abs() <= 9.0, "{pos:?} is out of the radius");
    }
    assert!(!r.mobs[i].extra.flag, "the Mob's egg is gone after the laying");
    assert!(h.world.get::<Carrying>(e).is_none(), "the ECS load is dropped after the laying");
}
