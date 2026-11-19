//! The wither against a real `Mob` wither in the same scene (`HORIZON_TEST_RESOURCES`, process species `minecraft:wither`): the end
//! of the charge (the blast and its roar at the same tick), the volleys of the side heads (the shot sound of each, the first one
//! at the same tick, the same count), and the blocks broken after an accepted hurt (the same cells).
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{MobEvent, PlayerView};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{
    behaviours::invulnerable_charge,
    cow::*,
    goals::*,
    hostile::PlayerState,
    host_out::HostEvent,
    targeting::*,
};

fn skip() -> bool {
    !have_data() || horizon_ecs_proto::cow::skip_unless(&invulnerable_charge::MODULE, "the wither charge")
}
struct Scene {
    r: MobRef,
    h: Herd,
    mob: usize,
    ecs: Entity,
}
/// A wither in its charge on both sides (`counter` 220 on the Mob), a player 10 blocks away.
fn scene(at: Vec3, player: Vec3) -> Scene {
    let mut r = MobRef::new(40, false);
    let mut h = Herd::with_slab(40, true, false);
    let mob = r.add(at);
    r.mobs[mob].extra.counter = 220;
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let ecs = spawn_species(&mut h.world, &sp, at.x, at.z, 77);
    h.world.get_mut::<ActiveUntil>(ecs).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0 });
    h.world.resource_mut::<PlayerState>().immortal = true;
    set_target(&mut h.world, ecs, Target::player(1, 30));
    Scene { r, h, mob, ecs }
}
/// What one side made in a tick: the Mob's events, and the host's events of the ECS.
#[derive(Default)]
struct Tick {
    blast: bool,
    shots: u32,
    breaks: Vec<[i32; 3]>,
}
fn step(s: &mut Scene, view: Option<&PlayerView>) -> (Tick, Tick) {
    let from = s.r.events.len();
    s.r.step(view);
    s.h.tick();
    let mut mob = Tick::default();
    for ev in &s.r.events[from..] {
        match ev {
            MobEvent::Explode(..) => mob.blast = true,
            MobEvent::Sound(name, ..) if *name == "entity.wither.shoot" => mob.shots += 1,
            MobEvent::BreakBlock { pos, .. } => mob.breaks.push(*pos),
            _ => {}
        }
    }
    let mut ecs = Tick::default();
    for ev in s.h.world.resource_mut::<Events>().host.drain() {
        match ev {
            HostEvent::Sound { name: "entity.wither.spawn", .. } => ecs.blast = true,
            HostEvent::Sound { name: "entity.wither.shoot", .. } => ecs.shots += 1,
            HostEvent::SetBlock { x, y, z, name } if name == "minecraft:air" => ecs.breaks.push([x, y, z]),
            _ => {}
        }
    }
    (mob, ecs)
}

/// The charge lasts 220 ticks on both sides: the blast and the spawn roar come at the same tick, within one.
#[test]
fn the_charge_ends_with_the_blast_at_the_mob_s_tick() {
    if skip() {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(40.5, 1.0, 0.5));
    let (mut m, mut e) = (None, None);
    for t in 1..=230i64 {
        let (mob, ecs) = step(&mut s, None);
        if mob.blast && m.is_none() {
            m = Some(t);
        }
        if ecs.blast && e.is_none() {
            e = Some(t);
        }
    }
    eprintln!("blast: mob {m:?}, ecs {e:?}");
    let (m, e) = (m.expect("the Mob blasts"), e.expect("the ECS blasts"));
    assert!((m - e).abs() <= 1, "mob {m}, ecs {e}");
    assert!((219..=221).contains(&m), "mob {m}");
}

/// After the charge, a player 10 blocks away: the side heads and the centre head volley on both sides; the first shot sound
/// at the same tick (within 2), and the same count over 200 ticks (within 2).
#[test]
fn the_side_heads_volley_as_the_mob_s_do() {
    if skip() {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(10.5, 1.0, 0.5));
    let view = player_view(Vec3::new(10.5, 1.0, 0.5), None);
    for _ in 0..230 {
        step(&mut s, Some(&view));
    }
    let (mut m, mut e) = (Vec::new(), Vec::new());
    for t in 1..=200i64 {
        let (mob, ecs) = step(&mut s, Some(&view));
        for _ in 0..mob.shots {
            m.push(t);
        }
        for _ in 0..ecs.shots {
            e.push(t);
        }
    }
    eprintln!("shots: mob {m:?}, ecs {e:?}");
    assert!(m.len() >= 4 && e.len() >= 4, "mob {}, ecs {}", m.len(), e.len());
    assert!((m[0] - e[0]).abs() <= 2, "first shot: mob {}, ecs {}", m[0], e[0]);
    assert!((m.len() as i64 - e.len() as i64).abs() <= 2, "count: mob {}, ecs {}", m.len(), e.len());
}

/// An accepted hurt (1 health from the player) starts the 20-tick countdown: the same cells are broken on both sides, in the
/// box of the wither, once the countdown ends.
#[test]
fn a_hurt_breaks_the_same_cells_as_the_mob() {
    if skip() {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(10.5, 1.0, 0.5));
    for _ in 0..230 {
        step(&mut s, None);
    }
    s.r.hit(s.mob, 1.0, [10.5, 1.0, 0.5]);
    s.h.hit(s.ecs, 1.0, [10.5, 1.0, 0.5]);
    let (mut m, mut e) = (Vec::new(), Vec::new());
    for _ in 0..40 {
        let (mob, ecs) = step(&mut s, None);
        m.extend(mob.breaks);
        e.extend(ecs.breaks);
    }
    m.sort();
    e.sort();
    eprintln!("breaks: mob {m:?}, ecs {e:?}");
    assert_eq!(m, e);
}
