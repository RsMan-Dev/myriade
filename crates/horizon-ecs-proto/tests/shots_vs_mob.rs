//! The projectile socle (hurting projectiles, a rate-of-fire shooting module) against the `Mob`'s own `Projectile::tick` and goals
//! (`HORIZON_TEST_RESOURCES` set): the flight of a small fireball, a large fireball and a spit tick by tick (same position and velocity,
//! gone at the same tick), then the shots of a ghast (one every 60 ticks, from the same place along the same line) and of a blaze (three per
//! 178 ticks) at a player standing in the open. `HORIZON_SPECIES` is not read: the species are taken by name.
mod common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::{
    arrows,
    mobs::{MobEvent, Who as MWho},
    projectiles::{Hit, Projectile, ProjectileKind, ProjectileWorld},
};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::{
    behaviours::{burst_shot, charged_shot, Entry},
    cow::*,
    goals::*,
    projectile::{launch, Flight, Projectile as EProjectile},
    species::Config,
    targeting::{set_target, Target, Who},
};
use std::collections::HashMap;

struct Open<'a>(&'a MobRef);
impl ProjectileWorld for Open<'_> {
    fn clip(&self, from: Vec3, to: Vec3) -> Option<(Vec3, [i32; 3], [i32; 3])> {
        let mw = self.0.w;
        let movement = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z);
        arrows::clip_block_hit(mw.world, mw.facts, mw.collision, from, movement).map(|(at, p, n)| (at, [p.x, p.y, p.z], n))
    }
    fn in_water(&self, _: Aabb) -> bool {
        false
    }
    fn targets(&self) -> &[(MWho, Aabb)] {
        &[]
    }
}
fn close(a: Vec3, b: Vec3) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9 && (a.z - b.z).abs() < 1e-9
}

/// The same start and velocity: the same position every tick and the same tick of the end, on the ground 20 blocks away.
#[test]
fn the_hurting_and_thrown_kinds_fly_as_the_mob_s_do() {
    if !have_data() {
        return;
    }
    let kinds: [(&str, ProjectileKind, Flight, Vec3); 3] = [
        ("small fireball", ProjectileKind::SmallFireball, Flight::SMALL_FIREBALL, Vec3::new(0.02, -0.01, 0.1)),
        ("large fireball", ProjectileKind::LargeFireball { power: 1 }, Flight::large_fireball(1), Vec3::new(-0.03, -0.012, 0.1)),
        ("spit", ProjectileKind::LlamaSpit, Flight::SPIT, Vec3::new(0.2, 0.3, 0.5)),
    ];
    for (name, kind, flight, v) in kinds {
        let (r, mut h) = flat_pair();
        let at = Vec3::new(0.5, 3.0, 0.5);
        let mut m = Projectile::new(kind, at, None);
        m.velocity = v;
        let e = launch(&mut h.world, flight, at, v, None);
        let world = Open(&r);
        let mut gone = None;
        for t in 1..=400i64 {
            let (keep, hit) = m.tick(&world);
            h.tick();
            if !keep || !matches!(hit, Hit::None) {
                gone = Some(t);
                assert!(h.world.get_entity(e).is_err(), "{name}: gone together at tick {t}");
                break;
            }
            let p = h.world.get::<Pos>(e).unwrap().0;
            assert!(close(p, m.position), "{name} tick {t}: ecs {p:?} mob {:?}", m.position);
        }
        eprintln!("{name}: gone at tick {gone:?} in both");
        assert!(gone.is_some(), "{name} never ended");
    }
}

fn shots_of_mob(kind: &str, at: Vec3, player: Vec3, ticks: usize) -> Vec<(usize, Vec3, Vec3)> {
    let mut r = MobRef::new(24, false);
    let i = r.add_kind(at, mob_kind(kind));
    r.mobs[i].ai.target = Some(MWho::Player);
    let view = player_view(player, None);
    let mut shots = Vec::new();
    for t in 0..ticks {
        let before = r.events.len();
        r.step(Some(&view));
        for e in &r.events[before..] {
            if let MobEvent::Projectile(p) = e {
                shots.push((t, p.position, p.velocity));
            }
        }
        // The creature is held where it is: its own wanderings are not what is compared.
        r.mobs[i].position = at;
        r.mobs[i].velocity = Vec3::ZERO;
        r.mobs[i].yaw = 0.0;
        r.mobs[i].body_yaw = 0.0;
    }
    shots
}
fn shots_of_ecs(name: &str, entry: Entry, at: Vec3, player: Vec3, ticks: usize) -> Vec<(usize, Vec3, Vec3)> {
    let mut h = Herd::with_slab(24, false, false);
    set_lazy_look(&mut h.world, false);
    h.set_online(&[(player.x, player.z)]);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == name).unwrap();
    sp.goals = Config { entries: Box::leak(Box::new([entry])) };
    let e = spawn_species(&mut h.world, &sp, at.x, at.z, 99);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    h.tick();
    teleport(&mut h.world, e, at);
    h.world.get_mut::<Yaw>(e).unwrap().0 = 0.0;
    set_target(&mut h.world, e, Target::new(Who::Player(1), 200, u8::MAX));
    let mut first: HashMap<Entity, usize> = HashMap::new();
    let mut shots = Vec::new();
    for t in 0..ticks {
        h.tick();
        let found: Vec<(Entity, Vec3, Vec3)> = h.world.query::<(Entity, &Pos, &Vel, &EProjectile)>().iter(&h.world).map(|(e, p, v, _)| (e, p.0, v.0)).collect();
        for (id, p, v) in found {
            if first.insert(id, t).is_none() {
                shots.push((t, p, v));
            }
        }
        teleport(&mut h.world, e, at);
        h.world.get_mut::<Yaw>(e).unwrap().0 = 0.0;
    }
    shots
}
fn gaps(v: &[(usize, Vec3, Vec3)]) -> Vec<usize> {
    v.windows(2).map(|w| w[1].0 - w[0].0).collect()
}

/// A ghast 6 blocks up, a player 28 blocks away: a fireball every 60 ticks from 4 blocks ahead of it along the line to the player's
/// middle. The ECS one is seen a tick later or not (the system order): its start, moved by one tick of flight or not, is the Mob's.
#[test]
fn a_ghast_shoots_as_the_mob_s_does() {
    if !have_data() {
        return;
    }
    let (at, player) = (Vec3::new(0.5, 6.0, 0.5), Vec3::new(0.5, 1.0, 28.5));
    let mob = shots_of_mob("minecraft:ghast", at, player, 400);
    let ecs = shots_of_ecs("minecraft:ghast", Entry::new(&charged_shot::MODULE, 7, &charged_shot::Params::GHAST), at, player, 400);
    eprintln!("ghast: mob {:?} ecs {:?}", mob.iter().map(|s| s.0).collect::<Vec<_>>(), ecs.iter().map(|s| s.0).collect::<Vec<_>>());
    assert!(mob.len() >= 5 && ecs.len() >= 5);
    assert!(gaps(&mob).iter().all(|&g| g == 60) && gaps(&ecs).iter().all(|&g| g == 60), "mob {:?} ecs {:?}", gaps(&mob), gaps(&ecs));
    let (m, e) = (&mob[0], &ecs[0]);
    // The ECS projectile is read after its first tick of flight or before: compare with the Mob's start moved 0 or 1 tick.
    let mut p = Projectile::new(ProjectileKind::LargeFireball { power: 1 }, m.1, None);
    p.velocity = m.2;
    let r = MobRef::new(24, false);
    let w = Open(&r);
    let same_start = close(e.1, m.1) && close(e.2, m.2);
    let _ = p.tick(&w);
    let one_tick = close(e.1, p.position) && close(e.2, p.velocity);
    assert!(same_start || one_tick, "mob start {:?} v {:?}; ecs {:?} v {:?}", m.1, m.2, e.1, e.2);
    let offset = ecs[0].0 as i64 - mob[0].0 as i64;
    eprintln!("first shot: ecs - mob = {offset} ticks");
    assert!(offset.abs() <= 2, "{offset}");
}

/// A blaze 6 blocks up, a player 20 blocks away: bursts of three small fireballs 6 ticks apart, a burst every 178 ticks.
#[test]
fn a_blaze_shoots_in_bursts_as_the_mob_s_does() {
    if !have_data() {
        return;
    }
    let (at, player) = (Vec3::new(0.5, 1.0, 0.5), Vec3::new(0.5, 1.0, 20.5));
    let mob = shots_of_mob("minecraft:blaze", at, player, 5400);
    let ecs = shots_of_ecs("minecraft:blaze", Entry::new(&burst_shot::MODULE, 4, &burst_shot::Params::BLAZE), at, player, 5400);
    eprintln!("blaze: {} / {} shots, first ticks mob {:?} ecs {:?}", mob.len(), ecs.len(), mob.iter().take(4).map(|s| s.0).collect::<Vec<_>>(), ecs.iter().take(4).map(|s| s.0).collect::<Vec<_>>());
    assert!(mob.len() >= 60 && ecs.len() >= 60);
    assert_eq!(gaps(&mob)[..58], gaps(&ecs)[..58], "the same rhythm");
    // The aim: the mean direction is the line to the player's middle, the spread grows as the fourth root of the distance.
    let law = |s: &[(usize, Vec3, Vec3)]| {
        let n = s.len() as f64;
        let ux: Vec<f64> = s.iter().map(|x| x.2.x / (x.2.x.powi(2) + x.2.y.powi(2) + x.2.z.powi(2)).sqrt()).collect();
        let mean = ux.iter().sum::<f64>() / n;
        let sd = (ux.iter().map(|u| (u - mean).powi(2)).sum::<f64>() / n).sqrt();
        (mean, sd)
    };
    let (lm, le) = (law(&mob), law(&ecs));
    eprintln!("lateral direction (mean, sd): mob {lm:?} ecs {le:?}");
    assert!(lm.0.abs() < 0.05 && le.0.abs() < 0.05 && (lm.1 / le.1 - 1.0).abs() < 0.5, "{lm:?} {le:?}");
}

// ---- the witch's potion --------------------------------------------------------------------------------------------------------------

/// The first potion a `Mob` witch throws at a cow `dist` blocks away that has these effects: its potion's name.
fn mob_potion(dist: f64, effects: &[(&'static str, i32)], id: u32) -> Option<String> {
    let mut r = MobRef::new(24, false);
    let mut m = horizon_client_lib::mobs::Mob::new(id, mob_kind("minecraft:witch"), Vec3::new(0.5, 1.0, 0.5), 0.0);
    m.persistent = true;
    m.ai.no_action_time = 100;
    scope(&mut m);
    r.mobs.push(m);
    r.searches.push(0);
    let w = r.mobs.len() - 1;
    let c = r.add_kind(Vec3::new(0.5, 1.0, 0.5 + dist), mob_kind("minecraft:cow"));
    for &(e, ticks) in effects {
        r.mobs[c].add_effect(e, ticks, 0);
    }
    r.mobs[w].ai.target = Some(MWho::Mob(r.mobs[c].id));
    for _ in 0..120 {
        let before = r.events.len();
        r.step(None);
        for e in &r.events[before..] {
            if let MobEvent::Projectile(p) = e {
                if let ProjectileKind::Potion(stack, _) = &p.kind {
                    return stack.data().and_then(|d| d.potion.as_ref()).map(|s| s.to_string());
                }
            }
        }
        if std::env::var_os("WITCH_DBG").is_some() && r.t % 20 == 0 {
            eprintln!("t {} target {:?} goals {:?} pos {:?}", r.t, r.mobs[w].ai.target, r.mobs[w].ai.goals.running_names(), r.mobs[w].position);
        }
        // The cow stays where it is.
        r.mobs[c].position = Vec3::new(0.5, 1.0, 0.5 + dist);
        r.mobs[c].velocity = Vec3::ZERO;
    }
    None
}
/// The same with the ECS witch: the first brew of the potion it throws.
fn ecs_potion(dist: f64, effects: &[(horizon_ecs_proto::effects::Kind, i32)], seed: u64) -> Option<horizon_ecs_proto::effects::Brew> {
    let mut h = Herd::with_slab(24, false, false);
    set_lazy_look(&mut h.world, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let mut sp = *table.iter().find(|s| s.name == "minecraft:witch").unwrap();
    sp.goals = Config { entries: Box::leak(Box::new([Entry::new(&horizon_ecs_proto::behaviours::ranged_attack::MODULE, 2, &horizon_ecs_proto::behaviours::ranged_attack::Params::WITCH)])) };
    let mut cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
    cow.goals = Config { entries: &[] };
    let w = spawn_species(&mut h.world, &sp, 0.5, 0.5, seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let c = spawn_species(&mut h.world, &cow, 0.5, 0.5 + dist, 4);
    for e in [w, c] {
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.resource_mut::<Entered>().0.push(e);
    }
    h.tick();
    for &(k, t) in effects {
        horizon_ecs_proto::effects::add(&mut h.world, c, k, 0, t);
    }
    set_target(&mut h.world, w, Target::new(Who::Entity(c), 200, u8::MAX));
    for _ in 0..120 {
        h.tick();
        let found = h.world.query::<&Flight>().iter(&h.world).find_map(|f| if let horizon_ecs_proto::projectile::OnHit::Splash(b) = f.hit { b.first().copied() } else { None });
        if found.is_some() {
            return found;
        }
        teleport(&mut h.world, c, Vec3::new(0.5, 1.0, 0.5 + dist));
    }
    None
}

/// Slowness from afar, poison on a healthy target, harming when it has both, weakness up close a quarter of the time.
#[test]
fn a_witch_chooses_its_potion_by_its_target_as_the_mob_does() {
    use horizon_ecs_proto::effects::{Brew, Kind};
    if !have_data() {
        return;
    }
    let name = |b: Brew| match b {
        Brew::Timed(Kind::Slowness, ..) => "minecraft:slowness",
        Brew::Timed(Kind::Poison, ..) => "minecraft:poison",
        Brew::Timed(Kind::Weakness, ..) => "minecraft:weakness",
        Brew::Harm(_) => "minecraft:harming",
        _ => "other",
    };
    let slow = ("minecraft:slowness", 400);
    let poison = ("minecraft:poison", 400);
    let cases: [(f64, Vec<(&'static str, i32)>, Vec<(Kind, i32)>, &str); 4] = [
        (12.0, vec![], vec![], "minecraft:slowness"),
        (5.0, vec![slow], vec![(Kind::Slowness, 400)], "minecraft:poison"),
        (5.0, vec![slow, poison], vec![(Kind::Slowness, 400), (Kind::Poison, 400)], "minecraft:harming"),
        (12.0, vec![slow], vec![(Kind::Slowness, 400)], "minecraft:poison"),
    ];
    for (k, (dist, m_fx, e_fx, want)) in cases.into_iter().enumerate() {
        let m = mob_potion(dist, &m_fx, 7000 + 10 * k as u32);
        let e = ecs_potion(dist, &e_fx, 70 + k as u64).map(name);
        eprintln!("distance {dist}, effects {m_fx:?}: mob {m:?} ecs {e:?}");
        assert_eq!(m.as_deref(), Some(want), "the Mob");
        assert_eq!(e, Some(want), "the ECS");
    }
    // Up close with slowness and poison: weakness one time in four (the Mob and the ECS, 120 witches each).
    let fx = [("minecraft:slowness", 400), ("minecraft:poison", 400)];
    let fe = [(Kind::Slowness, 400), (Kind::Poison, 400)];
    let mw = (0..120).filter(|&k| mob_potion(2.5, &fx, 9000 + 10 * k).as_deref() == Some("minecraft:weakness")).count();
    let ew = (0..120).filter(|&k| ecs_potion(2.5, &fe, 300 + k).map(name) == Some("minecraft:weakness")).count();
    eprintln!("weakness up close, of 120: mob {mw} ecs {ew}");
    assert!((15..=45).contains(&mw) && (15..=45).contains(&ew), "{mw} {ew}");
}

/// A splash potion: the effects reach the creatures near where it breaks, scaled by the nearness, the one it hits whole.
#[test]
fn a_splash_puts_its_effects_on_the_creatures_near_it() {
    use horizon_ecs_proto::effects::{self, Brew, Kind};
    if !have_data() {
        return;
    }
    let (_, mut h) = flat_pair();
    let hit = ecs_cow(&mut h, 0.5, 1.0, 0.5, 5);
    let near = ecs_cow(&mut h, 2.5, 1.0, 0.5, 6);
    let far = ecs_cow(&mut h, 9.5, 1.0, 0.5, 7);
    h.tick();
    static BREWS: [Brew; 2] = [Brew::Timed(Kind::Poison, 0, 900), Brew::Harm(0)];
    let at = Vec3::new(0.5, 1.0, 0.5);
    effects::splash(&mut h.world, at, Some(hit), &BREWS, None);
    let remaining = |h: &Herd, e: Entity, now: u64| h.world.get::<effects::Effects>(e).and_then(|f| f.list.first().map(|x| x.ends + 1 - now));
    let now = h.world.resource::<Clock>().0;
    eprintln!("poison ticks: hit {:?} near {:?} far {:?}", remaining(&h, hit, now), remaining(&h, near, now), remaining(&h, far, now));
    assert_eq!(remaining(&h, hit, now), Some((0.75_f64 * 900.0 + 0.5) as u64 + 1));
    assert_eq!(remaining(&h, near, now), Some((0.5_f64 * 900.0 * 0.75 + 0.5) as u64 + 1));
    assert_eq!(remaining(&h, far, now), None);
}
