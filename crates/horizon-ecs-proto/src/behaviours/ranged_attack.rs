//! `RangedAttackGoal` (fiche 32, golem de neige : `golem.rs:548-600`) : l'animal s'approche d'une cible qu'il voit, s'arrête
//! à `radius` une fois qu'il la voit depuis 5 ticks, la regarde, et tire toutes les `min..max` ticks. Seul le projectile
//! du golem de neige (boule de neige) est porté ici : la potion, le crachat et le crâne n'ont pas de projectile.
//! Réveil : une cible acquise (la fonction de `TargetWatchers`) ; le but ticque tant qu'il court (son marqueur).
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::projectile::{Flight, launch};
use crate::targeting::{self, Eye, Target, TargetWatchers, Who};
use horizon_core::geometry::{trig, Axis, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// La vitesse de la marche vers la cible, l'intervalle minimal et maximal entre deux tirs, la distance d'arrêt.
    pub speed: f64,
    pub min: i32,
    pub max: i32,
    pub radius: f32,
    /// Le projectile, sa vitesse initiale et son écart (`rangedAttackUncertainty`), le départ sous les yeux, la visée
    /// sous les yeux de la cible, et la hausse par bloc horizontal.
    pub flight: Flight,
    pub shot_speed: f64,
    pub uncertainty: f64,
    pub start_down: f64,
    pub aim_down: f64,
    pub lift: f64,
    /// A witch's throw: the potion is chosen by the target (`Witch.performRangedAttack`), the aim leads the target's motion, and the speed
    /// is 0.45 within 2 blocks, 0.75 beyond.
    pub witch: bool,
    /// The shot starts `(width + 1) * front` blocks in front of the body, along its yaw (the llama's spit, `llama.rs:165-171`: 0.5);
    /// 0: from the eyes' column.
    pub front: f64,
    /// The aim is at the bottom of the target's box plus this fraction of its height (the spit: 1/3, `llama.rs:172-176`); 0: at the target's
    /// eyes minus `aim_down`.
    pub aim_fraction: f64,
    /// The shooter is marked `DidShot` at each shot (what `LlamaHurtBy` reads: the llama's `flag2`).
    pub marks: bool,
    /// The sound of each shot (the wither's head: `entity.wither.shoot`, volume 2), `None`: none.
    pub sound: Option<&'static str>,
}
impl Params {
    /// Le golem de neige (`golem.rs:554-557`, `golem.rs:580`).
    pub const DEFAULT: Self = Self {
        speed: 1.25,
        min: 20,
        max: 20,
        radius: 10.0,
        flight: Flight::SNOWBALL,
        shot_speed: 1.6,
        uncertainty: 12.0,
        start_down: 0.1,
        aim_down: 1.1,
        lift: 0.2,
        witch: false,
        front: 0.0,
        aim_fraction: 0.0,
        marks: false,
        sound: None,
    };
    /// The llama's spit (`llama.rs:67`, `llama.rs:159-189`): 40 ticks, 20 blocks, speed 1.5, spread 10, from in front of the body, aimed at a third of the box.
    pub const LLAMA: Self = Self { speed: 1.25, min: 40, max: 40, radius: 20.0, flight: Flight::SPIT, shot_speed: 1.5, uncertainty: 10.0, start_down: 0.1, aim_down: 0.0, lift: 0.2, witch: false, front: 0.5, aim_fraction: 1.0 / 3.0, marks: true, sound: None };
    /// The witch (`witch.rs:20-24`): the same goal at 60 ticks, 10 blocks, a thrown potion.
    pub const WITCH: Self = Self { speed: 1.0, min: 60, max: 60, radius: 10.0, flight: Flight::POTION, shot_speed: 0.75, uncertainty: 8.0, start_down: 0.1, aim_down: 1.1, lift: 0.2, witch: true, front: 0.0, aim_fraction: 0.0, marks: false, sound: None };
}
/// L'état : le rang de l'entrée, les paramètres, les compteurs du `Mob` et la case où le chemin a été demandé.
#[derive(Component, Clone, Copy, Debug)]
pub struct Shooter {
    p: Params,
    slot: u8,
    attack_time: i32,
    see_time: i32,
    cell: (i32, i32),
}
/// Le but tourne.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Shooting;
/// A shot was made (`Params.marks`): the llama's `flag2`, read by the variant of `hurt_by` of lot B.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct DidShot;

pub static MODULE: Module = Module {
    name: "ranged_attack",
    flags: MOVE | LOOK,
    resources,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    retry_on_end: true,
    requires: &[Cap::Body, Cap::Goals, Cap::Targets, Cap::Locomotion, Cap::Head],
    ..Module::DEFAULT
};
fn resources(w: &mut World) {
    w.resource_mut::<TargetWatchers>().0.push(on_target);
}
fn on_target(w: &mut World, e: Entity) {
    if let Some(s) = w.get::<Shooter>(e) {
        let slot = s.slot;
        w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
    }
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Shooter { p, slot: inst.slot, attack_time: -1, see_time: 0, cell: (i32::MIN, 0) });
}
/// The tick of the module: its system, run by `arbitrate` while an animal carries it (no system in the schedule).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, shoot_run);
}
/// La cible est vivante (une créature sans vie ne l'est pas ; un joueur mort n'est pas lu par `targeting::info`).
fn alive(hp: &Query<&Health>, who: Who) -> bool {
    match who {
        Who::Entity(c) => hp.get(c).map_or(true, |h| h.0 > 0.0),
        Who::Player(_) => true,
    }
}
/// `canUse` : une cible vivante.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Target>(e).is_some_and(|t| targeting::info(w, t.who).is_some())
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Shooting);
}
/// `stop` du `Mob` : la vue et le compte à rebours sont remis à zéro (la marche n'est pas arrêtée ici).
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Shooting>();
    if let Some(mut s) = w.get_mut::<Shooter>(e) {
        s.see_time = 0;
        s.attack_time = -1;
    }
}
/// `RangedAttackGoal.tick`, à chaque tick du but. Fin : pas de cible vivante.
#[allow(clippy::type_complexity)]
fn shoot_run(
    tg: targeting::Targets,
    hp: Query<&Health>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
    mut q: Query<(crate::goals::Aim, &mut Shooter, Option<&Target>), With<Shooting>>,
) {
    for ((e, pos, mut look, looking, following, asked), mut s, target) in &mut q {
        let Some(t) = target else {
            reqs.0.push(Req::End(e, s.slot));
            continue;
        };
        let Some(i) = tg.info(t.who).filter(|_| alive(&hp, t.who)) else {
            reqs.0.push(Req::End(e, s.slot));
            continue;
        };
        let d = d2(pos.0, i.pos);
        s.see_time = if t.seen { s.see_time + 1 } else { 0 };
        let r = s.p.radius;
        if d <= f64::from(r * r) && s.see_time >= 5 {
            cmd.queue(move |w: &mut World| stop_nav(w, e));
        } else {
            let cell = (i.pos.x.floor() as i32, i.pos.z.floor() as i32);
            if cell != s.cell || (!following && !asked) {
                s.cell = cell;
                let (at, speed) = ([i.pos.x, i.pos.y, i.pos.z], s.p.speed);
                cmd.queue(move |w: &mut World| go_to_at(w, e, at, speed, 1));
            }
        }
        aim_clamped(&mut cmd, e, &mut look, looking, [i.pos.x, i.eye, i.pos.z], Some(30.0), Some(30.0));
        s.attack_time -= 1;
        if s.attack_time == 0 {
            if t.seen {
                let dist = (d.sqrt() as f32) / r;
                let who = t.who;
                let p = s.p;
                cmd.queue(move |w: &mut World| shoot(w, e, who, p));
                s.attack_time = (dist * (s.p.max - s.p.min) as f32 + s.p.min as f32).floor() as i32;
            }
        } else if s.attack_time < 0 {
            let f = d.sqrt() / f64::from(r);
            s.attack_time = (f64::from(s.p.min) + f * f64::from(s.p.max - s.p.min)).floor() as i32;
        }
    }
}
/// `snowball_attack` : le départ sous les yeux, la visée à la hauteur des yeux de la cible moins 1,1 et 0,2 par bloc
/// horizontal, puis l'écart triangulaire de `Projectile.shoot`.
fn shoot(w: &mut World, e: Entity, who: Who, p: Params) {
    let Some(i) = targeting::info(w, who) else { return };
    let (Some(pos), Some(eye)) = (w.get::<Pos>(e).map(|x| x.0), w.get::<Eye>(e).map(|x| x.0)) else { return };
    let start = if p.front > 0.0 {
        let (Some(width), Some(yaw)) = (w.get::<Body>(e).map(|b| f64::from(b.width)), w.get::<Yaw>(e).map(|y| f64::from(y.0))) else { return };
        let (yaw, reach) = (yaw.to_radians(), (width + 1.0) * p.front);
        Vec3::new(pos.x - reach * f64::from(trig::sin(yaw)), pos.y + eye - p.start_down, pos.z + reach * f64::from(trig::cos(yaw)))
    } else {
        Vec3::new(pos.x, pos.y + eye - p.start_down, pos.z)
    };
    let motion = match who {
        Who::Entity(t) if p.witch => w.get::<Vel>(t).map_or(Vec3::ZERO, |v| v.0),
        _ => Vec3::ZERO,
    };
    let (xd, zd) = (i.pos.x + motion.x - pos.x, i.pos.z + motion.z - pos.z);
    let (mut p, dist) = (p, (xd * xd + zd * zd).sqrt());
    if p.witch {
        let health = match who {
            Who::Entity(t) => w.get::<Health>(t).map_or(20.0, |h| h.0),
            Who::Player(_) => 20.0,
        };
        use crate::effects::{Brew, Kind};
        let has = |w: &World, kind| match who {
            Who::Entity(t) => crate::effects::has(w, t, kind),
            Who::Player(id) => crate::effects::player_has(w, id, kind),
        };
        let (slow, poison, weak) = (has(w, Kind::Slowness), has(w, Kind::Poison), has(w, Kind::Weakness));
        static SLOW: [Brew; 1] = [Brew::Timed(Kind::Slowness, 0, 1800)];
        static POISON: [Brew; 1] = [Brew::Timed(Kind::Poison, 0, 900)];
        static WEAK: [Brew; 1] = [Brew::Timed(Kind::Weakness, 0, 1800)];
        static HARM: [Brew; 1] = [Brew::Harm(0)];
        // `Witch.performRangedAttack`: slowness from afar, poison on the healthy, weakness up close (a quarter of the time), else harming.
        let brews: &'static [Brew] = if dist >= 8.0 && !slow {
            &SLOW
        } else if health >= 8.0 && !poison {
            &POISON
        } else if dist <= 3.0 && !weak && w.get_mut::<Rng>(e).unwrap().next_float() < 0.25 {
            &WEAK
        } else {
            &HARM
        };
        p.flight.hit = crate::projectile::OnHit::Splash(brews);
        p.shot_speed = if dist <= 2.0 { 0.45 } else { 0.75 };
    }
    let p = p;
    let yd = if p.aim_fraction > 0.0 {
        let bottom = i.bounds.min(Axis::Y);
        bottom + (i.bounds.max(Axis::Y) - bottom) * p.aim_fraction
    } else {
        i.eye - p.aim_down
    };
    let yo = (xd * xd + zd * zd).sqrt() * p.lift;
    let dir = Vec3::new(xd, yd + yo - start.y, zd).normalize();
    let spread = 0.017_227_5 * p.uncertainty;
    let mut rng = w.get_mut::<Rng>(e).unwrap();
    let mut triangle = |mean: f64, spread: f64| mean + spread * (rng.next_f64() - rng.next_f64());
    let v = Vec3::new((dir.x + triangle(0.0, spread)) * p.shot_speed, (dir.y + triangle(0.0, spread)) * p.shot_speed, (dir.z + triangle(0.0, spread)) * p.shot_speed);
    if let Some(name) = p.sound {
        w.resource_mut::<Events>().host.sound(name, [pos.x, pos.y, pos.z], 2.0, 1.0);
    }
    launch(w, p.flight, start, v, Some(Who::Entity(e)));
    if p.marks {
        w.entity_mut(e).insert(DidShot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::projectile::Projectile;
    use crate::species::Config;
    use crate::targeting::{clear_target, set_target};
    use std::collections::HashMap;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 1, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// Un zombie avec ce seul but, et une vache (sans buts) à 5 blocs, cible vue.
    fn world() -> Option<(Herd, Entity, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        sp.goals = config();
        let mut cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        cow.goals = Config { entries: &[] };
        let golem = spawn_species(&mut h.world, &sp, 0.5, 0.5, 51);
        let prey = spawn_species(&mut h.world, &cow, 5.5, 0.5, 52);
        for e in [golem, prey] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        h.tick();
        set_target(&mut h.world, golem, Target::new(Who::Entity(prey), 200, u8::MAX));
        Some((h, golem, prey))
    }
    /// Projectiles vus, avec le tick de leur première apparition.
    fn projectiles(h: &mut Herd, first: &mut HashMap<Entity, usize>, tick: usize) {
        let ids: Vec<Entity> = h.world.query_filtered::<Entity, With<Projectile>>().iter(&h.world).collect();
        for id in ids {
            first.entry(id).or_insert(tick);
        }
    }

    /// Départ et tirs : des boules à 20 ticks d'intervalle, pendant que la cible est vue ; arrêt quand elle est perdue.
    #[test]
    fn it_fires_a_snowball_every_twenty_ticks_and_stops_when_the_target_is_lost() {
        let Some((mut h, golem, _)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let mut first = HashMap::new();
        for t in 0..80 {
            h.tick();
            projectiles(&mut h, &mut first, t);
        }
        assert!(running(&h, golem), "témoin : le but tire");
        let mut ticks: Vec<usize> = first.values().copied().collect();
        ticks.sort_unstable();
        assert!(ticks.len() >= 3, "au moins trois boules en 80 ticks, {ticks:?}");
        assert!(ticks.windows(2).all(|w| w[1] - w[0] == 20), "intervalle de 20 ticks, {ticks:?}");
        clear_target(&mut h.world, golem);
        for _ in 0..6 {
            h.tick();
        }
        assert!(!running(&h, golem), "arrêt quand la cible est perdue");
    }

    /// The llama's spit through the same goal (`Params::LLAMA`): one spit every 40 ticks while the target is seen, `DidShot` set, stop when the target is lost.
    #[test]
    fn the_llama_spits_every_forty_ticks_and_stops_when_the_target_is_lost() {
        use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
        use crate::projectile::Projectile;
        use crate::targeting::{clear_target, set_target};
        use crate::species::Config;
        use crate::behaviours::Entry;
        std::env::var_os("HORIZON_TEST_RESOURCES").map(|_| ()).unwrap_or(());
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        }
        let mut h = Herd::with_slab(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 1, &Params::LLAMA)]));
        sp.goals = Config { entries };
        let mut cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        cow.goals = Config { entries: &[] };
        let spitter = spawn_species(&mut h.world, &sp, 0.5, 0.5, 51);
        let prey = spawn_species(&mut h.world, &cow, 12.5, 0.5, 52);
        for e in [spitter, prey] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        h.tick();
        set_target(&mut h.world, spitter, Target::new(Who::Entity(prey), 200, u8::MAX));
        let mut first = std::collections::HashMap::new();
        for t in 0..200 {
            h.tick();
            let ids: Vec<Entity> = h.world.query_filtered::<Entity, With<Projectile>>().iter(&h.world).collect();
            for id in ids {
                first.entry(id).or_insert(t);
            }
        }
        let mut ticks: Vec<usize> = first.values().copied().collect();
        ticks.sort_unstable();
        assert!(ticks.len() >= 3, "at least three spits in 200 ticks, {ticks:?}");
        assert!(ticks.windows(2).all(|w| w[1] - w[0] == 40), "an interval of 40 ticks, {ticks:?}");
        assert!(h.world.get::<DidShot>(spitter).is_some(), "witness: the spit sets DidShot");
        clear_target(&mut h.world, spitter);
        for _ in 0..6 {
            h.tick();
        }
        assert!(!h.world.get::<GoalSet>(spitter).unwrap().is_running(&MODULE), "stops when the target is lost");
    }
}
