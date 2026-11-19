//! Une même scène des deux côtés, villageois adulte seul : le coucher à 12000 et l'arrivée au lit, le réveil à 2000 et le départ
//! vers le poste, l'arrivée à la cloche à l'heure de rencontre (9000). `Mob` : son heure du monde (`MobWorld.day_time`) est réglée
//! à chaque tick, ses lit et poste sont posés dans le monde du `Mob`. ECS : `Times` à chaque tick, mémoires d'emplacement posées
//! à la main (les capteurs qui les trouvent ne sont pas portés ici). Tolérance : 40 ticks, à confirmer par un lancement.
use crate::common;
use bevy_ecs::prelude::*;
use common::*;
use horizon_client_lib::mobs::{villager, Mob, MobWorld};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::behaviours::{bed::Sleeping, schedule::Times};
use horizon_ecs_proto::species::villager::{JOB_SITE, MEETING_POINT};
use horizon_ecs_proto::{brain::*, cow::*, goals::Entered};

const BED: [i32; 3] = [4, 1, 0];
const JOB: [i32; 3] = [-8, 1, 0];
const BELL: [i32; 3] = [0, 1, 8];
const SPAWN: (f64, f64) = (0.5, 0.5);

fn skip() -> bool {
    !have_data() || std::env::var("HORIZON_SPECIES").as_deref() != Ok("minecraft:villager")
}
fn centre(p: [i32; 3]) -> (f64, f64) {
    (f64::from(p[0]) + 0.5, f64::from(p[2]) + 0.5)
}
fn near(at: (f64, f64), p: [i32; 3], d: f64) -> bool {
    let (x, z) = centre(p);
    (at.0 - x).powi(2) + (at.1 - z).powi(2) <= d * d
}
/// Le tick de chaque événement de la scène (`None` : pas arrivé dans la fenêtre).
#[derive(Debug, PartialEq)]
struct Marks {
    sleep: Option<usize>,
    bed: Option<usize>,
    wake: Option<usize>,
    work: Option<usize>,
    bell: Option<usize>,
}
/// Une fenêtre de jour : `ticks` ticks à partir de `day` (heure du jour), relevés à chaque tick.
fn window(day: i64, ticks: usize) -> impl Iterator<Item = i64> {
    (0..ticks).map(move |k| (day + k as i64).rem_euclid(24000))
}
fn mob_side(days: &[i64]) -> Vec<((f64, f64), bool)> {
    let mut r = MobRef::new(60, false);
    r.set_block(BED[0], BED[1], BED[2], "minecraft:red_bed");
    r.set_block(JOB[0], JOB[1], JOB[2], "minecraft:composter");
    r.set_block(BELL[0], BELL[1], BELL[2], "minecraft:bell");
    r.apply_block_events();
    let i = r.add_kind(Vec3::new(SPAWN.0, 1.0, SPAWN.1), mob_kind("minecraft:villager"));
    let mut out = Vec::with_capacity(days.len());
    for &day in days {
        // Le monde du `Mob` est reconstruit à chaque tick pour porter l'heure (il est gelé sinon) ; `MobWorld` est copié, la
        // fuite est d'un petit objet par tick.
        r.w = Box::leak(Box::new(MobWorld { day_time: day, ..r.w.clone() }));
        r.step(None);
        let m: &Mob = &r.mobs[i];
        out.push(((m.position.x, m.position.z), villager::sleeping(m)));
    }
    out
}
fn ecs_side(days: &[i64]) -> Vec<((f64, f64), bool)> {
    let mut h = Herd::with_slab(60, true, false);
    let table = h.world.resource::<SpeciesTable>().0.clone();
    let sp = *table.iter().find(|s| s.name == "minecraft:villager").unwrap();
    let e = spawn_species(&mut h.world, &sp, SPAWN.0, SPAWN.1, 1);
    h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
    h.world.resource_mut::<Entered>().0.push(e);
    set(&mut h.world, e, &horizon_ecs_proto::behaviours::bed::NEAREST_BED, MemValue::Block(BED), None);
    set(&mut h.world, e, &JOB_SITE, MemValue::Block(JOB), None);
    set(&mut h.world, e, &MEETING_POINT, MemValue::Block(BELL), None);
    let mut out = Vec::with_capacity(days.len());
    for &day in days {
        Times::set(&mut h.world, day);
        h.tick();
        let at = h.world.get::<Pos>(e).unwrap().0;
        out.push(((at.x, at.z), h.world.get::<Sleeping>(e).is_some()));
    }
    out
}
fn marks(run: &[((f64, f64), bool)], days: &[i64]) -> Marks {
    let first = |f: &dyn Fn(usize) -> bool| (0..run.len()).find(|&k| f(k));
    let sleep = first(&|k| run[k].1);
    let bed = first(&|k| near(run[k].0, BED, 1.5));
    let wake = sleep.and_then(|s| (s..run.len()).find(|&k| !run[k].1));
    let work = first(&|k| days[k] >= 2000 && near(run[k].0, JOB, 9.0));
    let bell = first(&|k| days[k] >= 9000 && near(run[k].0, BELL, 6.0));
    Marks { sleep, bed, wake, work, bell }
}

#[test]
fn bedtime_departure_and_meeting_match_the_mob() {
    if skip() {
        return;
    }
    // Trois fenêtres d'une même partie : coucher (11960 à 12600), départ (1960 à 2600), rencontre (8960 à 9600).
    let mut days: Vec<i64> = window(11_960, 640).collect();
    days.extend(window(1_960, 640));
    days.extend(window(8_960, 640));
    let (mob, ecs) = (mob_side(&days), ecs_side(&days));
    let (m, e) = (marks(&mob, &days), marks(&ecs, &days));
    for (name, a, b) in [("sleep", m.sleep, e.sleep), ("bed", m.bed, e.bed), ("wake", m.wake, e.wake), ("work", m.work, e.work), ("bell", m.bell, e.bell)] {
        let (a, b) = (a.map(|k| k as i64), b.map(|k| k as i64));
        match (a, b) {
            (Some(a), Some(b)) => assert!((a - b).abs() <= 40, "{name}: mob {a}, ecs {b}"),
            _ => panic!("{name}: mob {a:?}, ecs {b:?} (un côté n'arrive jamais)"),
        }
    }
}
