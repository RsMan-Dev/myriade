//! `LeapAtTargetGoal` (fiche 08 ; araignée et araignée des cavernes, `goals.rs:1669-1719`) : cible à 2 à 4 blocs (carré de
//! distance dans [4, 16]), animal au sol : il bondit vers la position mémorisée de la cible, avec une chance sur trois à
//! chaque demande (une demande tous les deux ticks du sélecteur). La chance n'est pas retirée à chaque tick : elle est
//! tirée une fois, à l'acquisition de la cible, comme une attente géométrique (`F` échecs avant le succès, délai
//! `premier + 2F` ticks). Le but finit à l'atterrissage (`Events.landed`, `!onGround` du `Mob`).
//! LIMITE (pas portée, cadre à demander) : la distance n'est pas un événement ; une cible qui entre dans la portée après
//! son acquisition ne réveille pas le but, et une attente échue hors portée n'est pas redemandée.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::targeting::{self, Target, TargetWatchers};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// La vitesse verticale du bond (0,4 pour l'araignée, `hostile.rs:47`).
    pub yd: f32,
}
impl Params {
    pub const DEFAULT: Self = Self { yd: 0.4 };
}
/// L'état : la vitesse verticale, le rang, la position mémorisée de la cible, et l'échéance de l'attente tirée
/// (`None` : aucune attente en cours).
#[derive(Component, Clone, Copy, Debug)]
pub struct Leap {
    yd: f32,
    slot: u8,
    target: [f64; 3],
    due: Option<u64>,
}
/// Le but tourne : l'animal est en l'air.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Leaping;

pub static MODULE: Module = Module {
    name: "leap_at_target",
    flags: JUMP | MOVE,
    resources,
    equip,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Targets],
    ..Module::DEFAULT
};
fn resources(w: &mut World) {
    w.resource_mut::<TargetWatchers>().0.push(on_target);
}
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let yd = params::<Params>(p).yd;
    w.entity_mut(e).insert(Leap { yd, slot: inst.slot, target: [0.0; 3], due: None });
}
/// The tick of the module: its system, run by `arbitrate` while an animal carries it (no system in the schedule).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, land);
}
/// La position de la cible si elle est à 2 à 4 blocs (carré de distance dans [4, 16]).
fn in_range(w: &World, e: Entity) -> Option<[f64; 3]> {
    let who = w.get::<Target>(e)?.who;
    let i = targeting::info(w, who)?;
    let p = w.get::<Pos>(e)?.0;
    let d = d2(p, i.pos);
    (4.0..=16.0).contains(&d).then_some([i.pos.x, i.pos.y, i.pos.z])
}
/// La portée, et l'animal au sol (`!onGround` du `canUse`).
fn reach(w: &World, e: Entity) -> Option<[f64; 3]> {
    w.get::<Contact>(e)?.on_ground.then_some(())?;
    in_range(w, e)
}
/// Une cible acquise à portée : tirer la chance une fois (attente géométrique de paramètre 1/3). Le sol n'est pas testé ici
/// (il est inconnu à l'apparition) ; `can_use` le teste à l'échéance.
fn on_target(w: &mut World, e: Entity) {
    let Some(slot) = w.get::<Leap>(e).map(|l| l.slot) else { return };
    if w.get::<Leaping>(e).is_some() || in_range(w, e).is_none() {
        return;
    }
    draw(w, e, slot);
}
/// The chance of the next ask: a geometric wait (1 in 3 per ask), the first ask at the next selector tick or two ticks later.
fn draw(w: &mut World, e: Entity, slot: u8) {
    let (now, phase) = (now_of(w, e), phase_of(w, e));
    let failures = with_rng(w, e, |r| geometric(r, 3)).max(0) as u64;
    // Les demandes tombent aux ticks de phase (`now + phase` pair) : la première est au tick suivant ou dans deux ticks.
    let first = if (now + 1 + phase) % 2 == 0 { 1 } else { 2 };
    let wait = first + 2 * failures;
    if let Some(mut l) = w.get_mut::<Leap>(e) {
        l.due = Some(now + wait);
    }
    crate::goals::arm(w, e, slot, wait as i32);
}
/// `canUse` à l'échéance tirée : la position est mémorisée, puis la portée doit tenir ; une échéance antérieure à la
/// dernière acquisition ne compte pas.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(due) = w.get::<Leap>(e).and_then(|l| l.due) else { return false };
    if now_of(w, e) < due {
        return false;
    }
    if let Some(mut l) = w.get_mut::<Leap>(e) {
        l.due = None;
    }
    let Some(at) = reach(w, e) else { return false };
    if let Some(mut l) = w.get_mut::<Leap>(e) {
        l.target = at;
    }
    true
}
/// `start` : la vitesse horizontale vaut 0,4 dans la direction de la cible plus 0,2 de la vitesse actuelle (la direction
/// brute si la cible est sur l'animal), et la vitesse verticale `yd` ; la vitesse est remplacée.
fn start(w: &mut World, e: Entity, _: Inst) {
    let Some(l) = w.get::<Leap>(e).copied() else { return };
    let (Some(p), Some(v)) = (w.get::<Pos>(e).map(|x| x.0), w.get::<Vel>(e).map(|x| x.0)) else { return };
    let (mut dx, mut dz) = (l.target[0] - p.x, l.target[2] - p.z);
    let len_sq = dx * dx + dz * dz;
    if len_sq > 1.0e-7 {
        let len = len_sq.sqrt();
        dx = dx / len * 0.4 + v.x * 0.2;
        dz = dz / len * 0.4 + v.z * 0.2;
    }
    set_velocity(w, e, Vec3::new(dx, f64::from(l.yd), dz));
    w.entity_mut(e).insert(Leaping);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Leaping>();
    // The Mob asks again at every selector tick: a target still held and still in range (a prey that runs) is leapt at again.
    // Controlled in `lot2_vs_mob` (1512 leaps in 300 ticks for 90 animals on the Mob side against 90 before this line).
    if w.get::<Target>(e).is_some()
        && let Some(slot) = w.get::<Leap>(e).map(|l| l.slot)
    {
        draw(w, e, slot);
    }
}
/// `canContinueToUse` = `!onGround` : le but finit au tick où l'animal atterrit (l'événement, lu au tick suivant).
fn land(ev: Res<Events>, q: Query<&Leap, With<Leaping>>, mut reqs: ResMut<Requests>) {
    for e in ev.landed.iter() {
        if let Ok(l) = q.get(e) {
            reqs.0.push(Req::End(e, l.slot));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, Pos, SpeciesTable, spawn_species};
    use crate::species::Config;
    use crate::targeting::{Who, set_target};
    use horizon_core::geometry::Vec3 as V;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 3, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// Un zombie avec ce seul but, et une vache (sans buts) à `dist` blocs sur x, cible acquise.
    fn world(dist: f64) -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        sp.goals = config();
        let mut cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        cow.goals = Config { entries: &[] };
        let spider = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
        let prey = spawn_species(&mut h.world, &cow, 0.5 + dist, 0.5, 62);
        for e in [spider, prey] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        // Le sol d'un animal apparu est inconnu au premier tick : on le laisse se poser avant de le faire viser.
        for _ in 0..5 {
            h.tick();
        }
        let y = h.world.get::<Pos>(spider).unwrap().0.y;
        h.teleport(prey, V::new(0.5 + dist, y, 0.5));
        set_target(&mut h.world, spider, Target::new(Who::Entity(prey), 200, u8::MAX));
        Some((h, spider))
    }

    /// Un bond à 3 blocs (le but tourne, puis finit à l'atterrissage) ; jamais à 1 bloc (carré 1, hors de [4, 16]).
    #[test]
    fn it_leaps_two_to_four_blocks_away_and_ends_on_landing() {
        let Some((mut h, spider)) = world(3.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let mut leapt = false;
        for _ in 0..200 {
            h.tick();
            leapt |= running(&h, spider);
        }
        assert!(leapt, "témoin : un bond à 3 blocs dans 200 ticks");
        for _ in 0..100 {
            h.tick();
        }
        assert!(!running(&h, spider), "le but finit à l'atterrissage");

        let Some((mut near, spider_near)) = world(1.0) else { return };
        let mut leapt_near = false;
        for _ in 0..200 {
            near.tick();
            leapt_near |= running(&near, spider_near);
        }
        assert!(!leapt_near, "pas de bond à 1 bloc");
    }
}
