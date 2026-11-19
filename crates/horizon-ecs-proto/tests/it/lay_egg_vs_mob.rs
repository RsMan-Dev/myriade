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
    behaviours::lay_egg::{self, Carrying, Lay},
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
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let e = spawn_species(&mut h.world, &sp, at.x, at.z, 92);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    lay_egg::load(&mut h.world, e);
    (r, h, i, e)
}

#[test]
fn a_turtle_with_an_egg_walks_digs_and_lays_it_as_a_mob_does() {
    if !have_data() || species().name != "minecraft:turtle" {
        return;
    }
    let at = Vec3::new(2.5, 1.0, 0.5);
    let (mut r, mut h, i, e) = scene(at);
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
    // The digging: 200 ticks on both sides (`counter > 200`, `dug > 200`), to within a tick.
    let (m_span, e_span) = (m_t - m_dig, e_t - e_dig);
    assert!((195..=205).contains(&m_span) && (195..=205).contains(&e_span), "dig: mob {m_span}, ecs {e_span}");
    assert!((m_span - e_span).abs() <= 2, "dig: mob {m_span}, ecs {e_span}");
    // The egg block: one block above a sand block, within 9 blocks of the anchor on the ground.
    for pos in [m_pos, e_pos] {
        assert_eq!(pos[1], 1, "the egg is one block above the sand: {pos:?}");
        assert!((f64::from(pos[0]) - at.x).abs() <= 9.0 && (f64::from(pos[2]) - at.z).abs() <= 9.0, "{pos:?} is out of the radius");
    }
    assert!(!r.mobs[i].extra.flag, "the Mob's egg is gone after the laying");
    assert!(h.world.get::<Carrying>(e).is_none(), "the ECS load is dropped after the laying");
}
