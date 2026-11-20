//! The spike crystals and the respawn against Horizon's `Dragon`, tick by tick. Goldens: `dragon_golden/crystals.txt` (the nearest
//! crystal heals the dragon one point every tenth tick; destroying it costs the blast's 10 points; one that is not the nearest costs
//! none) and `dragon_golden/respawn.txt` (the stages of a respawn with four portal crystals, then the spikes and the podium).
use crate::dragon_support::*;
use bevy_ecs::prelude::*;
use horizon_ecs_proto::blocks::{state_id, world_of};
use horizon_ecs_proto::cow::*;
use horizon_ecs_proto::dragon::crystals::{self, EndCrystal};
use horizon_ecs_proto::dragon::fight::{EndFight, FightEvents, NearestCrystal};
use horizon_ecs_proto::dragon::respawn;
use horizon_core::geometry::Vec3;
use horizon_world::worldgen::end_spike::spikes_for_seed;

fn near(h: &Herd, e: Entity) -> String {
    h.world.get::<NearestCrystal>(e).and_then(|n| n.0).map_or("none".to_string(), |c| bits(h.world.get::<Pos>(c).unwrap().0))
}

fn line(h: &Herd, e: Entity, name: &str, t: usize) -> String {
    format!("{name} {t} {} alive{} near {}", snap(h, e), h.world.resource::<EndFight>().crystals_alive, near(h, e))
}

#[test]
fn the_nearest_crystal_heals_and_its_loss_hurts_like_in_horizon() {
    if !crate::common::have_data() {
        return;
    }
    let mut h = herd();
    h.world.insert_resource(EndFight::default());
    h.world.init_resource::<FightEvents>();
    let e = dragon(&mut h, 10, 100.0);
    h.world.resource_mut::<EndFight>().dragon = Some(e);
    let spikes = spikes_for_seed(world_of(&h.world).world_seed);
    let mut out = Vec::new();
    let mut ids = Vec::new();
    for (i, s) in spikes.iter().enumerate() {
        out.push(format!("spike {i} {} {} {}", s.center_x, s.center_z, s.height));
        let at = Vec3::new(f64::from(s.center_x) + 0.5, f64::from(s.height + 1), f64::from(s.center_z) + 0.5);
        ids.push(crystals::spawn(&mut h.world, at, true));
    }
    for t in 0..400 {
        h.tick();
        out.push(line(&h, e, "heal", t));
    }
    for (round, name) in ["nearest", "other"].into_iter().enumerate() {
        let n = h.world.get::<NearestCrystal>(e).and_then(|n| n.0);
        let Some(c) = (if round == 0 { n } else { ids.iter().copied().find(|&c| Some(c) != n) }) else {
            out.push(format!("{name} none"));
            continue;
        };
        out.push(format!("{name} {}", bits(h.world.get::<Pos>(c).unwrap().0)));
        crystals::destroy(&mut h.world, c, Some(PLAYER_ID), false);
        ids.retain(|&x| x != c);
        for t in 0..60 {
            h.tick();
            out.push(line(&h, e, name, t));
        }
    }
    compare("crystals", out);
}

#[test]
fn the_respawn_stages_and_the_rebuilt_spikes_match_horizon() {
    if !crate::common::have_data() {
        return;
    }
    let mut h = herd_of(60);
    h.world.insert_resource(EndFight { killed: true, exit_portal: Some([0, 61, 0]), ..Default::default() });
    h.world.init_resource::<FightEvents>();
    for d in [[0, -1], [0, 1], [-1, 0], [1, 0]] {
        crystals::spawn(&mut h.world, Vec3::new(f64::from(d[0] * 3) + 0.5, 62.0, f64::from(d[1] * 3) + 0.5), true);
    }
    h.world.resource_mut::<EndFight>().crystals_alive = 0;
    assert!(respawn::try_respawn(&mut h.world));
    let mut out = vec![format!("started {:?}", h.world.resource::<EndFight>().respawn)];
    for t in 0..800 {
        h.tick();
        let w = &mut h.world;
        let n = w.query_filtered::<(), With<EndCrystal>>().iter(w).count();
        let f = w.resource::<EndFight>();
        out.push(format!("respawn {t} {:?} crystals{n} dragon{}", f.respawn, f.dragon.is_some()));
    }
    let mw = world_of(&h.world);
    for (i, s) in spikes_for_seed(mw.world_seed).iter().enumerate() {
        let (mut obsidian, mut bars) = (0, 0);
        for y in 0..=s.height + 10 {
            for z in s.center_z - s.radius - 1..=s.center_z + s.radius + 1 {
                for x in s.center_x - s.radius - 1..=s.center_x + s.radius + 1 {
                    match mw.registry.state(state_id(mw, [x, y, z])).map_or("?", |s| s.name.as_str()) {
                        "minecraft:obsidian" => obsidian += 1,
                        "minecraft:iron_bars" => bars += 1,
                        _ => {}
                    }
                }
            }
        }
        out.push(format!("spike {i} obsidian{obsidian} bars{bars}"));
    }
    out.extend(blocks(&h, [0, 200, 0]));
    compare("respawn", out);
}
