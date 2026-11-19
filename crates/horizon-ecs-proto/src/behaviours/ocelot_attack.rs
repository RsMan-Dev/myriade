//! `OcelotAttackGoal` (fiche 65 ; ocelot priorité 8, chat priorité 9 : `tame.rs:1023` et `1065`). Le but démarre à
//! l'acquisition d'une cible et tourne tant qu'elle est vivante et à 225 (carré) au plus. Il regarde la cible (vitesse
//! 30), marche vers sa position à 1,33 entre (2 x largeur)² et 16, 0,6 au-delà jusqu'à 225, 0,8 au-delà, et frappe
//! toutes les 20 ticks à portée (carré <= (2 x largeur)²). Aucun tirage. Quand le but s'arrête alors que la cible est
//! tenue, le sélecteur redemande à +2 (condition de fond : la distance, sans événement du cadre).
//! DOUTE : la fonction d'attaque de `melee` n'est pas publique ; le coup reprend son appel (`DamageQueue::add_by_entity`
//! ou `PlayerState::hurt`), sans son cadencement (`melee_run`), `attack_time` du module tient lieu de cadence.
//! DOUTE : la redemande à +2 ne suit pas la phase du sélecteur (échéance de la roue).
//! DOUTE : la condition de départ inclut la distance (`d <= 225`) ; le `Mob` démarre puis arrête au même tick quand la
//! cible est trop loin (`canUse` = cible tenue) : aucun effet sur le monde, seul l'ordre des drapeaux diffère.
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::hostile::PlayerState;
use crate::targeting::{self, Target, TargetWatchers, Who};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Vitesse hors de la portée de mêlée et au-delà de `band` (`d² < 225`).
    pub slow: f64,
    /// Vitesse entre la portée de mêlée et `band` (d² dans (portée², 16)).
    pub near: f64,
    /// Vitesse au-delà de `within` (d² >= 225).
    pub far: f64,
    /// Le carré de distance au-delà duquel le but finit (`canContinueToUse`, `d² <= 225`).
    pub within: f64,
    /// Le carré de distance qui sépare « proche » de « moyen » pour la vitesse (16).
    pub band: f64,
    /// Ticks entre deux coups (`attack_time`).
    pub cooldown: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { slow: 0.6, near: 1.33, far: 0.8, within: 225.0, band: 16.0, cooldown: 20 };
}

/// L'état : les paramètres, le temps avant le prochain coup, et la redemande armée (une seule chaîne par animal).
#[derive(Component, Clone, Copy, Debug)]
pub struct Pounce {
    p: Params,
    attack_time: i32,
    armed: bool,
}
/// Le but tourne.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Attacking;

pub static MODULE: Module = Module {
    name: "ocelot_attack",
    flags: MOVE | LOOK,
    resources,
    equip,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Head, Cap::Targets, Cap::Locomotion],
    ..Module::DEFAULT
};
fn resources(w: &mut World) {
    w.resource_mut::<TargetWatchers>().0.push(on_target);
}
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Pounce { p, attack_time: 0, armed: false });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(attack_run.in_set(slot));
}
/// Une cible acquise : réveil du but, sauf si une redemande est déjà armée ou si le but tourne.
fn on_target(w: &mut World, e: Entity) {
    let Some(pn) = w.get::<Pounce>(e).copied() else { return };
    if pn.armed || w.get::<Attacking>(e).is_some() {
        return;
    }
    wake(w, e, &MODULE);
}
/// La cible vivante, avec le carré de la distance à l'animal.
fn gap(w: &World, e: Entity, who: Who) -> Option<(targeting::Info, f64)> {
    let p = w.get::<Pos>(e)?.0;
    let i = targeting::info(w, who)?;
    if let Who::Entity(c) = who
        && w.get::<Health>(c).is_some_and(|h| h.0 <= 0.0)
    {
        return None;
    }
    let d = d2(p, i.pos);
    Some((i, d))
}
/// Redemande à +2 tant que la cible est tenue et vivante (une seule chaîne).
fn rearm(w: &mut World, e: Entity, slot: u8) {
    let Some(mut pn) = w.get_mut::<Pounce>(e) else { return };
    if pn.armed {
        return;
    }
    pn.armed = true;
    arm(w, e, slot, 2);
}
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if let Some(mut pn) = w.get_mut::<Pounce>(e) {
        pn.armed = false;
    }
    w.get::<Attacking>(e).is_none() && w.get::<Target>(e).is_some()
}
/// `canUse` : une cible tenue et à 225 au plus ; sinon redemande (cible tenue et vivante seulement).
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    if w.get::<Attacking>(e).is_some() {
        return false;
    }
    let Some(who) = w.get::<Target>(e).map(|t| t.who) else { return false };
    let Some(within) = w.get::<Pounce>(e).map(|pn| pn.p.within) else { return false };
    let Some((_, d)) = gap(w, e, who) else { return false };
    if d <= within {
        return true;
    }
    rearm(w, e, inst.slot);
    false
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Attacking);
    // Un but démarré est ticqué dans le tick où il démarre (GUIDE §9.1).
    tick_one(w, e);
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    stop_nav(w, e);
    w.entity_mut(e).remove::<Attacking>();
    if w.get::<Target>(e).is_some() {
        rearm(w, e, inst.slot);
    }
}
fn attack_run(w: &mut World, q: &mut QueryState<Entity, With<Attacking>>) {
    let runs: Vec<Entity> = q.iter(w).collect();
    for e in runs {
        if !tick_one(w, e)
            && let Some(slot) = slot_of(w, e, &MODULE)
        {
            w.resource_mut::<Requests>().0.push(Req::End(e, slot));
        }
    }
}
/// `canContinueToUse` puis `tick` : false quand le but doit finir.
fn tick_one(w: &mut World, e: Entity) -> bool {
    let Some(pn) = w.get::<Pounce>(e).copied() else { return false };
    let Some(who) = w.get::<Target>(e).map(|t| t.who) else { return false };
    let Some((i, d)) = gap(w, e, who) else { return false };
    if d > pn.p.within {
        return false;
    }
    let reach = f64::from(w.get::<Body>(e).map_or(0.0, |b| b.width)) * 2.0;
    let melee = reach * reach;
    if let Some(mut l) = w.get_mut::<Look>(e) {
        l.target = [i.pos.x, i.eye, i.pos.z];
        l.cooldown = 2;
        l.y_speed = 30.0;
        l.x_max = 30.0;
    }
    if w.get::<Looking>(e).is_none() {
        w.entity_mut(e).insert(Looking);
    }
    let speed = if d > melee && d < pn.p.band {
        pn.p.near
    } else if d < pn.p.within {
        pn.p.slow
    } else {
        pn.p.far
    };
    go_to_at(w, e, [i.pos.x, i.pos.y, i.pos.z], speed, 0);
    let mut attack_time = (pn.attack_time - 1).max(0);
    if d <= melee && attack_time <= 0 {
        attack_time = pn.p.cooldown;
        strike(w, e, who);
    }
    if let Some(mut m) = w.get_mut::<Pounce>(e) {
        m.attack_time = attack_time;
    }
    true
}
/// Le coup : dégâts d'attribut (1 si nul), par `hostile::strike`.
fn strike(w: &mut World, e: Entity, who: Who) {
    let amount = w.get::<AttackDamage>(e).map_or(0.0, |d| d.0);
    crate::hostile::strike(w, e, who, if amount <= 0.0 { 1.0 } else { amount });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, Pos, SpeciesTable, spawn_species};
    use crate::species::Config;
    use crate::targeting::set_target;
    use horizon_core::geometry::Vec3;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 8, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// Un zombie qui a ce seul but (dégâts d'attribut 3), et une vache à `dist` blocs sur x, cible acquise.
    fn world(dist: f64) -> Option<(Herd, Entity, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(60, true, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:zombie").unwrap();
        sp.goals = config();
        let mut cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        cow.goals = Config { entries: &[] };
        let attacker = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
        let prey = spawn_species(&mut h.world, &cow, 0.5 + dist, 0.5, 62);
        for e in [attacker, prey] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        h.world.entity_mut(attacker).insert(AttackDamage(3.0));
        // Le sol d'un animal apparu est inconnu au premier tick : on le laisse se poser avant de le faire viser.
        for _ in 0..5 {
            h.tick();
        }
        let y = h.world.get::<Pos>(attacker).unwrap().0.y;
        h.teleport(prey, Vec3::new(0.5 + dist, y, 0.5));
        set_target(&mut h.world, attacker, Target::new(Who::Entity(prey), 200, u8::MAX));
        Some((h, attacker, prey))
    }

    /// Témoin : le but tourne à 1 bloc et la vache perd 3 points de vie (le coup d'attribut) ; sans le but, rien.
    #[test]
    fn it_runs_and_strikes_a_target_in_reach() {
        let Some((mut h, attacker, prey)) = world(1.0) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let before = h.world.get::<Health>(prey).unwrap().0;
        let mut ran = false;
        for _ in 0..60 {
            h.tick();
            ran |= running(&h, attacker);
        }
        assert!(ran, "témoin : le but démarre à l'acquisition d'une cible à 1 bloc");
        let after = h.world.get::<Health>(prey).unwrap().0;
        assert!(before - after >= 3.0 - 1e-3, "un coup de 3 au moins : {before} -> {after}");
    }

    /// Le but démarre à 3 blocs, puis finit quand la cible passe au-delà de 15 blocs (carré 225).
    #[test]
    fn it_stops_when_the_target_is_beyond_fifteen_blocks() {
        let Some((mut h, attacker, prey)) = world(3.0) else { return };
        let mut ran = false;
        for _ in 0..30 {
            h.tick();
            ran |= running(&h, attacker);
        }
        assert!(ran, "témoin : le but tourne à 3 blocs");
        // À 16 blocs de l'attaquant tel qu'il est maintenant (il a rejoint la cible pendant les 30 ticks).
        let a = h.world.get::<Pos>(attacker).unwrap().0;
        h.teleport(prey, Vec3::new(a.x + 16.0, a.y, a.z));
        for _ in 0..30 {
            h.tick();
        }
        assert!(!running(&h, attacker), "le but finit au-delà de 15 blocs");
    }
}
