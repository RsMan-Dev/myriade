//! The dragon's attacks against Horizon's, tick by tick: the strafe (the fireball in flight and its cloud), the charge, and the whole
//! perch (landing, scanning, the attack, the flames and their cloud, the takeoff). Golden: `dragon_golden/combat.txt`; each line is
//! the position, yaw, phase and health of the dragon, the fireballs, and the clouds (position, radius, age, victims). The height of
//! a fireball's cloud is not compared (`ecs-ecarts.md`: Horizon places it half a block lower).
use crate::dragon_support::*;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::dragon::breath::{Cloud, Flame};
use horizon_ecs_proto::dragon::phases::{self, PhaseState};
use horizon_ecs_proto::projectile::Projectile;

thread_local! {
    static BORN: std::cell::RefCell<std::collections::HashMap<Entity, usize>> = Default::default();
}

fn frame(h: &mut Herd, e: Entity, name: &str, t: usize) -> String {
    let w = &mut h.world;
    let mut fb: Vec<String> = w.query_filtered::<&Pos, With<Projectile>>().iter(w).map(|p| bits(p.0)).collect();
    fb.sort();
    let mut cl: Vec<String> = w
        .query::<(Entity, &Cloud, Has<Flame>, Has<horizon_ecs_proto::breath_cloud::Live>)>()
        .iter(w)
        .map(|(id, c, flame, live)| {
            // A cloud asleep in its wait does not count its age (`breath_cloud`): it is the ticks since it was first seen.
            let born = BORN.with(|b| *b.borrow_mut().entry(id).or_insert(t));
            let age = if live { c.age } else { (t - born) as i32 };
            let y = if flame { format!("{:016x}", c.at[1].to_bits()) } else { "~".to_string() };
            format!("{:016x}/{y}/{:016x}/{:08x}/{}/{}", c.at[0].to_bits(), c.at[2].to_bits(), c.radius.to_bits(), age, c.victims.len())
        })
        .collect();
    cl.sort();
    let (p, y) = (w.get::<Pos>(e).unwrap().0, w.get::<Yaw>(e).unwrap().0);
    let (ph, hp) = (w.get::<phases::PhaseId>(e).unwrap().0, w.get::<Health>(e).unwrap().0);
    format!("{name} {t} {} {:08x} p{ph} h{:08x} fb[{}] cl[{}]", bits(p), y.to_bits(), hp.to_bits(), fb.join(";"), cl.join(";"))
}

#[test]
fn strafe_charge_and_perch_match_horizon() {
    if !crate::common::have_data() {
        return;
    }
    let me = Vec3::new(PLAYER.0, PLAYER.1, PLAYER.2);
    let mut out = Vec::new();
    let mut rng = Vec::new();
    for (name, ticks) in [("strafe", 700), ("charge", 80), ("perch", 1900)] {
        let mut h = herd();
        let e = dragon(&mut h, 10, 200.0);
        match name {
            "strafe" => phases::enter(&mut h.world, e, phases::STRAFE, PhaseState { player: Some(PLAYER_ID), ..Default::default() }),
            "charge" => phases::enter(&mut h.world, e, phases::CHARGING, PhaseState { target: Some(me), ..Default::default() }),
            _ => phases::set_phase(&mut h.world, e, phases::LANDING_APPROACH),
        }
        for t in 0..ticks {
            h.tick();
            out.push(frame(&mut h, e, name, t));
            rng.push(format!("{name} {t} {:012x}", h.world.get::<horizon_ecs_proto::dragon::flight::DragonRng>(e).unwrap().0.state()));
        }
    }
    if let Some(dir) = std::env::var_os("HORIZON_DRAGON_DUMP") {
        std::fs::write(std::path::Path::new(&dir).join("combat_rng.txt"), rng.join("\n")).unwrap();
    }
    compare("combat", out);
}
