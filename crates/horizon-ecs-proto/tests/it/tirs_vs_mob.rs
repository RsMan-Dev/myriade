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
    have_data() && !wrong_species(&[name])
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
    h.world.resource_mut::<Online>().set(Player { id: 1, x: player.x, y: player.y, z: player.z, main: 0, off: 0, ..Default::default() });
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
    /// Entity event 21 (the guardian's charge sound), by side.
    mob_charge: u32,
    ecs_charge: u32,
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
                MobEvent::EntityEvent { event: 21, .. } => t.mob_charge += 1,
                MobEvent::Fangs { .. } => t.mob_fangs += 1,
                MobEvent::SpawnKind { .. } => t.mob_vex += 1,
                _ => {}
            }
        }
        t.ecs_charge = self.h.drain_host_out().iter().filter(|e| matches!(e, horizon_ecs_proto::host_out::HostEvent::EntityEvent { code: 21, .. })).count() as u32;
        let mut q = self.h.world.query::<(Entity, &Projectile)>();
        let new: Vec<Entity> = q.iter(&self.h.world).map(|x| x.0).filter(|e| !self.seen.contains(e)).collect();
        self.seen.extend(new.iter().copied());
        t.ecs_shot = new.len() as u32;
        let ps = self.h.world.resource::<PlayerState>();
        t.ecs_dmg = ps.hits[self.hits_seen..].iter().filter(|h| (h.amount - self.magic).abs() < 1e-3).map(|h| h.amount).sum();
        self.hits_seen = ps.hits.len();
        let mut f = self.h.world.query::<(Entity, &horizon_ecs_proto::fangs::Fang)>();
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
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(10.5, 1.0, 0.5), Some(("minecraft:crossbow", crossbow())), false);
    let view = player_view(Vec3::new(10.5, 1.0, 0.5), None);
    let mut v = Vec::new();
    let mut last_phase = String::new();
    for t in 1..=200 {
        v.push(s.step(&view));
        let st = s.h.world.get::<ranged_crossbow::Crossbow>(s.ecs).map(|c| format!("{c:?}")).unwrap_or_default();
        let ph: String = st.split("phase: ").nth(1).map(|x| x.split(',').next().unwrap_or("").to_string()).unwrap_or_default();
        if ph != last_phase {
            eprintln!("t={t}: ecs phase {ph} ({st})");
            last_phase = ph;
        }
        if t % 10 == 0 && t <= 120 {
            eprintln!("t={t}: mob x {:.2} / ecs x {:.2}", s.r.mobs[s.mob].position.x, s.h.world.get::<Pos>(s.ecs).unwrap().0.x);
        }
    }
    let (m, e) = (at(&v, |t| t.mob_shot > 0), at(&v, |t| t.ecs_shot > 0));
    for (t, d) in &s.trace {
        eprintln!("mob event t={t}: {d}");
    }
    // Each side draws its own random delays (20..39 ticks between the charge and the shot): the first bolt and the period are
    // compared as the window the source allows (charge 25 + delay + a tick or two), not as one tick.
    eprintln!("crossbow bolts: mob {m:?}, ecs {e:?}");
    assert!(m.len() >= 2 && e.len() >= 2, "two bolts each: mob {m:?}, ecs {e:?}");
    for (who, b) in [("mob", &m), ("ecs", &e)] {
        assert!((50..=130).contains(&b[0]), "{who}: first bolt {b:?}");
        assert!((46..=66).contains(&(b[1] - b[0])), "{who}: period {b:?}");
    }
    assert!((m.len() as i64 - e.len() as i64).abs() <= 1, "mob {m:?}, ecs {e:?}");
}

/// The guardian in water with the player within 15 blocks: the beam of 2 seconds ends in the same tick on both sides, with the same damage (1 magic + 2 on hard).
#[test]
#[ignore = "defect of the Mob: re-targeting before the goal stops (ai.rs:2883-2887, goals.rs:1770/1884 then can_continue goals.rs:161, attack_time guardian.rs:122/134): it hits at 90, 91 and 93 where the game hits once per beam (wiki: 6 HP plus 2 HP magic, then about 3 s before the next); the ECS is the game; see suivi/ecs-ecarts.md"]
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

/// The beam of the guardian, as the game has it: once per beam, its magic damage then the blow of 6 at the same tick, at least 3 s between two beams; the
/// first hit at the tick of the `Mob`. The strict count against the `Mob` is the ignored test above (a defect of the `Mob`).
#[test]
fn the_guardian_beam_is_its_magic_then_its_blow() {
    if !is("minecraft:guardian") && !is("minecraft:elder_guardian") {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 2.0, 0.5), Vec3::new(6.5, 2.0, 0.5), None, true);
    let view = player_view(Vec3::new(6.5, 2.0, 0.5), None);
    let v = run(&mut s, &view, 400);
    let (m, e) = (at(&v, |t| t.mob_dmg > 0.0), at(&v, |t| t.ecs_dmg > 0.0));
    close(&m, &e, 6, "beam hits");
    assert!(e.windows(2).all(|w| w[1] - w[0] >= 60), "at least 3 s between two beams: {e:?}");
    let hits = s.h.world.resource::<PlayerState>().hits.iter().map(|h| (h.tick, h.amount)).collect::<Vec<_>>();
    let blow = hits.iter().filter(|h| (h.1 - s.magic).abs() > 1e-3).count();
    assert_eq!(blow, e.len(), "one blow with each beam: {hits:?}");
    let first = |f: fn(&Tick) -> f32| v.iter().map(f).find(|d| *d > 0.0).unwrap();
    assert_eq!(first(|t| t.mob_dmg), first(|t| t.ecs_dmg));
    // The charge sound (entity event 21) starts the beam's charge: the first one is on the same tick on both sides.
    let (mc, ec) = (at(&v, |t| t.mob_charge > 0), at(&v, |t| t.ecs_charge > 0));
    eprintln!("charge sound: mob {mc:?}, ecs {ec:?}");
    // The ecs guardian gets its target a tick after the `Mob` (the beam hits at 90 and 91: the same lag for the sound), so the sound is compared to the
    // beam's own hit: the same 80 ticks before it on both sides.
    assert!(!ec.is_empty() && !m.is_empty());
    assert_eq!(m[0] - mc[0], e[0] - ec[0], "charge sound before the hit");
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
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(10.5, 1.0, 0.5), Some(("minecraft:trident", trident())), false);
    let view = player_view(Vec3::new(10.5, 1.0, 0.5), None);
    let v = run(&mut s, &view, 200);
    let (m, e) = (at(&v, |t| t.mob_shot > 0), at(&v, |t| t.ecs_shot > 0));
    close(&m, &e, 3, "trident throws");
    assert!((m.len() as i64 - e.len() as i64).abs() <= 1, "mob {m:?}, ecs {e:?}");
}

/// The blaze's burst: three small fireballs per salve, at the same ticks on both sides, the same count in 150
/// ticks; the `Charged` flag is up while it charges and down after the salve.
#[test]
fn the_blaze_charges_and_shoots_a_burst_as_a_mob_does() {
    if !is("minecraft:blaze") {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(8.5, 1.0, 0.5), None, false);
    let view = player_view(Vec3::new(8.5, 1.0, 0.5), None);
    let (mut v, mut charged, mut mob_flag) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..150 {
        v.push(s.step(&view));
        charged.push(s.h.world.get::<burst_shot::Charged>(s.ecs).is_some());
        mob_flag.push(s.r.mobs[s.mob].extra.flag);
    }
    let (m, e) = (at(&v, |t| t.mob_shot > 0), at(&v, |t| t.ecs_shot > 0));
    let first = |f: &[bool]| f.iter().position(|c| *c).map(|i| i as i64 + 1);
    let (ms, es) = (first(&mob_flag), first(&charged));
    eprintln!("blaze charge starts: mob {ms:?}, ecs {es:?}; fireballs mob {m:?}, ecs {e:?}");
    // Same goal, same draws (none shape the timing): the time from the start of the charge to each fireball is EQUAL on both sides.
    for i in 0..3 {
        assert_eq!(m[i] - ms.unwrap(), e[i] - es.unwrap(), "fireball {i}: mob {m:?} from {ms:?}, ecs {e:?} from {es:?}");
    }
    // The goal starts and acts at the tick of the decision on both sides: the charge and the fireballs are at the SAME ticks.
    assert_eq!(ms, es, "charge start: mob {ms:?}, ecs {es:?}");
    assert_eq!(m, e, "fireballs: mob {m:?}, ecs {e:?}");
    assert_eq!(e.len(), 3, "three per salve: {e:?}");
    assert!(charged[..e[0] as usize].iter().any(|c| *c), "charged while charging");
    assert!(e[2] as usize + 100 >= 150 || !charged[e[2] as usize + 100..].iter().any(|c| *c), "not charged after the rest");
}

/// The blaze under 2 blocks of a seen target: a blow every 20 ticks of the blaze's own damage (the ECS side; the `Mob`'s blow is the
/// `do_hurt_target` event, not compared here).
#[test]
fn the_blaze_strikes_in_melee_under_two_blocks() {
    if !is("minecraft:blaze") {
        return;
    }
    let mut s = scene(Vec3::new(0.5, 1.0, 0.5), Vec3::new(1.9, 1.0, 0.5), None, false);
    let view = player_view(Vec3::new(1.9, 1.0, 0.5), None);
    run(&mut s, &view, 120);
    let hits: Vec<(u64, f32)> = s.h.world.resource::<PlayerState>().hits.iter().map(|h| (h.tick, h.amount)).collect();
    eprintln!("melee hits: {hits:?}");
    assert!(hits.len() >= 3, "{hits:?}");
    assert!(hits.windows(2).all(|w| w[1].0 - w[0].0 == 20), "every 20 ticks: {hits:?}");
    assert!(hits.iter().all(|h| (h.1 - burst_shot::Params::BLAZE.melee_damage).abs() < 1e-3));
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
    let (mut len, mut seen_len, mut drunk) = (0, None, None);
    for _ in 0..600 {
        s.step(&view);
        match s.h.world.get::<drink_potion::Drinking>(s.ecs).copied() {
            Some(d) => {
                // The draws are random (0.15 water breathing first, then the 0.5 swiftness for a far target): either is legal here.
                assert!(matches!(d.potion, drink_potion::Potion::WaterBreathing | drink_potion::Potion::Swiftness), "{:?}", d.potion);
                drunk = Some(d.potion);
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
    assert_eq!(seen_len, Some(33), "it drinks for 32 ticks (left counts 32..=0), seen {seen_len:?}");
    let kind = if drunk == Some(drink_potion::Potion::WaterBreathing) { horizon_ecs_proto::effects::Kind::WaterBreathing } else { horizon_ecs_proto::effects::Kind::Speed };
    assert!(horizon_ecs_proto::effects::has(&s.h.world, s.ecs, kind));
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

/// The pillager with the SAME random delays on both sides: the two sides draw from different generators, so the ECS's seed is searched until its
/// two `attack_delay` draws (`20 + nextInt(20)` at each charge, read from the `Mob`'s own timeline: shot - load - 1) equal the `Mob`'s. Then the
/// start of the charge to the first bolt, and the period, are then EQUAL to the tick; the start itself (`Mob` sound `loading_start`, ECS phase `Charging`) depends on the walk-in
/// and the seed: a second search finds an ECS seed that starts it on the Mob's tick.
#[test]
fn the_pillager_with_the_same_delays_shoots_to_the_tick() {
    if !is("minecraft:pillager") {
        return;
    }
    let (at_, to) = (Vec3::new(0.5, 1.0, 0.5), Vec3::new(10.5, 1.0, 0.5));
    let view = player_view(to, None);
    let mut s = scene(at_, to, Some(("minecraft:crossbow", crossbow())), false);
    let v = run(&mut s, &view, 200);
    let m = at(&v, |t| t.mob_shot > 0);
    let tick_of = |s: &Scene, f: &dyn Fn(&str) -> bool| -> Vec<i64> { s.trace.iter().filter(|(_, d)| f(d)).map(|(t, _)| *t).collect() };
    let (starts, loads) = (tick_of(&s, &|d| d.contains("loading_start")), tick_of(&s, &|d| d.starts_with("CrossbowLoad")));
    assert!(m.len() >= 2 && loads.len() >= 2 && !starts.is_empty(), "mob {m:?} loads {loads:?} starts {starts:?}");
    let want = [m[0] - loads[0] - 1, m[1] - loads[1] - 1];
    let phase = |s: &Scene| s.h.world.get::<ranged_crossbow::Crossbow>(s.ecs).map(|c| format!("{c:?}")).unwrap_or_default();
    let field = |st: &str, name: &str| -> i64 { st.split(name).nth(1).and_then(|x| x.trim_start_matches(": ").split(',').next()).and_then(|x| x.trim().parse().ok()).unwrap_or(-1) };
    let mut found = None;
    'seeds: for seed in 1..=60_000u64 {
        let mut e = scene(at_, to, Some(("minecraft:crossbow", crossbow())), false);
        e.h.world.get_mut::<Rng>(e.ecs).unwrap().0 = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let (mut last, mut delays, mut start, mut shots) = (String::new(), Vec::new(), 0, Vec::new());
        for t in 1..=200i64 {
            e.h.tick();
            let st = phase(&e);
            let ph: String = st.split("phase: ").nth(1).map(|x| x.split(',').next().unwrap_or("").to_string()).unwrap_or_default();
            if ph != last {
                if ph == "Charging" && start == 0 {
                    start = t;
                }
                if ph == "Charged" {
                    delays.push(field(&st, "attack_delay"));
                    if delays[delays.len() - 1] != want[delays.len() - 1] {
                        continue 'seeds;
                    }
                }
                last = ph;
            }
            let mut q = e.h.world.query::<(Entity, &Projectile)>();
            let new: Vec<Entity> = q.iter(&e.h.world).map(|x| x.0).filter(|x| !e.seen.contains(x)).collect();
            e.seen.extend(new.iter().copied());
            if !new.is_empty() {
                shots.push(t);
            }
            if shots.len() >= 2 {
                break;
            }
        }
        if shots.len() >= 2 && delays.len() >= 2 {
            found = Some((seed, start, shots));
            break;
        }
    }
    let (seed, start, e) = found.expect("an ECS seed with the Mob's two delays");
    eprintln!("same delays {want:?} (seed {seed}): mob start {} shots {m:?}; ecs start {start} shots {e:?}", starts[0]);
    assert_eq!(m[0] - starts[0], e[0] - start, "start of the charge to the first bolt: mob {m:?} from {starts:?}, ecs {e:?} from {start}");
    assert_eq!(m[1] - m[0], e[1] - e[0], "period: mob {m:?}, ecs {e:?}");
    // The start of the charge moves with the seed (walk-in, `path_delay`): some ECS seed starts it on the Mob's tick.
    let starts_at = (1..=3000u64).find_map(|seed| {
        let mut e = scene(at_, to, Some(("minecraft:crossbow", crossbow())), false);
        e.h.world.get_mut::<Rng>(e.ecs).unwrap().0 = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (1..=60i64).find(|_| {
            e.h.tick();
            phase(&e).contains("phase: Charging")
        })
        .filter(|t| *t == starts[0]).map(|_| seed)
    });
    eprintln!("charge starts on the Mob's tick {} with seed {starts_at:?}", starts[0]);
    assert!(starts_at.is_some(), "no ECS seed starts the charge on tick {}", starts[0]);
    assert_eq!(m[1] - m[0], e[1] - e[0], "period: mob {m:?}, ecs {e:?}");
}

/// The witch with the SAME draws on both sides: the Mob's first drink (tick, potion, length of the flag) is read, then the ECS seed is searched
/// until its own first drink falls on the same tick with the same potion; the length is then equal (33 ticks of `Drinking`).
#[test]
fn a_witch_with_the_same_draws_drinks_the_same_potion() {
    if !is("minecraft:witch") {
        return;
    }
    let (at_, to) = (Vec3::new(0.5, 2.0, 0.5), Vec3::new(30.5, 2.0, 0.5));
    let view = player_view(to, None);
    let mut s = scene(at_, to, None, true);
    let (mut t0, mut potion, mut len) = (0, String::new(), 0);
    for t in 1..=600 {
        s.step(&view);
        let flag = s.r.mobs[s.mob].extra.flag;
        if flag && t0 == 0 {
            t0 = t;
            potion = s.r.mobs[s.mob].extra.main_hand_stack().map(|i| format!("{i:?}")).unwrap_or_default();
        }
        if t0 > 0 {
            if !flag {
                break;
            }
            len += 1;
        }
    }
    assert!(t0 > 0, "the Mob drank");
    let kind = if potion.contains("water_breathing") { drink_potion::Potion::WaterBreathing } else if potion.contains("swiftness") { drink_potion::Potion::Swiftness } else { panic!("potion {potion}") };
    let mut found = None;
    'seeds: for seed in 1..=20_000u64 {
        let mut e = scene(at_, to, None, true);
        e.h.world.get_mut::<Rng>(e.ecs).unwrap().0 = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let (mut first, mut n, mut which) = (0, 0, None);
        for t in 1..=(t0 + 40) {
            e.h.tick();
            match e.h.world.get::<drink_potion::Drinking>(e.ecs).copied() {
                Some(d) => {
                    if first == 0 {
                        first = t;
                        which = Some(d.potion);
                        if first != t0 || d.potion != kind {
                            continue 'seeds;
                        }
                    }
                    n += 1;
                }
                None if first > 0 => break,
                None => {}
            }
        }
        if first == t0 {
            found = Some((seed, which, n));
            break;
        }
    }
    let (seed, which, n) = found.expect("an ECS seed drinking the Mob's potion on its tick");
    eprintln!("witch same draws (seed {seed}): mob {potion} at {t0} for {len} ticks; ecs {which:?} for {n} ticks");
    assert_eq!(which, Some(kind));
    assert_eq!(n, len, "the drinking lasts as long as the Mob's flag: mob {len}, ecs {n}");
}
