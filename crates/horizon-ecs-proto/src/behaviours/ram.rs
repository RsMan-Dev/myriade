//! `GoatAi` ram (fiche 99) : `PrepareRamNearestTarget` puis `RamTarget`, un seul but. Après le compte à rebours (`RamCooldown`),
//! la chèvre choisit la cible la plus proche vue (16 blocs, pas une chèvre), cherche à 4 à 7 blocs un départ d'où elle la
//! rejoint, y va à 1,25, baisse la tête (20 ticks), puis charge à 3,0 jusqu'à la toucher ou finir son chemin.
//! Replis (fiche 99) : le recul est le recul standard (`Cause::Entity`, sans colère : pas de `by`), pas de cornes, pas de
//! sonde de chemin pour le départ (le plus proche des départs marchables est retenu) ; la tentation (un joueur à portée
//! tenant la nourriture) et l'amour (`breed::Love`) la font céder. Joueurs visés et touchés, tête baissée (événements 58/59).
use crate::IFloor as _;
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::spots::stable_destination;
use crate::bridge::teams::allied;
use crate::hostile::PlayerState;
use crate::targeting::{self, Eye, Who};
use horizon_client_lib::items::ItemStack;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

/// `RAMS`, `RAM_PREPARE_TIME`, `MIN/MAX_RAM_DISTANCE`, les délais d'expiration, la portée de la cible et les vitesses.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `RAMS` : le compte à rebours après une charge (uniforme), et le minimum après un échec.
    pub cooldown: (i32, i32),
    /// A variant a marker picks (its countdown and sounds); None: the countdown and sounds above.
    pub marked: Option<Variant>,
    pub prepare_sound: &'static str,
    pub impact_sound: &'static str,
    /// The parts a ram can snap off (`Goat.dropHorn`); None: none can come off.
    pub shed: Option<ShedDef>,
    pub prepare_wait: u64,
    pub min_distance: i32,
    pub max_distance: i32,
    pub prepare_timeout: i32,
    pub ram_timeout: i32,
    pub sense_range: f64,
    pub approach_speed: f64,
    pub charge_speed: f64,
}
impl Params {
    pub const DEFAULT: Self = Self {
        cooldown: (600, 6000),
        marked: None,
        prepare_sound: "entity.goat.prepare_ram",
        impact_sound: "entity.goat.ram_impact",
        shed: None,
        prepare_wait: 20,
        min_distance: 4,
        max_distance: 7,
        prepare_timeout: 160,
        ram_timeout: 200,
        sense_range: 16.0,
        approach_speed: 1.25,
        charge_speed: 3.0,
    };
}

/// A marked variant of an animal: its marker's test, and its own countdown and sounds.
#[derive(Clone, Copy, Debug)]
pub struct Variant {
    pub is: fn(&World, Entity) -> bool,
    pub cooldown: (i32, i32),
    pub prepare_sound: &'static str,
    pub impact_sound: &'static str,
    pub shed: Option<ShedDef>,
}

/// What a ram can snap off: the items a part drops (one picked by the animal's random) and the sound of the break.
#[derive(Clone, Copy, Debug)]
pub struct ShedDef {
    pub items: &'static [&'static str],
    pub break_sound: &'static str,
}

/// The parts snapped off so far: bit 0 the left, bit 1 the right. Absent: all of them are there.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Shed(pub u8);

/// `RAM_COOLDOWN_TICKS` : la date (temps propre) à partir de laquelle le bélier peut recommencer. Lue par `long_jump`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct RamCooldown(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Done,
    Prepare,
    RamStart,
    Ram,
}
#[derive(Clone, Copy, Debug)]
struct Candidate {
    start: [i32; 3],
    target_pos: [i32; 3],
    target: Who,
}

/// L'état du but sur la chèvre : ses paramètres, l'échéance armée, et la charge en cours.
#[derive(Component, Clone, Copy, Debug)]
pub struct Rammer {
    slot: u8,
    p: &'static Params,
    armed: Option<u64>,
    phase: Phase,
    candidate: Option<Candidate>,
    reached_at: Option<u64>,
    ram_target: Option<Vec3>,
    ram_direction: Vec3,
    time: i32,
    /// The head is down (`Goat.setLoweredHead`, the entity events 58 and 59 sent when it changes).
    lowering: bool,
}

/// Le but tourne (son `tick` le visite).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Ramming;

pub static MODULE: Module = Module {
    name: "ram",
    flags: MOVE | LOOK,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Head, Cap::Locomotion, Cap::Targets],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert((
        Rammer { slot: inst.slot, p, armed: None, phase: Phase::Done, candidate: None, reached_at: None, ram_target: None, ram_direction: Vec3::ZERO, time: 0, lowering: false },
        RamCooldown(0),
    ));
}

fn sample(r: &mut Rng, (lo, hi): (i32, i32)) -> i32 {
    lo + r.next_int(hi - lo + 1)
}

/// Une échéance de la roue à la date `date` (temps propre), une seule par date.
fn schedule(w: &mut World, e: Entity, date: u64) {
    let now = now_of(w, e);
    let date = date.max(now + 1);
    let Some(mut r) = w.get_mut::<Rammer>(e) else { return };
    if r.armed == Some(date) {
        return;
    }
    r.armed = Some(date);
    let slot = r.slot;
    drop(r);
    arm_at(w, e, date, Due::Goal(slot));
}

/// `RAM_COOLDOWN_TICKS` : une date, et son échéance.
pub(crate) fn set_cooldown(w: &mut World, e: Entity, ticks: i32) {
    let date = now_of(w, e) + ticks.max(0) as u64;
    if let Some(mut c) = w.get_mut::<RamCooldown>(e) {
        c.0 = date;
    }
    schedule(w, e, date);
    // The goat's long jump activity holds while the ram cools down: it is asked now (no polling for the end of the ram).
    crate::goals::wake(w, e, &crate::behaviours::long_jump::MODULE);
}

/// The ticks left of `RAM_COOLDOWN_TICKS` (the memory `minecraft:ram_cooldown_ticks`), None when it is over.
pub fn cooldown_left(w: &World, e: Entity) -> Option<u64> {
    let now = now_of(w, e);
    w.get::<RamCooldown>(e).map(|c| c.0).filter(|d| *d > now).map(|d| d - now)
}
/// Sets the ticks left of the ram cooldown (`None`: over), as the memory read of a save gives them.
pub fn set_cooldown_left(w: &mut World, e: Entity, left: Option<u64>) {
    match left {
        Some(t) => set_cooldown(w, e, t as i32),
        None => {
            if let Some(mut c) = w.get_mut::<RamCooldown>(e) {
                c.0 = 0;
            }
        }
    }
}

/// `RAMS` of this animal: the marked variant's range when its marker is there.
fn rams(w: &World, e: Entity) -> (i32, i32) {
    let Some(p) = w.get::<Rammer>(e).map(|r| r.p) else { return (600, 6000) };
    match p.marked {
        Some(v) if (v.is)(w, e) => v.cooldown,
        _ => p.cooldown,
    }
}

/// The sounds of the prepare and of the impact, the marked variant's when its marker is there.
fn sounds(w: &World, e: Entity) -> (&'static str, &'static str) {
    let Some(p) = w.get::<Rammer>(e).map(|r| r.p) else { return ("", "") };
    match p.marked {
        Some(v) if (v.is)(w, e) => (v.prepare_sound, v.impact_sound),
        _ => (p.prepare_sound, p.impact_sound),
    }
}

/// `Mob.voice_pitch` (`LivingEntity.getVoicePitch`): 1 ± 0,2 as an adult, 1,5 ± 0,2 as a baby (`GrowAt`), from two draws of its own random.
pub(crate) fn voice_pitch(w: &mut World, e: Entity) -> f32 {
    let base = if w.get::<GrowAt>(e).is_some() { 1.5 } else { 1.0 };
    with_rng(w, e, |r| {
        let a = (r.next_u32() >> 8) as f32 / 16_777_216.0;
        let b = (r.next_u32() >> 8) as f32 / 16_777_216.0;
        (a - b) * 0.2 + base
    })
}

/// `Goat.setLoweredHead` and the entity event (58 down, 59 up) a server sends when the head changes.
fn lower(w: &mut World, e: Entity, down: bool) {
    let Some(mut r) = w.get_mut::<Rammer>(e) else { return };
    if r.lowering == down {
        return;
    }
    r.lowering = down;
    drop(r);
    w.resource_mut::<Events>().host.entity_event(e, if down { 58 } else { 59 });
}

/// Premier compte à rebours : `finalize` tire `[600, 6000]`.
fn arm(w: &mut World, e: Entity, _: Inst) {
    let (lo, hi) = w.get::<Rammer>(e).map_or((600, 6000), |r| r.p.cooldown);
    let t = with_rng(w, e, |r| sample(r, (lo, hi)));
    set_cooldown(w, e, t);
}

/// Une échéance du compte à rebours (ou de l'attente d'un sélecteur) : `canUse` aux ticks de sélecteur seulement.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    if let Some(mut r) = w.get_mut::<Rammer>(e)
        && r.armed == Some(now)
    {
        r.armed = None;
    }
    if w.get::<Ramming>(e).is_some() {
        return false;
    }
    if !(now <= 1 || (now + phase_of(w, e)) % 2 == 0) {
        schedule(w, e, now + 1);
        return false;
    }
    true
}

/// `PrepareRamNearestTarget.canUse` (`RamCooldown` échu) et pas de joueur qui tente (l'amour n'est pas lisible : absent).
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    let date = w.get::<RamCooldown>(e).map_or(0, |c| c.0);
    if now < date {
        schedule(w, e, date);
        return false;
    }
    !super::tempt::tempted(w, e) && super::long_jump::breeding_until(w, e).is_none()
}

fn start(w: &mut World, e: Entity, _: Inst) {
    if let Some(mut r) = w.get_mut::<Rammer>(e) {
        r.phase = Phase::Prepare;
        r.time = 0;
        r.reached_at = None;
        r.candidate = None;
    }
    w.entity_mut(e).insert(Ramming);
    if let Some(target) = nearest_target(w, e) {
        choose(w, e, target);
    }
    // Le tick du même tick (GUIDE règle 9.1).
    body(w, e);
}

/// `PrepareRamNearestTarget.stop` (sans charge : le compte à rebours minimal) ; la fin d'une charge a déjà posé le sien.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Ramming>();
    let phase = w.get::<Rammer>(e).map(|r| r.phase);
    if phase == Some(Phase::Prepare) {
        lower(w, e, false);
        let wait = rams(w, e).0;
        set_cooldown(w, e, wait);
    }
    if let Some(mut r) = w.get_mut::<Rammer>(e) {
        r.phase = Phase::Done;
        r.candidate = None;
    }
}

fn tick(w: &mut World) {
    for e in crate::goals::entities::<With<Ramming>>(w) {
        run(w, e);
    }
}

/// La cible vit (une créature : sa vie est lue ici, `targeting::info` ne la donne pas ; un joueur vivant a une fiche).
fn alive(w: &World, c: Who) -> bool {
    match c {
        Who::Entity(c) => w.get::<Health>(c).is_none_or(|h| h.0 > 0.0),
        Who::Player(_) => true,
    }
}

/// `RAM_TARGET_CONDITIONS` for a creature: not the goat itself, not its team's, alive, not of its species. A player is a candidate
/// here unless it is in the goat's team (`targeting::info` says whether it can be targeted).
fn ram_target_ok(w: &World, e: Entity, who: Who) -> bool {
    if allied(w, Who::Entity(e), who) {
        return false;
    }
    match who {
        Who::Entity(c) => {
            c != e
                && alive(w, who)
                && !crate::brain::behaviours::play_dead::is_playing_dead(w, c)
                && w.get::<SpeciesId>(c).copied() != w.get::<SpeciesId>(e).copied()
        }
        Who::Player(_) => true,
    }
}

/// `Goal.canContinueToUse` puis `tick`, chaque tick : finir quand la cible vit, le temps n'a pas expiré, ou la charge est en route.
fn run(w: &mut World, e: Entity) {
    let Some(r) = w.get::<Rammer>(e).copied() else { return };
    let keep = match r.phase {
        Phase::Done => false,
        Phase::Prepare => {
            r.time <= r.p.prepare_timeout
                && !super::tempt::tempted(w, e)
                && super::long_jump::breeding_until(w, e).is_none()
                && r.candidate.is_some_and(|c| targeting::info(w, c.target).is_some() && alive(w, c.target))
        }
        Phase::RamStart | Phase::Ram => r.ram_target.is_some() && r.time <= r.p.ram_timeout,
    };
    if !keep {
        w.resource_mut::<Requests>().0.push(Req::End(e, r.slot));
        return;
    }
    body(w, e);
}

/// `PrepareRamNearestTarget.tick` / `RamTarget.tick` : l'attente, la charge puis l'impact.
fn body(w: &mut World, e: Entity) {
    if let Some(mut r) = w.get_mut::<Rammer>(e) {
        r.time += 1;
    }
    let phase = w.get::<Rammer>(e).map(|r| r.phase);
    match phase {
        Some(Phase::Prepare) => prepare_tick(w, e),
        Some(Phase::RamStart) => {
            let Some(r) = w.get::<Rammer>(e).copied() else { return };
            let Some(target) = r.ram_target else { return };
            let here = block_of(pos_of(w, e));
            let d = Vec3::new(f64::from(here[0]) - target.x, 0.0, f64::from(here[2]) - target.z);
            if let Some(mut m) = w.get_mut::<Rammer>(e) {
                m.ram_direction = d.normalize();
                m.phase = Phase::Ram;
                m.time = 0;
            }
            let speed = r.p.charge_speed;
            go_to_at(w, e, [target.x, target.y, target.z], speed, 0);
            ram_tick(w, e);
        }
        Some(Phase::Ram) => ram_tick(w, e),
        _ => {}
    }
}

fn pos_of(w: &World, e: Entity) -> Vec3 {
    w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0)
}

fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.ifloor(), p.y.ifloor(), p.z.ifloor()]
}

fn nav_done(w: &World, e: Entity) -> bool {
    w.get::<Following>(e).is_none() && w.get::<NeedsPath>(e).is_none()
}

/// `PrepareRamNearestTarget.tick` : regarder la cible, marcher vers le départ, attendre 20 ticks, puis fixer le bord de la
/// case de la cible.
fn prepare_tick(w: &mut World, e: Entity) {
    let Some(r) = w.get::<Rammer>(e).copied() else { return };
    let Some(c) = r.candidate else { return };
    let Some(t) = targeting::info(w, c.target) else { return };
    look(w, e, [t.pos.x, t.eye, t.pos.z]);
    let now = now_of(w, e);
    let pos = pos_of(w, e);
    let at = [f64::from(c.start[0]) + 0.5, f64::from(c.start[1]), f64::from(c.start[2]) + 0.5];
    if nav_done(w, e) && block_of(pos) != c.start {
        go_to_at(w, e, at, r.p.approach_speed, 0);
    }
    if block_of(t.pos) != c.target_pos {
        // La cible a changé de case : un nouveau choix.
        lower(w, e, false);
        stop_nav(w, e);
        choose(w, e, c.target);
    } else if block_of(pos) == c.start {
        lower(w, e, true);
        let since = match r.reached_at {
            Some(s) => s,
            None => {
                if let Some(mut m) = w.get_mut::<Rammer>(e) {
                    m.reached_at = Some(now);
                }
                now
            }
        };
        if now - since >= r.p.prepare_wait {
            let edge = |t: i32, s: i32| f64::from(t) + 0.5 + 0.5 * f64::from((t - s).signum());
            let (x, z) = (edge(c.target_pos[0], c.start[0]), edge(c.target_pos[2], c.start[2]));
            let pitch = voice_pitch(w, e);
            let (prepare, _) = sounds(w, e);
            w.resource_mut::<Events>().host.sound(prepare, [pos.x, pos.y, pos.z], 1.0, pitch);
            if let Some(mut m) = w.get_mut::<Rammer>(e) {
                m.ram_target = Some(Vec3::new(x, f64::from(c.target_pos[1]), z));
                m.phase = Phase::RamStart;
                m.candidate = None;
            }
        }
    }
}

/// `PrepareRamNearestTarget.chooseRamPosition`.
fn choose(w: &mut World, e: Entity, target: Who) {
    if let Some(mut r) = w.get_mut::<Rammer>(e) {
        r.reached_at = None;
        r.candidate = None;
    }
    let Some(i) = targeting::info(w, target) else { return };
    let target_pos = block_of(i.pos);
    let start = ramming_start(w, e, target_pos);
    if let (Some(start), Some(mut r)) = (start, w.get_mut::<Rammer>(e)) {
        r.candidate = Some(Candidate { start, target_pos, target });
    }
}

/// `PrepareRamNearestTarget.calculateRammingStartPosition` : la case de départ la plus proche, à 4 à 7 blocs de la cible
/// dans une direction cardinale. Repli : pas de sonde de chemin (la première case marchable triée est retenue).
fn ramming_start(w: &World, e: Entity, target: [i32; 3]) -> Option<[i32; 3]> {
    let mw = level_mw(w);
    let p = w.get::<Rammer>(e)?.p;
    if !walkable(w, e, mw, target) {
        return None;
    }
    let mut starts = Vec::new();
    for (dx, dz) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
        let mut s = target;
        for _ in 0..p.max_distance {
            s = [s[0] + dx, s[1], s[2] + dz];
            if !walkable(w, e, mw, s) {
                s = [s[0] - dx, s[1], s[2] - dz];
                break;
            }
        }
        let manhattan = (s[0] - target[0]).abs() + (s[1] - target[1]).abs() + (s[2] - target[2]).abs();
        if manhattan >= p.min_distance {
            starts.push(s);
        }
    }
    let here = block_of(pos_of(w, e));
    starts.sort_by_key(|s| (0..3).map(|i| i64::from(s[i] - here[i]).pow(2)).sum::<i64>());
    // `create_path_to(p, 0).reached`: the first start a walk reaches, nearest first.
    starts.into_iter().find(|&s| crate::behaviours::long_jump::reaches_on_foot(w, e, s))
}

fn level_mw(w: &World) -> &'static MobWorld<'static> {
    w.resource::<Level>().mw.0
}

/// `PrepareRamNearestTarget.isWalkableBlock` : destination stable et malus de chemin nul.
fn walkable(w: &World, e: Entity, mw: &MobWorld, p: [i32; 3]) -> bool {
    crate::pathsearch::adjacent_reset();
    let t = crate::pathsearch::path_type_static(mw, p[0], p[1], p[2]);
    let malus = w.get::<NavParams>(e).map_or(t.malus(), |n| n.malus[t as usize].unwrap_or_else(|| t.malus()));
    stable_destination(mw, p) && malus == 0.0
}

/// La cible la plus proche dans `sense_range`, vue : les joueurs d'abord (`PrepareRamNearestTarget`), puis les créatures
/// d'une autre espèce ; un joueur ne remplace que par une distance strictement plus courte.
fn nearest_target(w: &mut World, e: Entity) -> Option<Who> {
    let range = w.get::<Rammer>(e)?.p.sense_range;
    let pos = pos_of(w, e);
    let mut near = Vec::new();
    w.resource::<Online>().for_near(pos.x, pos.z, range, |p| near.push(Who::Player(p.id)));
    w.resource::<Index>().for_radius(pos.x, pos.z, range, |c| near.push(Who::Entity(c)));
    let mw = level_mw(w);
    let eye = w.get::<Eye>(e).map_or(0.0, |y| y.0);
    let mut best: Option<(Who, f64)> = None;
    for who in near {
        if !ram_target_ok(w, e, who) {
            continue;
        }
        let Some(i) = targeting::info(w, who) else { continue };
        if i.exempt {
            continue;
        }
        let d = (i.pos.x - pos.x).powi(2) + (i.pos.y - pos.y).powi(2) + (i.pos.z - pos.z).powi(2);
        if d > range * range || best.is_some_and(|b| d >= b.1) {
            continue;
        }
        let from = Vec3::new(pos.x, pos.y + eye, pos.z);
        if mw.line_of_sight_for(Some(pos.y), from, Vec3::new(i.pos.x, i.eye, i.pos.z)) {
            best = Some((who, d));
        }
    }
    best.map(|b| b.0)
}

/// `RamTarget.tick` : la charge touche une créature de sa boîte (dégât sans colère, recul standard), ou finit son chemin.
fn ram_tick(w: &mut World, e: Entity) {
    let pos = pos_of(w, e);
    if let Some(target) = rammed(w, e) {
        let amount = w.get::<AttackDamage>(e).map_or(0.0, |a| a.0);
        let power = ram_power(w, e);
        match target {
            Who::Entity(c) => w.resource_mut::<DamageQueue>().add_ram(c, amount, (pos.x, pos.z), power),
            // `noAggroMobAttack`: the hit angers no one, so no attacker is named.
            Who::Player(id) => {
                let tick = w.resource::<Clock>().0;
                w.resource_mut::<PlayerState>().hurt(tick, id, amount, (pos.x, pos.z), None);
                // The push of `LivingEntity.knockback` away from the goat, on the ground (the player's own velocity is the host's).
                let (px, pz) = crate::area::player_xz(w, id);
                let (dx, dz) = (px - pos.x, pz - pos.z);
                let l = (dx * dx + dz * dz).sqrt().max(1.0e-5);
                crate::area::push_player(w, id, [dx / l * power, power.min(0.4), dz / l * power]);
            }
        }
        finish_ram(w, e);
        let (_, impact) = sounds(w, e);
        w.resource_mut::<Events>().host.sound(impact, [pos.x, pos.y, pos.z], 1.0, 1.0);
    } else if rammed_horn_block(w, e, pos) {
        let (_, impact) = sounds(w, e);
        w.resource_mut::<Events>().host.sound(impact, [pos.x, pos.y, pos.z], 1.0, 1.0);
        if let Some(snap) = drop_horn(w, e, pos) {
            w.resource_mut::<Events>().host.sound(snap, [pos.x, pos.y, pos.z], 1.0, 1.0);
        }
        finish_ram(w, e);
    } else if nav_done(w, e) || w.get::<Rammer>(e).is_some_and(|r| r.ram_target.is_none()) {
        finish_ram(w, e);
    }
}

/// The shed of this animal: its marked variant's when the marker is there.
fn shed_of(w: &World, e: Entity) -> Option<ShedDef> {
    let p = w.get::<Rammer>(e).map(|r| r.p)?;
    match p.marked {
        Some(v) if (v.is)(w, e) => v.shed,
        _ => p.shed,
    }
}

/// `Mob.getEffect` level (amplifier + 1, 0 without the effect) of `kind` on this animal.
fn effect_level(w: &World, e: Entity, kind: crate::effects::Kind) -> i32 {
    w.get::<crate::effects::Effects>(e).and_then(|f| f.list.iter().find(|x| x.kind == kind)).map_or(0, |x| x.amp + 1)
}

/// `RamTarget` power: the goat's speed's factor (with the speed and slowness effects) times 2.5, or 1.0 as a baby.
fn ram_power(w: &World, e: Entity) -> f64 {
    // Jar: `Goat.getSpeed()` during the charge, the movement attribute times the charge's modifier (0.2 * 3.0 for an adult).
    let attr = w.get::<Body>(e).map_or(0.2, |b| b.speed);
    let speed = (attr * w.get::<Rammer>(e).map_or(3.0, |r| r.p.charge_speed)) as f32;
    let boost = 0.25 * (effect_level(w, e, crate::effects::Kind::Speed) - effect_level(w, e, crate::effects::Kind::Slowness)) as f32;
    let factor = (speed * 1.65).clamp(0.2, 3.0) + boost;
    let force = if w.get::<GrowAt>(e).is_some() { 1.0 } else { 2.5 };
    f64::from(factor) * force
}

/// `Goat.dropHorn`: an adult that still has a part to lose snaps one off (the left or the right by a coin when both are there),
/// which falls from it with a push. Returns the break sound to play, or None when nothing came off.
fn drop_horn(w: &mut World, e: Entity, pos: Vec3) -> Option<&'static str> {
    let def = shed_of(w, e)?;
    if w.get::<GrowAt>(e).is_some() {
        return None;
    }
    let lost = w.get::<Shed>(e).map_or(0, |s| s.0);
    if lost == 3 {
        return None;
    }
    let left = match lost {
        0 => with_rng(w, e, |r| r.next_u32() & 1 == 1),
        1 => false,
        _ => true,
    };
    let (vx, vy, vz) = with_rng(w, e, |r| {
        let mut between = |min: f64, max: f64| r.next_f64() * (max - min) + min;
        (between(-0.2, 0.2), between(0.3, 0.7), between(-0.2, 0.2))
    });
    let bit = if left { 1 } else { 2 };
    w.entity_mut(e).insert(Shed(lost | bit));
    // `Goat.createHorn`: the instrument is drawn from a random seeded with the hash of the goat's UUID (here its `Uid`, folded).
    let uid = w.get::<Uid>(e).map_or(0, |u| u.0);
    let mut random = horizon_core::random::LegacyRandom::new(i64::from(((uid >> 32) as i32) ^ (uid as i32)));
    let pick = random.next_int_bound(def.items.len() as i32).unwrap_or(0) as usize;
    let stack = ItemStack::new(def.items[pick], 1);
    crate::item::drop_item(w, stack, pos, Vec3::new(vx, vy, vz), 10);
    Some(def.break_sound)
}

/// Le premier joueur, puis la première créature, de la boîte de la chèvre (vivants, ciblables, autre espèce).
fn rammed(w: &World, e: Entity) -> Option<Who> {
    let bounds = w.get::<Bounds>(e)?.0;
    let pos = pos_of(w, e);
    let mut near = Vec::new();
    w.resource::<Online>().for_near(pos.x, pos.z, 3.0, |p| near.push(Who::Player(p.id)));
    w.resource::<Index>().for_radius(pos.x, pos.z, 3.0, |c| near.push(Who::Entity(c)));
    near.into_iter().find(|&who| {
        ram_target_ok(w, e, who) && targeting::info(w, who).is_some_and(|i| !i.exempt && i.bounds.intersects(bounds))
    })
}

/// `RamTarget.hasRammedHornBreakingBlock` : le bloc devant (ou celui au-dessus) porte `snaps_goat_horn`.
fn rammed_horn_block(w: &World, e: Entity, pos: Vec3) -> bool {
    let mw = level_mw(w);
    let vel = w.get::<Vel>(e).map_or(Vec3::ZERO, |v| v.0);
    let ahead = Vec3::new(vel.x, 0.0, vel.z).normalize();
    let at = block_of(pos.add_vector(ahead));
    let snaps = |p: [i32; 3]| mw.has_tag("minecraft:snaps_goat_horn", mw.block_name(p[0], p[1], p[2]));
    snaps(at) || snaps([at[0], at[1] + 1, at[2]])
}

/// `RamTarget.finishRam` : la cible est quittée, le compte à rebours tiré dans `RAMS` part.
fn finish_ram(w: &mut World, e: Entity) {
    if w.get::<Rammer>(e).is_none() {
        return;
    }
    lower(w, e, false);
    let range = rams(w, e);
    let t = with_rng(w, e, |r| sample(r, range));
    set_cooldown(w, e, t);
    if let Some(mut r) = w.get_mut::<Rammer>(e) {
        r.ram_target = None;
        r.phase = Phase::Done;
    }
}

/// `Look` : `LookControl.setLookAt` (même effet que `goals::aim`, sans `Commands`).
pub(super) fn look(w: &mut World, e: Entity, at: [f64; 3]) {
    let looking = w.get::<Looking>(e).is_some();
    if let Some(mut l) = w.get_mut::<Look>(e) {
        l.target = at;
        l.cooldown = 2;
        l.y_speed = l.y_default;
    }
    if !looking {
        w.entity_mut(e).insert(Looking);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;
    use crate::targeting::Who;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 2, &Params { cooldown: (50, 50), ..Params::DEFAULT })];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A goat (a cow with the ram alone) at x = 0.5 and a prey cow at `px`, the prey as the goat's target.
    fn scene(h: &mut Herd, px: f64) -> (Entity, Entity) {
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let cow = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        let mut goat = cow;
        goat.name = "minecraft:goat";
        goat.goals = Config { entries: &ENTRIES };
        let mut prey_sp = *table.iter().find(|s| s.name == "minecraft:pig").unwrap();
        prey_sp.goals = Config { entries: &[] };
        let g = spawn_species(&mut h.world, &goat, 0.5, 0.5, 51);
        let p = spawn_species(&mut h.world, &prey_sp, px, 0.5, 52);
        h.world.flush();
        for e in [g, p] {
            h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        }
        h.world.get_mut::<AttackDamage>(g).unwrap().0 = 2.0;
        h.world.get_mut::<Health>(p).unwrap().0 = 10.0;
        crate::goals::arm_timers(&mut h.world, g);
        (g, p)
    }

    /// Un compte à rebours échu : la chèvre part vers un départ (le but tourne), prend une case de départ à 4 à 7 blocs, et
    /// la charge commence après 20 ticks de tête baissée.
    #[test]
    fn a_goat_starts_its_ram_when_the_cooldown_is_over_and_charges_after_the_wait() {
        let Some(mut h) = herd() else { return };
        let (g, _p) = scene(&mut h, 6.5);
        let mut started = None;
        for t in 1..=120 {
            h.tick();
            if running(&h, g) {
                started = Some(t);
                break;
            }
        }
        let started = started.expect("the ram starts when its cooldown (50 ticks) is over");
        assert!((45..=60).contains(&started), "the cooldown is 50 ticks: started at {started}");
        let mut charged = false;
        for _ in 0..200 {
            h.tick();
            if h.world.get::<Rammer>(g).unwrap().phase == Phase::Ram {
                charged = true;
                break;
            }
        }
        assert!(charged, "after the 20 ticks of lowered head the goat charges");
    }

    /// Un coéquipier n'est pas une cible : la chèvre et sa proie dans une même équipe, le bélier ne part jamais et la vie de la
    /// proie reste pleine (la règle du Mob : `p.allies.contains`, `goat.rs:491`).
    #[test]
    fn a_goat_does_not_ram_its_team_mate() {
        let Some(mut h) = herd() else { return };
        let (g, p) = scene(&mut h, 6.5);
        crate::bridge::teams::set_team(&mut h.world, "herd", &[Who::Entity(g), Who::Entity(p)]);
        for _ in 0..120 {
            h.tick();
            assert!(!running(&h, g), "the goat never starts a ram against its team mate");
        }
        assert_eq!(h.world.get::<Health>(p).unwrap().0, 10.0, "the team mate is not hit");
    }

    /// La charge touche la cible : des dégâts partent à la cible (témoin : sa vie baisse), le but finit aussitôt, et le
    /// compte à rebours du bélier repart après la charge (ici 50 ticks, la configuration de test).
    #[test]
    fn a_charge_hits_its_target_and_starts_a_new_cooldown() {
        let Some(mut h) = herd() else { return };
        let (g, p) = scene(&mut h, 6.5);
        for _ in 0..400 {
            h.tick();
            if h.world.get::<Health>(p).unwrap().0 < 10.0 {
                break;
            }
        }
        assert!(h.world.get::<Health>(p).unwrap().0 < 10.0, "the charge hits the prey: its health fell");
        let now = now_of(&h.world, g);
        let date = h.world.get::<RamCooldown>(g).unwrap().0;
        assert!(date > now && date - now <= 51, "a new cooldown of 50 ticks runs after the charge: {}", date as i64 - now as i64);
        for _ in 0..3 {
            h.tick();
        }
        assert!(!running(&h, g), "the goal ends after the hit");
    }

    /// La cible sort de la portée de départ : le but finit, le compte à rebours minimal part (témoin : la cible vivante
    /// retient le but jusque-là).
    #[test]
    fn the_ram_stops_when_its_target_is_gone() {
        let Some(mut h) = herd() else { return };
        let (g, p) = scene(&mut h, 6.5);
        for _ in 0..200 {
            h.tick();
            if running(&h, g) {
                break;
            }
        }
        assert!(running(&h, g), "the goal runs with a living target");
        h.world.get_mut::<Health>(p).unwrap().0 = 0.0;
        for _ in 0..5 {
            h.tick();
        }
        assert!(!running(&h, g), "a dead target ends the goal");
    }
}
