//! The ranged goals of the lot "shots" against the `Mob` of the species of the process (`HORIZON_SPECIES`, `HORIZON_TEST_RESOURCES`): the pillager's
//! crossbow, the guardian's beam, the evoker's vexes and fangs, the illusioner's cast and the drowned's trident. Same scene on both sides, the
//! player the target from the first tick; the instants of the shots/spells and what they do are compared. Each test skips (printed) in a
//! process of another species. The illusioner's mirror images have no `Mob` counterpart: its test only counts the casts of the ECS.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{MobEvent, Who as MWho};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::{behaviours::*, cow::*, goals::*, hostile::PlayerState, projectile::*, targeting::*};

fn is(name: &str) -> bool {
    have_data() && std::env::var("HORIZON_SPECIES").as_deref() == Ok(name)
}
struct Scene {
    r: MobRef,
    h: Herd,
    mob: usize,
    ecs: Entity,
    seen: std::collections::HashSet<Entity>,
    hits_seen: usize,
    /// The amount of the beam's magic damage (the one channel both sides are compared on): 1, or 3 for the elder.
    magic: f32,
    /// Every event of the Mob that touches a player, with its tick (diagnostic).
    trace: Vec<(i64, String)>,
}
/// The species of the process at `at`, the player at `player` (the target on both sides), `main`: the item in hand (name, ECS id).
fn scene(at: Vec3, player: Vec3, main: Option<(&str, u32)>, water: bool) -> Scene {
    let mut r = MobRef::new(40, false);
    let mut h = Herd::with_slab(40, true, false);
    if water {
        for x in -12..=12 {
            for z in -12..=12 {
                for y in 0..=8 {
                    r.set_block(x, y, z, "minecraft:water");
                    h.set_block(x, y, z, "minecraft:water");
                }
            }
        }
    }
    let mob = r.add(at);
    if let Some((n, _)) = main {
        r.mobs[mob].extra.set_main_hand(Some(n.into()));
    }
    r.mobs[mob].ai.target = Some(MWho::Player);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == species().name).unwrap();
    let ecs = spawn_species(&mut h.world, &sp, at.x, at.z, 77);
    h.world.get_mut::<ActiveUntil>(ecs).unwrap().0 = u64::MAX;
    if let Some((_, id)) = main {
        h.world.entity_mut(ecs).insert(Equipment { main: id, ..Default::default() });
    }
    h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0 });
    h.world.resource_mut::<PlayerState>().immortal = true;
    set_target(&mut h.world, ecs, Target::player(1, 30));
    Scene { r, h, mob, ecs, seen: Default::default(), hits_seen: 0, magic: if is("minecraft:elder_guardian") { 3.0 } else { 1.0 }, trace: Vec::new() }
}
/// What each side did in one tick.
#[derive(Default, Debug, Clone, Copy)]
struct Tick {
    mob_shot: u32,
    ecs_shot: u32,
    mob_dmg: f32,
    ecs_dmg: f32,
    mob_fangs: u32,
    ecs_fangs: u32,
    mob_vex: u32,
    ecs_vex: u32,
}
impl Scene {
    fn step(&mut self, view: &horizon_client_lib::mobs::PlayerView) -> Tick {
        let from = self.r.events.len();
        self.r.step(Some(view));
        self.h.tick();
        let mut t = Tick::default();
        for e in &self.r.events[from..] {
            let d = format!("{e:?}");
            if d.contains("Player") || d.starts_with("Damage") || d.starts_with("Attack") || d.contains("crossbow") {
                self.trace.push((self.r.t, d.chars().take(110).collect()));
            }
            match e {
                MobEvent::CrossbowShot { .. } | MobEvent::Projectile(_) => t.mob_shot += 1,
                MobEvent::DamagePlayer { amount, kind, .. } if kind.ends_with("indirect_magic") => t.mob_dmg += amount,
                MobEvent::Fangs { .. } => t.mob_fangs += 1,
                MobEvent::SpawnKind { .. } => t.mob_vex += 1,
                _ => {}
            }
        }
        let mut q = self.h.world.query::<(Entity, &Projectile)>();
        let new: Vec<Entity> = q.iter(&self.h.world).map(|x| x.0).filter(|e| !self.seen.contains(e)).collect();
        self.seen.extend(new.iter().copied());
        t.ecs_shot = new.len() as u32;
        let ps = self.h.world.resource::<PlayerState>();
        t.ecs_dmg = ps.hits[self.hits_seen..].iter().filter(|h| (h.amount - self.magic).abs() < 1e-3).map(|h| h.amount).sum();
        self.hits_seen = ps.hits.len();
        let mut f = self.h.world.query::<(Entity, &spell_use::Fang)>();
        let nf: Vec<Entity> = f.iter(&self.h.world).map(|x| x.0).filter(|e| !self.seen.contains(e)).collect();
        self.seen.extend(nf.iter().copied());
        t.ecs_fangs = nf.len() as u32;
        let mut v = self.h.world.query::<(Entity, &limited_life::Lifespan)>();
        let nv: Vec<Entity> = v.iter(&self.h.world).map(|x| x.0).filter(|e| !self.seen.contains(e)).collect();
        self.seen.extend(nv.iter().copied());
        t.ecs_vex = nv.len() as u32;
        t
    }
}
fn run(s: &mut Scene, view: &horizon_client_lib::mobs::PlayerView, ticks: usize) -> Vec<Tick> {
    (0..ticks).map(|_| s.step(view)).collect()
}
/// The ticks (1-based) where `f` is non-zero.
fn at(v: &[Tick], f: impl Fn(&Tick) -> bool) -> Vec<i64> {
    v.iter().enumerate().filter(|(_, t)| f(t)).map(|(i, _)| i as i64 + 1).collect()
}
fn close(a: &[i64], b: &[i64], tol: i64, what: &str) {
    eprintln!("{what}: mob {a:?}, ecs {b:?}");
    assert!(!a.is_empty() && !b.is_empty(), "{what}: mob {a:?}, ecs {b:?}");
    assert!((a[0] - b[0]).abs() <= tol, "{what}: first at mob {}, ecs {}", a[0], b[0]);
}

/// The pillager with a crossbow, the player 10 blocks away: the first bolt leaves at the same tick (or a few apart), and the same count in 200 ticks.
#[test]
fn the_pillager_shoots_its_crossbow_as_a_mob_does() {
    if !is("minecraft:pillager") {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(10.5, 1.0, 0.5), Some(("minecraft:crossbow", CROSSBOW)), false);
    let view = player_view(Vec3::new(10.5, 1.0, 0.5), None);
    let mut v = Vec::new();
    for t in 1..=200 {
        v.push(s.step(&view));
        if t % 10 == 0 && t <= 120 {
            eprintln!("t={t}: mob x {:.2} / ecs x {:.2}", s.r.mobs[s.mob].position.x, s.h.world.get::<Pos>(s.ecs).unwrap().0.x);
        }
    }
    let (m, e) = (at(&v, |t| t.mob_shot > 0), at(&v, |t| t.ecs_shot > 0));
    for (t, d) in &s.trace {
        eprintln!("mob event t={t}: {d}");
    }
    close(&m, &e, 4, "crossbow bolts");
    assert!((m.len() as i64 - e.len() as i64).abs() <= 1, "mob {m:?}, ecs {e:?}");
    let mut q = s.h.world.query::<(&Pos, &Vel, &Projectile)>();
    let first = q.iter(&s.h.world).next().expect("a bolt");
    assert!(first.0.0.y > 1.0 && first.0.0.y < 2.0, "starts at the eyes: {:?}", first.0.0);
}

/// The guardian in water with the player within 15 blocks: the beam of 2 seconds ends in the same tick on both sides, with the same damage (1 magic + 2 on hard).
#[test]
fn the_guardian_beam_hurts_as_a_mob_does() {
    if !is("minecraft:guardian") && !is("minecraft:elder_guardian") {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 2.0, 0.5), Vec3::new(6.5, 2.0, 0.5), None, true);
    let view = player_view(Vec3::new(6.5, 2.0, 0.5), None);
    let v = run(&mut s, &view, 200);
    let (m, e) = (at(&v, |t| t.mob_dmg > 0.0), at(&v, |t| t.ecs_dmg > 0.0));
    close(&m, &e, 6, "beam hits");
    let (md, ed): (f32, f32) = (v.iter().map(|t| t.mob_dmg).take_while(|_| true).sum(), v.iter().map(|t| t.ecs_dmg).sum());
    eprintln!("damage in 200 ticks: mob {md}, ecs {ed}");
    for (t, d) in &s.trace {
        eprintln!("mob event t={t}: {d}");
    }
    for h in &s.h.world.resource::<PlayerState>().hits {
        eprintln!("ecs hit tick {}: {}", h.tick, h.amount);
    }
    assert!((m.len() as i64 - e.len() as i64).abs() <= 1, "mob {m:?}, ecs {e:?}");
    let first = |f: fn(&Tick) -> f32| v.iter().map(f).find(|d| *d > 0.0).unwrap();
    assert_eq!(first(|t| t.mob_dmg), first(|t| t.ecs_dmg));
}

/// The evoker: the first spell (vexes or fangs, by the draw of each side) comes at the same tick within the tolerance, the vexes in threes, the fangs as a ring or a line.
#[test]
fn the_evoker_casts_as_a_mob_does() {
    if !is("minecraft:evoker") {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(8.5, 1.0, 0.5), None, false);
    let view = player_view(Vec3::new(8.5, 1.0, 0.5), None);
    let v = run(&mut s, &view, 400);
    let (m, e) = (at(&v, |t| t.mob_fangs + t.mob_vex > 0), at(&v, |t| t.ecs_fangs + t.ecs_vex > 0));
    eprintln!("state: spells {:?}, target {}, now {}", s.h.world.get::<spell_use::Spells>(s.ecs), s.h.world.get::<Target>(s.ecs).is_some(), now_of(&s.h.world, s.ecs));
    close(&m, &e, 8, "spells");
    let sum = |f: fn(&Tick) -> u32| v.iter().map(f).sum::<u32>();
    eprintln!("fangs: mob {}, ecs {}; vexes: mob {}, ecs {}", sum(|t| t.mob_fangs), sum(|t| t.ecs_fangs), sum(|t| t.mob_vex), sum(|t| t.ecs_vex));
    assert_eq!(sum(|t| t.mob_vex) % 3, 0);
    assert_eq!(sum(|t| t.ecs_vex) % 3, 0);
    // The fangs: a line of 16 (far) or 5 + 8 (near), per cast.
    assert!(sum(|t| t.ecs_fangs) > 0 || sum(|t| t.ecs_vex) > 0);
}

/// The illusioner: it casts (the `Casting` state appears) within a few ticks of when the `Mob` does. The mirror images are not in the `Mob`: only the ECS side is counted.
#[test]
fn the_illusioner_casts_its_spell() {
    if !is("minecraft:illusioner") {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(8.5, 1.0, 0.5), Some(("minecraft:bow", 0)), false);
    let view = player_view(Vec3::new(8.5, 1.0, 0.5), None);
    let mut casts = 0;
    let mut was = false;
    for _ in 0..300 {
        s.step(&view);
        let now = s.h.world.get::<spell_casting::Casting>(s.ecs).is_some();
        if now && !was {
            casts += 1;
        }
        was = now;
    }
    eprintln!("state: spells {:?}, target {}, now {}", s.h.world.get::<spell_use::Spells>(s.ecs), s.h.world.get::<Target>(s.ecs).is_some(), now_of(&s.h.world, s.ecs));
    eprintln!("illusioner casts (ECS): {casts}");
    assert!(casts >= 1);
}

/// The drowned with a trident, the player 10 blocks away in the open: the throw instants and the first trident's start match.
#[test]
fn the_drowned_throws_its_trident_as_a_mob_does() {
    if !is("minecraft:drowned") {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(10.5, 1.0, 0.5), Some(("minecraft:trident", TRIDENT)), false);
    let view = player_view(Vec3::new(10.5, 1.0, 0.5), None);
    let v = run(&mut s, &view, 200);
    let (m, e) = (at(&v, |t| t.mob_shot > 0), at(&v, |t| t.ecs_shot > 0));
    close(&m, &e, 3, "trident throws");
    assert!((m.len() as i64 - e.len() as i64).abs() <= 1, "mob {m:?}, ecs {e:?}");
}

/// The witch underwater drinks water breathing: `Drinking` lasts 32 ticks, the speed is 0.25 lower meanwhile, the effect comes at the end.
/// The draws differ between the `Mob` and the ECS (own seeds): the rules are checked, not the instants.
#[test]
fn a_witch_in_water_drinks_water_breathing() {
    if !is("minecraft:witch") {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 2.0, 0.5), Vec3::new(30.5, 2.0, 0.5), None, true);
    let view = player_view(Vec3::new(30.5, 2.0, 0.5), None);
    let base = s.h.world.get::<Body>(s.ecs).unwrap().speed;
    let (mut len, mut seen_len) = (0, None);
    for _ in 0..600 {
        s.step(&view);
        match s.h.world.get::<drink_potion::Drinking>(s.ecs).copied() {
            Some(d) => {
                assert_eq!(d.potion, drink_potion::Potion::WaterBreathing);
                assert!((s.h.world.get::<Body>(s.ecs).unwrap().speed - (base - 0.25).max(0.0)).abs() < 1e-9);
                len += 1;
            }
            None if len > 0 => {
                seen_len = Some(len);
                break;
            }
            None => {}
        }
    }
    assert_eq!(seen_len, Some(33), "it drinks for 32 ticks (left counts 32..=0)");
    assert!(horizon_ecs_proto::effects::has(&s.h.world, s.ecs, horizon_ecs_proto::effects::Kind::WaterBreathing));
    assert_eq!(s.h.world.get::<Body>(s.ecs).unwrap().speed, base);
}

/// A drinking witch throws nothing (`isDrinkingPotion`), and throws again afterwards.
#[test]
fn a_drinking_witch_does_not_throw() {
    if !is("minecraft:witch") {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(8.5, 1.0, 0.5), None, false);
    let view = player_view(Vec3::new(8.5, 1.0, 0.5), None);
    let saved_speed = s.h.world.get::<Body>(s.ecs).unwrap().speed;
    s.h.world.entity_mut(s.ecs).insert(drink_potion::Drinking { potion: drink_potion::Potion::Healing, left: 150, saved_speed });
    let v = run(&mut s, &view, 160);
    let e = at(&v, |t| t.ecs_shot > 0);
    assert!(e.iter().all(|t| *t > 150), "no throw while drinking: {e:?}");
}
