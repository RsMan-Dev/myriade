//! The goat's ram (`PrepareRamNearestTarget` then `RamTarget`) against the `Mob`: a goat with no cooldown and a cow 9 blocks east on flat
//! ground. The ECS goat has no horns and no hold on the target's anger (`ram.rs` header); the timeline of the charge is compared.
use crate::common::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::mobs::MobEvent;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{cow::*, goals::*};

struct Run {
    /// First tick the goat moves faster than 0.4 blocks a tick (the charge, at 3.0 times its speed), first tick the cow loses health.
    charge: Option<i64>,
    hurt: Option<i64>,
    goat_x: f64,
}

fn mob_run() -> Run {
    let mut rf = MobRef::new(60, false);
    let g = rf.add_kind(Vec3::new(0.5, 1.0, 0.5), mob_kind("minecraft:goat"));
    let c = rf.add_kind(Vec3::new(9.5, 1.0, 0.5), mob_kind("minecraft:cow"));
    let p = player_view(Vec3::new(40.0, 1.0, 40.0), None);
    let mut out = Run { charge: None, hurt: None, goat_x: 0.0 };
    let mut last = rf.mobs[g].position.x;
    for t in 1..=7000 {
        let from = rf.events.len();
        rf.step(Some(&p));
        let x = rf.mobs[g].position.x;
        if (x - last).abs() > 0.4 {
            out.charge.get_or_insert(t);
        }
        last = x;
        if rf.events[from..].iter().any(|e| matches!(e, MobEvent::AttackMob { .. })) || rf.mobs[c].health < rf.mobs[c].attrs.max_health {
            out.hurt.get_or_insert(t);
        }
    }
    out.goat_x = last;
    out
}

fn ecs_run() -> Run {
    let mut h = Herd::with_slab(60, false, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = |n: &str| table.iter().position(|s| s.name == n).unwrap();
    let g = spawn_species(&mut h.world, &table[sp("minecraft:goat")], 0.5, 0.5, 1000);
    let c = spawn_species(&mut h.world, &table[sp("minecraft:cow")], 9.5, 0.5, 1001);
    for e in [g, c] {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(e);
    }
    let full = h.world.get::<Health>(c).unwrap().0;
    let mut out = Run { charge: None, hurt: None, goat_x: 0.0 };
    let mut last = 0.5;
    for t in 1..=7000 {
        h.tick();
        let x = h.world.get::<Pos>(g).map_or(last, |p| p.0.x);
        if (x - last).abs() > 0.4 {
            out.charge.get_or_insert(t);
        }
        last = x;
        if h.world.get::<Health>(c).is_some_and(|x| x.0 < full) {
            out.hurt.get_or_insert(t);
        }
    }
    out.goat_x = last;
    out
}

#[test]
fn a_goat_rams_a_cow_as_a_mob_does() {
    if !have_data() {
        return;
    }
    let (m, e) = (mob_run(), ecs_run());
    eprintln!("ram: mob charge {:?} hurt {:?} x {:.1}; ecs charge {:?} hurt {:?} x {:.1}", m.charge, m.hurt, m.goat_x, e.charge, e.hurt, e.goat_x);
    let (mc, mh, ec, eh) = (m.charge.expect("the Mob goat charges"), m.hurt.expect("the Mob cow is hurt"), e.charge.expect("the ECS goat charges"), e.hurt.expect("the ECS cow is hurt"));
    // The wait before the first charge differs by design: the jar's `GoatAi.initMemories` draws 600..6000 at creation (the ECS), the `Mob` tries at once,
    // fails (no start spot yet) and waits the 600 minimum. What is compared is the charge itself: the cow is hurt a few ticks after it starts.
    assert!(mc >= 600 && ec >= 600, "no charge before the 600 ticks minimum (mob {mc}, ecs {ec})");
    assert!((mh - mc).abs_diff(eh - ec) <= 3, "ticks from the start of the charge to the hit: mob {}, ecs {}", mh - mc, eh - ec);
}
