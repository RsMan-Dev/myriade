//! The warden against the real `Mob` in the same scene (`HORIZON_TEST_RESOURCES` set, process species `minecraft:warden`): the emerge,
//! the anger after three blows, the tick of the sonic boom, and the burrow after the calm. The blows stand for the vibrations: the
//! `Mob` has no public way to inject one, so the anger path is the one of `handle_hurt` on both sides (see the notes).
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::{warden, MobEvent};
use horizon_ecs_proto::{cow::*, behaviours::{act_phase::{ActPhase, Phase}, dig::Withdrawn, suspect_anger}, goals::*, targeting::*};

fn skip() -> bool {
    !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:warden")
}

/// The first tick of each probe, on both sides: `[emerging, angry, booming, gone]`.
#[derive(Default, Clone, Copy)]
struct Marks {
    t: [Option<i64>; 4],
}

impl Marks {
    fn note(&mut self, t: i64, probes: [bool; 4]) {
        for (k, on) in probes.into_iter().enumerate() {
            if on && self.t[k].is_none() {
                self.t[k] = Some(t);
            }
        }
    }
}

fn close(a: Option<i64>, b: Option<i64>, tol: i64) -> bool {
    matches!((a, b), (Some(a), Some(b)) if (a - b).abs() <= tol)
}

/// One warden on each side, a player 6 blocks away, both sides' ticks in step.
fn scene() -> (MobRef, Herd, Entity, usize) {
    let mut r = MobRef::new(60, false);
    let mut h = Herd::with_slab(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let i = r.add_id(horizon_core::geometry::Vec3::new(0.5, 1.0, 0.5), 7);
    let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    h.world.resource_mut::<Online>().set(Player { id: 1, x: 6.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
    (r, h, e, i)
}

/// Emergence, anger and burrow, then the sonic boom, tick by tick on both sides.
#[test]
fn the_warden_emerges_gets_angry_booms_and_burrows_as_a_mob_does() {
    if skip() {
        return;
    }
    let (mut r, mut h, e, i) = scene();
    let player = player_view(horizon_core::geometry::Vec3::new(6.5, 1.0, 0.5), None);
    let mut mob = Marks::default();
    let mut ecs = Marks::default();
    let mut boom_mob: Option<i64> = None;
    let mut seen_events = 0usize;
    for t in 1..=1500i64 {
        if (140..=142).contains(&t) {
            r.hit(i, 1.0, [6.5, 1.0, 0.5]);
            h.hit(e, 1.0, (6.5, 0.5));
        }
        r.step(Some(&player));
        h.tick();
        let m = &r.mobs[i];
        mob.note(t, [warden::pose(m) == warden::Pose::Emerging, warden::anger_level(m).is_angry(), false, m.removed]);
        let st = *h.world.get::<ActPhase>(e).unwrap();
        ecs.note(t, [st.phase == Phase::Emerging, suspect_anger::level(&h.world, e) == suspect_anger::Level::Angry, st.phase == Phase::Booming, h.world.get::<Withdrawn>(e).is_some()]);
        for ev in &r.events[seen_events..] {
            if let MobEvent::EntityEvent { event: 62, .. } = ev {
                boom_mob.get_or_insert(t);
            }
        }
        seen_events = r.events.len();
    }
    eprintln!("emerging: mob {:?} ecs {:?}", mob.t[0], ecs.t[0]);
    eprintln!("angry: mob {:?} ecs {:?}", mob.t[1], ecs.t[1]);
    eprintln!("boom start: mob {:?} ecs {:?}; gone: mob {:?} ecs {:?}", boom_mob, ecs.t[2], mob.t[3], ecs.t[3]);
    assert!(close(mob.t[0], ecs.t[0], 2), "emerging: mob {:?}, ecs {:?}", mob.t[0], ecs.t[0]);
    assert!(close(mob.t[1], ecs.t[1], 2), "angry after the blows: mob {:?}, ecs {:?}", mob.t[1], ecs.t[1]);
    assert!(close(boom_mob, ecs.t[2], 2), "sonic boom start: mob {:?}, ecs {:?}", boom_mob, ecs.t[2]);
    assert!(close(mob.t[3], ecs.t[3], 10), "burrow after the calm: mob {:?}, ecs {:?}", mob.t[3], ecs.t[3]);
}
