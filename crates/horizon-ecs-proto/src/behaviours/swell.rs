//! `Swell` (fiche 12) : l'animal gonfle quand sa cible est à moins de 3 blocs (carré < 9) ; il reste gonflé tant que la
//! cible est vivante, à 7 blocs au plus (carré <= 49) et vue. `SwellDir` donne le sens (+1 gonfle, -1 dégonfle) ; la
//! règle d'explosion qui le lit n'est pas dans ce module. Pas de tirage, pas d'échéance : le but est réveillé par la
//! distance de la cible, examinée seulement pour les animaux qui ont une cible et portent `SwellDir`.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::targeting::{self, Target, Who};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The swell at which it explodes (`maxSwell`), and the radius of the blast (`explosionRadius`, doubled when charged).
    pub max: i32,
    pub radius: f32,
}
impl Params {
    pub const DEFAULT: Self = Self { max: 30, radius: 3.0 };
    /// The goal without a blast (the swell direction only).
    pub const NO_BLAST: Self = Self { max: 0, radius: 0.0 };
}

/// Le sens du gonflement : +1 gonfle, -1 dégonfle ; -1 à la création.
#[derive(Component, Clone, Copy, Debug)]
pub struct SwellDir(pub i8);
/// Le but tourne ; il retient la cible prise au départ (comme le `Mob`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Swelling {
    pub who: Who,
}

pub static MODULE: Module = Module {
    name: "swell",
    flags: MOVE,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Targets, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(SwellDir(-1));
    if p.max > 0 {
        w.entity_mut(e).insert(crate::explosion::Fuse { value: 0, at: 0, dir: -1, max: p.max, radius: p.radius, armed: 0, ignited: false });
    }
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((swell_run, swell_watch).chain().in_set(slot));
}
/// La cible est-elle vivante ? (un joueur mort n'est pas lu par `targeting::info` ; une créature sans vie non plus.)
fn alive(w: &World, who: Who) -> Option<targeting::Info> {
    let i = targeting::info(w, who)?;
    match who {
        Who::Entity(c) => w.get::<Health>(c).is_none_or(|h| h.0 > 0.0).then_some(i),
        Who::Player(_) => Some(i),
    }
}
/// Vivante et à moins de 3 blocs.
fn close(w: &World, e: Entity, who: Who) -> bool {
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    alive(w, who).is_some_and(|i| d2(p, i.pos) < 9.0)
}
/// `Swell.tick` : +1 si la cible est vivante, à 7 blocs au plus et vue (la vue de la cible courante), sinon -1.
fn reading(w: &World, e: Entity, who: Who) -> i8 {
    let (Some(p), Some(i)) = (w.get::<Pos>(e).map(|p| p.0), alive(w, who)) else { return -1 };
    let seen = w.get::<Target>(e).is_some_and(|t| t.who == who && t.seen);
    if d2(p, i.pos) <= 49.0 && seen { 1 } else { -1 }
}
/// `canUse` : le sens est +1 (le gonflement continue) ou la cible est proche.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(who) = w.get::<Target>(e).map(|t| t.who) else { return false };
    w.get::<SwellDir>(e).is_some_and(|d| d.0 > 0) || close(w, e, who)
}
/// `start` : arrête la navigation, retient la cible, et lit le sens tout de suite (le `Mob` ticque le but le même tick).
fn start(w: &mut World, e: Entity, _: Inst) {
    stop_nav(w, e);
    let Some(who) = w.get::<Target>(e).map(|t| t.who) else { return };
    w.entity_mut(e).insert(Swelling { who });
    let d = reading(w, e, who);
    if let Some(mut s) = w.get_mut::<SwellDir>(e) {
        s.0 = d;
    }
    crate::explosion::set_dir(w, e, d);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Swelling>();
}
/// `canContinueToUse` (le sens du dernier tick, ou la cible proche maintenant) puis `tick`, pour les animaux qui gonflent.
fn swell_run(w: &mut World, q: &mut QueryState<(Entity, &Swelling)>) {
    let runs: Vec<(Entity, Who)> = q.iter(w).map(|(e, s)| (e, s.who)).collect();
    for (e, who) in runs {
        let dir = w.get::<SwellDir>(e).map_or(-1, |d| d.0);
        if dir <= 0 && !close(w, e, who) {
            if let Some(slot) = slot_of(w, e, &MODULE) {
                w.resource_mut::<Requests>().0.push(Req::End(e, slot));
            }
            continue;
        }
        let d = reading(w, e, who);
        if let Some(mut s) = w.get_mut::<SwellDir>(e) {
            s.0 = d;
        }
        crate::explosion::set_dir(w, e, d);
    }
}
/// `canUse` des animaux qui ne gonflent pas et ont une cible : une cible à moins de 3 blocs demande le départ (le
/// sélecteur le prend à son tour de phase).
fn swell_watch(w: &mut World, q: &mut QueryState<(Entity, &Target), (With<SwellDir>, Without<Swelling>)>) {
    let cands: Vec<(Entity, Who)> = q.iter(w).map(|(e, t)| (e, t.who)).collect();
    for (e, who) in cands {
        if close(w, e, who)
            && let Some(slot) = slot_of(w, e, &MODULE)
        {
            w.resource_mut::<Requests>().0.push(Req::Start(e, slot));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, Pos, SpeciesTable, spawn_species};
    use crate::species::Config;
    use crate::targeting::{Target, set_target};
    use horizon_core::geometry::Vec3;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 2, &Params::NO_BLAST)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    fn dir(h: &Herd, e: Entity) -> i8 {
        h.world.get::<SwellDir>(e).unwrap().0
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
    /// Un animal de l'espèce « zombie » avec ce seul but, et une vache (sans buts) comme cible, à 4 blocs.
    fn world() -> Option<(Herd, Entity, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        sp.goals = config();
        let mut cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        cow.goals = Config { entries: &[] };
        let creeper = spawn_species(&mut h.world, &sp, 0.5, 0.5, 31);
        let prey = spawn_species(&mut h.world, &cow, 4.5, 0.5, 32);
        for e in [creeper, prey] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        h.tick();
        let y = h.world.get::<Pos>(creeper).unwrap().0.y;
        set_target(&mut h.world, creeper, Target::new(Who::Entity(prey), 200, u8::MAX));
        h.teleport(prey, Vec3::new(4.5, y, 0.5));
        Some((h, creeper, prey))
    }
    fn move_prey(h: &mut Herd, prey: Entity, dx: f64) {
        let y = h.world.get::<Pos>(prey).unwrap().0.y;
        h.teleport(prey, Vec3::new(0.5 + dx, y, 0.5));
    }

    /// Départ à moins de 3 blocs (et pas à 4), poursuite à 5 blocs, arrêt à 9 blocs.
    #[test]
    fn it_swells_within_three_blocks_goes_on_to_seven_and_stops_beyond() {
        let Some((mut h, creeper, prey)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        ticks(&mut h, 6);
        assert!(!running(&h, creeper), "pas de départ à 4 blocs");
        assert_eq!(dir(&h, creeper), -1);
        move_prey(&mut h, prey, 2.0);
        ticks(&mut h, 6);
        assert!(running(&h, creeper), "départ à 2 blocs");
        assert_eq!(dir(&h, creeper), 1);
        move_prey(&mut h, prey, 5.0);
        for _ in 0..10 {
            h.tick();
            assert!(running(&h, creeper) && dir(&h, creeper) == 1, "poursuite à 5 blocs");
        }
        move_prey(&mut h, prey, 9.0);
        ticks(&mut h, 6);
        assert!(!running(&h, creeper), "arrêt au-delà de 7 blocs");
        assert_eq!(dir(&h, creeper), -1);
    }
}
