//! Machine de phases : un identifiant d'un octet (`PhaseId`), l'état rare des phases qui se déplacent (`PhaseData`), le cadre
//! (`enter`, `set_phase`, `wake`) et la table `PHASES`, indexée par l'identifiant (ordre de déclaration du jar). Les réveils sont
//! des échéances sur le temps propre de l'entité (`cow::after`) ; une phase qui doit regarder chaque tick se réarme seule.
use super::data;
use super::death;
use super::fight;
use super::fireball;
use super::flight::LevelRandom;
use super::nodes::{self, FlightNodes, Path};
use super::perch;
use crate::cow::{after, now_of, Pos};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use horizon_core::random::{LegacyRandom, RandomSource};

pub const HOLDING: u8 = 0;
pub const STRAFE: u8 = 1;
pub const LANDING_APPROACH: u8 = 2;
pub const LANDING: u8 = 3;
pub const CHARGING: u8 = 8;

/// Ce qu'une phase rend : la phase suivante et son état, ou rien.
pub type Next = Option<(u8, PhaseState)>;

/// L'état des phases qui se déplacent, dans une boîte (rare). Une phase posée n'en a pas : elle garde `perch::PerchTime`.
#[derive(Clone, Debug, Default)]
pub struct PhaseState {
    pub path: Option<Path>,
    pub target: Option<Vec3>,
    pub clockwise: bool,
    /// Strafe : le compte des tirs avant le tir de feu.
    pub charge: i32,
    /// Charge : le temps propre où elle a commencé (None tant que la cible est hors de portée de vol).
    pub since: Option<u64>,
    /// Strafe : le joueur visé.
    pub player: Option<u32>,
    /// Décollage : premier tick.
    pub first: bool,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhaseId(pub u8);

#[derive(Component, Clone, Debug, Default)]
pub struct PhaseData(pub Box<PhaseState>);

/// Un changement de phase : l'entité, la phase quittée, la phase entrée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhaseChanged {
    pub e: Entity,
    pub from: u8,
    pub to: u8,
}

/// Le journal des `PhaseChanged`, pour l'hôte et les tests (à relier à `goals::Events` si le cahier le demande).
#[derive(Resource, Default)]
pub struct PhaseChanges(pub Vec<PhaseChanged>);

pub trait PhaseDef: Sync {
    /// Garde son état dans `PhaseData`.
    fn rare(&self) -> bool {
        true
    }
    /// Entrer à nouveau dans la phase la relance (le strafe sur un autre joueur).
    fn restart(&self) -> bool {
        false
    }
    /// Se réveille à chaque tick tant qu'elle tourne ; sinon, seulement par ses échéances.
    fn per_tick(&self, _: &World, _: Entity) -> bool {
        true
    }
    fn begin(&self, _: &mut World, _: Entity) {}
    fn end(&self, _: &mut World, _: Entity) {}
    fn on_wake(&self, w: &mut World, e: Entity) -> Next;
}

/// La table, par identifiant : Holding 0, Strafe 1, LandingApproach 2, Landing 3, Takeoff 4, Flaming 5, Scanning 6,
/// Attacking 7, Charging 8, Dying 9, Hover 10.
pub static PHASES: [&dyn PhaseDef; 11] = [
    &Holding,
    &Strafe,
    &LandingApproach,
    &Landing,
    &Takeoff,
    &perch::Flaming,
    &perch::Scanning,
    &perch::Attacking,
    &Charging,
    &Dying,
    &Hover,
];

/// Entre dans `to` avec `st` : l'ancienne phase finit, l'identifiant et l'état rare sont écrits, le changement est journalisé,
/// la nouvelle commence, et si elle se réveille par tick, son premier réveil est armé.
pub fn enter(w: &mut World, e: Entity, to: u8, st: PhaseState) {
    let from = w.get::<PhaseId>(e).map(|p| p.0);
    let def = PHASES[to as usize];
    if from == Some(to) && !def.restart() {
        return;
    }
    if let Some(f) = from {
        PHASES[f as usize].end(w, e);
    }
    w.entity_mut(e).insert(PhaseId(to));
    if def.rare() {
        w.entity_mut(e).insert(PhaseData(Box::new(st)));
    } else {
        w.entity_mut(e).remove::<PhaseData>();
    }
    w.get_resource_or_insert_with(PhaseChanges::default).0.push(PhaseChanged { e, from: from.unwrap_or(to), to });
    def.begin(w, e);
    if def.per_tick(w, e) {
        after(w, e, 1, wake);
    }
}

/// L'écrivain de `PhaseId` hors `enter` : entre dans `to` avec l'état par défaut.
pub fn set_phase(w: &mut World, e: Entity, to: u8) {
    enter(w, e, to, PhaseState::default());
}

/// Un réveil : la phase courante décide ; sans changement, elle se réarme si elle se réveille par tick.
pub fn wake(w: &mut World, e: Entity) {
    let Some(id) = w.get::<PhaseId>(e).map(|p| p.0) else { return };
    let def = PHASES[id as usize];
    match def.on_wake(w, e) {
        Some((to, st)) => enter(w, e, to, st),
        None if def.per_tick(w, e) => after(w, e, 1, wake),
        None => {}
    }
}

/// `(x, z)` normalisé, ou nul (`normalize_or_zero`).
pub fn unit2(x: f64, z: f64) -> (f64, f64) {
    let l = x.hypot(z);
    if l == 0.0 { (0.0, 0.0) } else { (x / l, z / l) }
}

/// Hors de la bande où le vol tient une cible : moins de 10 blocs ou plus de 150 (carrés : `dt < 100 || dt > 22500`).
pub fn out_of_reach(d: f64) -> bool {
    !(100.0..=22500.0).contains(&d)
}

/// Carré de la distance entre deux points.
pub fn d2(a: Vec3, b: Vec3) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)
}

fn hold() -> Next {
    Some((HOLDING, PhaseState::default()))
}

fn pos(w: &World, e: Entity) -> Vec3 {
    w.get::<Pos>(e).map_or(Vec3::new(0.0, 0.0, 0.0), |p| p.0)
}

fn take(w: &mut World, e: Entity) -> PhaseState {
    std::mem::take(&mut *w.get_mut::<PhaseData>(e).expect("phase data").0)
}

fn put(w: &mut World, e: Entity, s: PhaseState) {
    *w.get_mut::<PhaseData>(e).expect("phase data").0 = s;
}

fn target(w: &World, e: Entity) -> Option<Vec3> {
    w.get::<PhaseData>(e).and_then(|d| d.0.target)
}

/// Lance `f` avec le graphe de vol et l'aléa du niveau (`flight::LevelRandom`).
fn fly<T>(w: &mut World, f: impl FnOnce(&FlightNodes, &mut LegacyRandom) -> T) -> T {
    w.resource_scope(|w, mut r: Mut<LevelRandom>| {
        let g = w.resource::<FlightNodes>();
        f(g, &mut r.0)
    })
}

/// Le chemin de l'anneau d'attente, depuis le nœud le plus proche.
fn ring(w: &mut World, e: Entity, s: &mut PhaseState) {
    let here = pos(w, e);
    let crystals = fight::alive_crystals(w);
    let g = w.resource::<FlightNodes>().clone();
    let cur = g.closest(crystals, here);
    s.path = nodes::ring_path(&g, crystals, cur, false, &mut s.clockwise);
}

fn step(w: &mut World, s: &mut PhaseState) {
    if let Some(t) = fly(w, |g, rng| nodes::next_target(&mut s.path, g, rng)) {
        s.target = Some(t);
    }
}

fn roll(w: &mut World, bound: i32) -> i32 {
    fly(w, |_, rng| rng.next_int_bound(bound).unwrap_or(1))
}

pub struct Holding;
impl PhaseDef for Holding {
    fn on_wake(&self, w: &mut World, e: Entity) -> Next {
        let mut s = take(w, e);
        let r = holding(w, e, &mut s);
        put(w, e, s);
        r
    }
}

fn holding(w: &mut World, e: Entity, s: &mut PhaseState) -> Next {
    let here = pos(w, e);
    if !out_of_reach(s.target.map_or(0.0, |t| d2(t, here))) {
        return None;
    }
    if s.path.as_ref().is_some_and(Path::done) {
        if let Some(n) = holding_pick(w) {
            return Some(n);
        }
    }
    if s.path.as_ref().is_none_or(Path::done) {
        ring(w, e, s);
    }
    step(w, s);
    None
}

/// Au bout d'un chemin : une approche d'atterrissage, ou un strafe sur l'ennemi le plus proche de l'œuf.
fn holding_pick(w: &mut World) -> Next {
    let podium = w.resource::<FlightNodes>().podium();
    let centre = Vec3::new(0.5, podium.y + 0.5, 0.5);
    let egg = Vec3::new(0.0, podium.y, 0.0);
    let crystals = i32::from(fight::alive_crystals(w));
    if roll(w, crystals + 3) == 0 {
        return Some((LANDING_APPROACH, PhaseState::default()));
    }
    let (who, at) = fight::nearest_enemy(w, egg, None, None)?;
    let dist = d2(at, centre) / 512.0;
    if roll(w, (dist + 2.0) as i32) == 0 || roll(w, crystals + 2) == 0 {
        return Some((STRAFE, PhaseState { player: Some(who), ..Default::default() }));
    }
    None
}

pub struct Strafe;
impl PhaseDef for Strafe {
    fn restart(&self) -> bool {
        true
    }
    fn begin(&self, w: &mut World, e: Entity) {
        let mut s = take(w, e);
        if let Some(pp) = s.player.and_then(|q| fight::player_pos(w, q)) {
            let here = pos(w, e);
            let crystals = fight::alive_crystals(w);
            let (p, t) = fly(w, |g, rng| nodes::strafe_begin(g, crystals, pp, here, rng));
            s.path = p;
            s.target = t;
        }
        put(w, e, s);
    }
    fn on_wake(&self, w: &mut World, e: Entity) -> Next {
        let mut s = take(w, e);
        let r = strafe(w, e, &mut s);
        put(w, e, s);
        r
    }
}

fn strafe(w: &mut World, e: Entity, s: &mut PhaseState) -> Next {
    let Some(q) = s.player else { return hold() };
    let Some(pp) = fight::player_pos(w, q) else { return hold() };
    let here = pos(w, e);
    if s.path.as_ref().is_some_and(Path::done) {
        let ho = (0.4 + (pp.x - here.x).hypot(pp.z - here.z) / 80.0 - 1.0).min(10.0);
        s.target = Some(Vec3::new(pp.x, pp.y + ho, pp.z));
    }
    if out_of_reach(s.target.map_or(0.0, |t| d2(t, here))) {
        if s.path.as_ref().is_none_or(Path::done) {
            ring(w, e, s);
        }
        step(w, s);
    }
    let sees = d2(pp, here) < 4096.0 && {
        let eye = Vec3::new(here.x, here.y + 6.8, here.z);
        fight::line_of_sight(w, eye, Vec3::new(pp.x, fight::eye_y(w, q), pp.z))
    };
    if !sees {
        s.charge = (s.charge - 1).max(0);
        return None;
    }
    s.charge += 1;
    let yaw = f64::from(fight::yaw(w, e).to_radians());
    let dir = unit2(yaw.sin(), -yaw.cos());
    let aim = unit2(pp.x - here.x, pp.z - here.z);
    let angle = (dir.0 * aim.0 + dir.1 * aim.1).clamp(-1.0, 1.0).acos().to_degrees() + 0.5;
    if s.charge >= 5 && (0.0..10.0).contains(&angle) {
        let head = fight::head(w, e);
        let view = fight::view(w, e);
        let start = Vec3::new(head.x - view.x, head.y + 1.0, head.z - view.z);
        let to = Vec3::new(pp.x - start.x, pp.y + 0.9 - start.y, pp.z - start.z);
        fireball::shoot(w, Who::Entity(e), start, to);
        s.charge = 0;
        return hold();
    }
    None
}

pub struct LandingApproach;
impl PhaseDef for LandingApproach {
    fn on_wake(&self, w: &mut World, e: Entity) -> Next {
        let mut s = take(w, e);
        let r = approach(w, e, &mut s);
        put(w, e, s);
        r
    }
}

fn approach(w: &mut World, e: Entity, s: &mut PhaseState) -> Next {
    let here = pos(w, e);
    if !out_of_reach(s.target.map_or(0.0, |t| d2(t, here))) {
        return None;
    }
    if s.path.as_ref().is_none_or(Path::done) {
        let podium = w.resource::<FlightNodes>().podium();
        let crystals = fight::alive_crystals(w);
        let g = w.resource::<FlightNodes>().clone();
        let current = g.closest(crystals, here);
        let node = match fight::nearest_enemy(w, Vec3::new(0.0, podium.y, 0.0), None, None) {
            Some((_, pp)) => {
                let (x, z) = unit2(pp.x, pp.z);
                g.closest(crystals, Vec3::new(-x * 40.0, 105.0, -z * 40.0))
            }
            None => g.closest(crystals, Vec3::new(40.0, podium.y, 0.0)),
        };
        let last = [podium.x.floor() as i32, podium.y as i32, podium.z.floor() as i32];
        let mut p = nodes::find_path(&g, crystals, current, node, Some(last));
        if let Some(p) = p.as_mut() {
            p.advance();
        }
        s.path = p;
    }
    step(w, s);
    s.path.as_ref().is_some_and(Path::done).then(|| (LANDING, PhaseState::default()))
}

pub struct Landing;
impl PhaseDef for Landing {
    fn begin(&self, w: &mut World, e: Entity) {
        let podium = w.resource::<FlightNodes>().podium();
        if let Some(mut d) = w.get_mut::<PhaseData>(e) {
            d.0.target = Some(podium);
        }
    }
    fn on_wake(&self, w: &mut World, e: Entity) -> Next {
        let here = pos(w, e);
        if target(w, e).is_some_and(|t| d2(t, here) < 1.0) {
            perch::landed(w, e);
            return Some((data::SITTING_SCANNING, PhaseState::default()));
        }
        None
    }
}

pub struct Takeoff;
impl PhaseDef for Takeoff {
    fn begin(&self, w: &mut World, e: Entity) {
        if let Some(mut d) = w.get_mut::<PhaseData>(e) {
            d.0.first = true;
        }
    }
    fn on_wake(&self, w: &mut World, e: Entity) -> Next {
        let mut s = take(w, e);
        let r = take_off(w, e, &mut s);
        put(w, e, s);
        r
    }
}

fn take_off(w: &mut World, e: Entity, s: &mut PhaseState) -> Next {
    let here = pos(w, e);
    if !s.first && s.path.is_some() {
        let podium = w.resource::<FlightNodes>().podium();
        return if d2(podium, here) >= 100.0 { hold() } else { None };
    }
    s.first = false;
    let crystals = fight::alive_crystals(w);
    let g = w.resource::<FlightNodes>().clone();
    let current = g.closest(crystals, here);
    let look = fight::look(w, e);
    let t0 = g.closest(crystals, Vec3::new(-look.x * 40.0, 105.0, -look.z * 40.0)) as i32;
    let t = if crystals > 0 { t0.rem_euclid(12) } else { ((t0 - 12) & 7) + 12 };
    let mut p = nodes::find_path(&g, crystals, current, t as usize, None);
    if let Some(p) = p.as_mut() {
        p.advance();
    }
    s.path = Some(p.unwrap_or_default());
    step(w, s);
    None
}

pub struct Charging;
impl PhaseDef for Charging {
    /// Tant que la charge n'a pas commencé, elle se réveille par tick ; ensuite, par une échéance.
    fn per_tick(&self, w: &World, e: Entity) -> bool {
        w.get::<PhaseData>(e).is_some_and(|d| d.0.since.is_none())
    }
    fn on_wake(&self, w: &mut World, e: Entity) -> Next {
        let mut s = take(w, e);
        let here = pos(w, e);
        let now = now_of(w, e);
        let r = match (s.target, s.since) {
            (None, _) => hold(),
            (Some(_), Some(since)) if now.saturating_sub(since) >= 9 => hold(),
            (Some(t), None) if out_of_reach(d2(t, here)) => {
                s.since = Some(now);
                after(w, e, 9, wake);
                None
            }
            _ => None,
        };
        put(w, e, s);
        r
    }
}

/// `Dying` : la vie et le temps de mort sont au lot L5 (`death.rs`) ; la phase ne fait ici que son début.
pub struct Dying;
impl PhaseDef for Dying {
    fn begin(&self, w: &mut World, e: Entity) {
        death::begin(w, e);
    }
    fn per_tick(&self, _: &World, _: Entity) -> bool {
        false
    }
    fn on_wake(&self, _: &mut World, _: Entity) -> Next {
        None
    }
}

/// `Hover` : la cible est le point où la phase commence, une seule fois.
pub struct Hover;
impl PhaseDef for Hover {
    fn begin(&self, w: &mut World, e: Entity) {
        let here = pos(w, e);
        if let Some(mut d) = w.get_mut::<PhaseData>(e) {
            d.0.target.get_or_insert(here);
        }
    }
    fn per_tick(&self, _: &World, _: Entity) -> bool {
        false
    }
    fn on_wake(&self, _: &mut World, _: Entity) -> Next {
        None
    }
}
