//! `FrogAi` / `GoatAi` long jump (fiches 97 et 98) : UN module, deux phases. `Preparing` : le but tire une case de sa boîte
//! qu'il ne peut pas atteindre à pied, mais qu'un arc de 65 à 80 degrés atteint sans toucher de bloc, la regarde 40 ticks,
//! puis bondit. `Airborne` : la vitesse de l'arc, le frottement coupé, jusqu'à l'atterrissage ou 100 ticks. Le compte à
//! rebours est une échéance de la roue en temps propre ; le but qui court se compte par `tick`.
//! Replis (fiches 97 et 98, à faire lire au contrôleur) : pas de sonde de chemin (toute case est traitée comme non
//! marchable), hasard de l'animal (pas celui du niveau), boîte de pose debout, pas de bonus de saut, frottement par le
//! drapeau `Drive.discard_friction`, `interruptable` constant (vrai), pas de condition de tentation ni d'amour.
use super::*;
use crate::cow::*;
use crate::goals::*;
use horizon_client_lib::blocks::Fluid;
use horizon_client_lib::mobs::MobWorld;
use horizon_client_lib::mobs::path::PathType;
use horizon_core::geometry::{Aabb, Vec3, trig};
use horizon_core::jmath;

/// `LongJumpToPreferredBlock` / `LongJumpToRandomPos` : les nombres de la grenouille (`Params::DEFAULT`) et de la chèvre
/// (`prefer` à None, boîte 5 x 5, cooldown 600..1200, `gated_by_ram`).
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `TIME_BETWEEN_LONG_JUMPS` : le compte à rebours après un bond (uniforme, bornes comprises), divisé par 1 ; divisé par 2 après un refus.
    pub cooldown: (i32, i32),
    /// Les demi-boîtes des cases candidates (`MAX_LONG_JUMP_WIDTH`, `MAX_LONG_JUMP_HEIGHT`).
    pub width: i32,
    pub height: i32,
    /// `jump_strength` de l'espèce (`resources/entity-types.json`, 0,41999998688697815 pour la grenouille et la chèvre).
    pub jump_strength: f64,
    /// Le tag d'un bloc d'atterrissage préféré et sa chance d'être voulu (`PREFERRED_BLOCKS_CHANCE`) ; None : pas de tirage.
    pub preferred: Option<(&'static str, f64)>,
    /// Les règles de l'animal qui vit aussi dans l'eau (`FrogAi`) : pas de départ dans l'eau, atterrissage refusé près d'un fluide.
    pub swim_rules: bool,
    /// L'activité ne vaut que pendant le compte à rebours du bélier (`GoatAi.longJumpActivity`, lu dans `ram::RamCooldown`).
    pub gated_by_ram: bool,
}
impl Params {
    pub const DEFAULT: Self = Self {
        cooldown: (100, 140),
        width: 4,
        height: 2,
        jump_strength: 0.419_999_986_886_978_15,
        preferred: Some(("minecraft:frog_prefer_jump_to", 0.5)),
        swim_rules: true,
        gated_by_ram: false,
    };
}

/// `LONG_JUMP_COOLDOWN` et la boîte de la phase de préparation, par entrée : `ready_at` est la date (temps propre) à partir de
/// laquelle le but peut être demandé ; `armed` : la date de l'échéance déjà armée (une seule par date).
#[derive(Component, Clone, Copy, Debug)]
pub struct LongJumpState {
    slot: u8,
    p: Params,
    ready_at: u64,
    armed: Option<u64>,
}

/// Le choix de la phase de préparation (`LongJumpToRandomPos` et `LongJumpToPreferredBlock`).
#[derive(Component, Clone, Debug, Default)]
pub struct Leap {
    candidates: Vec<([i32; 3], i32)>,
    not_preferred: Vec<([i32; 3], i32)>,
    wanting_preferred: bool,
    initial: Option<Vec3>,
    chosen: Option<Vec3>,
    look: [f64; 3],
    tries: i32,
    prepare: i32,
}

/// Le but tourne en préparation (son `tick` le visite).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Preparing;
/// Le saut est parti : `since` est le temps propre du bond (`LongJumpMidJump`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Airborne {
    pub since: u64,
}

pub static MODULE: Module = Module {
    name: "long_jump",
    flags: MOVE | LOOK,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    // `interruptable = !mid_jump` (`frog.rs:924-926`) : constante ici (vrai), BESOIN DU CADRE.
    interruptable: true,
    ..Module::DEFAULT
};

const ALLOWED_ANGLES: [i32; 4] = [65, 70, 75, 80];
const FIND_JUMP_TRIES: i32 = 20;
const PREPARE_JUMP_DURATION: i32 = 40;
const MID_JUMP_DURATION: u64 = 100;
/// `JUMP_VELOCITY_MULTIPLIER` (f32 dans le `Mob`).
const JUMP_VELOCITY_MULTIPLIER: f32 = 3.571_428_8;

fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(LongJumpState { slot: inst.slot, p, ready_at: 0, armed: None });
}

fn sample(r: &mut Rng, (lo, hi): (i32, i32)) -> i32 {
    lo + r.next_int(hi - lo + 1)
}

/// Demande le but à la date `date` (temps propre) : une échéance de la roue, une seule par date.
fn schedule(w: &mut World, e: Entity, date: u64) {
    let now = now_of(w, e);
    let date = date.max(now + 1);
    let Some(mut st) = w.get_mut::<LongJumpState>(e) else { return };
    st.ready_at = date;
    if st.armed == Some(date) {
        return;
    }
    st.armed = Some(date);
    let slot = st.slot;
    drop(st);
    arm_at(w, e, date, Due::Goal(slot));
}

/// `CountDownCooldownTicks` du `LONG_JUMP_COOLDOWN` : un compte à rebours uniforme, divisé par `div`.
fn sample_cooldown(w: &mut World, e: Entity, div: i32) {
    let Some(p) = w.get::<LongJumpState>(e).map(|s| s.p) else { return };
    let t = with_rng(w, e, |r| sample(r, p.cooldown)) / div;
    let date = now_of(w, e) + t.max(0) as u64;
    schedule(w, e, date);
}

/// Premier compte à rebours de l'entrée (`finalize` : uniforme `[100, 140]` pour la grenouille).
fn arm(w: &mut World, e: Entity, _: Inst) {
    sample_cooldown(w, e, 1);
}

/// `LongJumpToPreferredBlock` / `GoatAi` : une échéance de la roue ; le but demande `canUse` aux ticks de sélecteur seulement.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    if let Some(mut st) = w.get_mut::<LongJumpState>(e)
        && st.armed == Some(now)
    {
        st.armed = None;
    }
    // Le but qui court se compte par son `tick` : une échéance qui arrive pendant le but ne le démarre pas.
    if w.get::<Preparing>(e).is_some() || w.get::<Airborne>(e).is_some() {
        return false;
    }
    if !selector_tick(w, e) {
        schedule(w, e, now + 1);
        return false;
    }
    true
}

fn selector_tick(w: &World, e: Entity) -> bool {
    let now = now_of(w, e);
    now <= 1 || (now + phase_of(w, e)) % 2 == 0
}

/// Le but navigue (chemin demandé ou en cours) : `Mob.ai.nav.isDone()` faux.
fn navigating(w: &World, e: Entity) -> bool {
    w.get::<Following>(e).is_some() || w.get::<NeedsPath>(e).is_some()
}

/// `LongJumpToRandomPos.canUse` et l'activité (`FrogAi`, `GoatAi.longJumpActivity`) : pas de départ dans l'eau pour la grenouille,
/// activité du bélier pour la chèvre ; puis sur le sol, pas dans l'eau ni la lave ni sur le miel (sinon compte à rebours / 2).
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    let Some(st) = w.get::<LongJumpState>(e).copied() else { return false };
    if now < st.ready_at {
        schedule(w, e, st.ready_at);
        return false;
    }
    let fluids = w.get::<FluidState>(e).copied().unwrap_or_default();
    if st.p.swim_rules && fluids.in_water {
        return false;
    }
    if st.p.gated_by_ram {
        let Some(ram) = w.get::<crate::behaviours::ram::RamCooldown>(e).copied() else { return false };
        if ram.0 <= now || navigating(w, e) {
            // Repli (GUIDE règle 4) : la condition de fond est vraie (le compte à rebours du bond est écoulé), aucun événement
            // ne dit quand le bélier finit son compte à rebours : une redemande à +2, un seul fil par animal.
            schedule(w, e, now + 2);
            return false;
        }
    }
    let p = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let block = block_of(p);
    let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    let honey = level_mw(w).block_name(block[0], block[1], block[2]) == "minecraft:honey_block";
    let can = on_ground && !fluids.in_water && !fluids.in_lava && !honey;
    if !can {
        sample_cooldown(w, e, 2);
    }
    can
}

fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

fn level_mw(w: &World) -> &'static MobWorld<'static> {
    w.resource::<Level>().mw.0
}

/// `LongJumpToRandomPos.start` : la boîte des candidates, la préparation, et le tirage « préféré » ; puis le tick du
/// même tick (GUIDE règle 9.1).
fn start(w: &mut World, e: Entity, _: Inst) {
    let Some(st) = w.get::<LongJumpState>(e).copied() else { return };
    let pos = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let mut leap = Leap {
        candidates: candidates(block_of(pos), st.p.width, st.p.height),
        tries: FIND_JUMP_TRIES,
        initial: Some(pos),
        ..Leap::default()
    };
    if let Some((_, chance)) = st.p.preferred {
        leap.wanting_preferred = with_rng(w, e, |r| r.next_f64() < chance);
    }
    w.entity_mut(e).insert((leap, Preparing));
    prepare_tick(w, e);
}

/// `LongJumpToRandomPos.stop` / `LongJumpMidJump.stop` : l'atterrissage ralentit les pas (vitesse x et z par 0,1),
/// le compte à rebours part (divisé par 1 après un bond, par 2 pendant la préparation).
fn stop(w: &mut World, e: Entity, _: Inst) {
    let now = now_of(w, e);
    let airborne = w.get::<Airborne>(e).copied();
    let was_preparing = w.get::<Preparing>(e).is_some();
    w.entity_mut(e).remove::<(Preparing, Airborne, Leap)>();
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.discard_friction = false;
    }
    if let Some(_) = airborne {
        if w.get::<Contact>(e).is_some_and(|c| c.on_ground)
            && let Some(v) = w.get::<Vel>(e).map(|v| v.0)
        {
            set_velocity(w, e, Vec3::new(v.x * f64::from(0.1_f32), v.y, v.z * f64::from(0.1_f32)));
        }
        sample_cooldown(w, e, 1);
    } else if was_preparing {
        // Interrompu pendant la préparation sans refus : aucun compte à rebours ; le sélecteur redemande (`Mob` : cooldown absent).
        let armed_pending = w.get::<LongJumpState>(e).and_then(|s| s.armed).is_some_and(|d| d > now);
        if !armed_pending {
            schedule(w, e, now + 2);
        }
    }
}

/// Le `tick` du module, appelé à chaque tick tant qu'un animal le porte : la préparation et le vol.
fn tick(w: &mut World) {
    for e in crate::goals::entities::<With<Preparing>>(w) {
        run_preparing(w, e);
    }
    for e in crate::goals::entities::<With<Airborne>>(w) {
        run_airborne(w, e);
    }
}

/// `LongJumpToRandomPos.canContinue` avant le saut : même position, des essais restants, pas dans l'eau, une case ou un tirage.
fn run_preparing(w: &mut World, e: Entity) {
    let Some(leap) = w.get::<Leap>(e).cloned() else { return };
    let pos = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
    let valid = leap.initial == Some(pos) && leap.tries > 0 && !in_water && !(leap.chosen.is_none() && leap.candidates.is_empty());
    if !valid {
        sample_cooldown(w, e, 2);
        if let Some(slot) = w.get::<LongJumpState>(e).map(|s| s.slot) {
            w.resource_mut::<Requests>().0.push(Req::End(e, slot));
        }
        return;
    }
    prepare_tick(w, e);
}

/// `LongJumpToRandomPos.tick` (préparation) : tirer une case, ou regarder la case choisie et sauter après 40 ticks.
fn prepare_tick(w: &mut World, e: Entity) {
    let Some(mut leap) = w.get::<Leap>(e).cloned() else { return };
    match leap.chosen {
        None => {
            leap.tries -= 1;
            pick(w, e, &mut leap);
        }
        Some(jump) => {
            look(w, e, leap.look);
            leap.prepare += 1;
            if leap.prepare >= PREPARE_JUMP_DURATION {
                if let Some(mut c) = w.get_mut::<Leap>(e) {
                    *c = leap.clone();
                }
                leap_now(w, e, jump);
                return;
            }
        }
    }
    if let Some(mut c) = w.get_mut::<Leap>(e) {
        *c = leap;
    }
}

/// `LongJumpToRandomPos.pickCandidate` : la première case acceptable qu'un arc atteint. Le test « atteignable à pied en 8 blocs »
/// est absent (repli : toute case est non marchable) : la première case acceptable avec un arc est retenue.
fn pick(w: &mut World, e: Entity, leap: &mut Leap) {
    let Some(p) = w.get::<LongJumpState>(e).map(|s| s.p) else { return };
    let mw = level_mw(w);
    let pos = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    while !leap.candidates.is_empty() {
        let Some((target, _)) = next_candidate(w, e, leap, &p, mw) else { continue };
        let here = [pos.x.floor() as i32, pos.z.floor() as i32];
        let acceptable = if p.swim_rules { acceptable_landing(w, e, &p, mw, target) } else { default_landing(w, e, mw, target) };
        if here == [target[0], target[2]] || !acceptable {
            continue;
        }
        let centre = Vec3::new(f64::from(target[0]) + 0.5, f64::from(target[1]) + 0.5, f64::from(target[2]) + 0.5);
        let Some(v) = optimal_jump_vector(w, e, &p, mw, centre) else { continue };
        leap.look = [centre.x, centre.y, centre.z];
        // Repli : `create_path_raw(&[target], 0, 8.0).reached` est absent : la case n'est jamais marchable.
        leap.chosen = Some(v);
        leap.prepare = 0;
        return;
    }
}

/// `LongJumpToPreferredBlock.getJumpCandidate` : tant qu'il en veut, la première case tirée sur un bloc préféré, sinon la
/// première passée sur le côté.
fn next_candidate(w: &mut World, e: Entity, leap: &mut Leap, p: &Params, mw: &MobWorld) -> Option<([i32; 3], i32)> {
    if !leap.wanting_preferred {
        return draw(w, e, leap);
    }
    while !leap.candidates.is_empty() {
        if let Some(c) = draw(w, e, leap) {
            let [x, y, z] = c.0;
            if p.preferred.is_some_and(|(tag, _)| mw.has_tag(tag, mw.block_name(x, y - 1, z))) {
                return Some(c);
            }
            leap.not_preferred.push(c);
        }
    }
    (!leap.not_preferred.is_empty()).then(|| leap.not_preferred.remove(0))
}

/// `WeightedRandom.getRandomItem` sur les candidates, retirée une fois tirée (hasard de l'animal : repli).
fn draw(w: &mut World, e: Entity, leap: &mut Leap) -> Option<([i32; 3], i32)> {
    let total: i32 = leap.candidates.iter().map(|c| c.1).sum();
    if total == 0 {
        return None;
    }
    let mut pick = with_rng(w, e, |r| r.next_int(total));
    let i = leap.candidates.iter().position(|c| {
        pick -= c.1;
        pick < 0
    })?;
    Some(leap.candidates.remove(i))
}

/// `FrogAi.isAcceptableLandingSpot` : pas de fluide dans les trois cases, une case préférée (ou au-dessus), une trappe, sinon
/// le défaut.
fn acceptable_landing(w: &World, e: Entity, p: &Params, mw: &MobWorld, [x, y, z]: [i32; 3]) -> bool {
    if (y - 1..=y + 1).any(|yy| mw.fluid(x, yy, z) != Fluid::None) {
        return false;
    }
    let preferred = |yy: i32| p.preferred.is_some_and(|(tag, _)| mw.has_tag(tag, mw.block_name(x, yy, z)));
    if preferred(y) || preferred(y - 1) {
        return true;
    }
    if mw.path_type_static(x, y, z) == PathType::Trapdoor
        || mw.facts.is_air(mw.world.block(x, y, z)) && mw.path_type_static(x, y - 1, z) == PathType::Trapdoor
    {
        return true;
    }
    default_landing(w, e, mw, [x, y, z])
}

/// `LongJumpToRandomPos.defaultAcceptableLandingSpot` : un bloc plein dessous et un type de chemin sans malus.
fn default_landing(w: &World, e: Entity, mw: &MobWorld, [x, y, z]: [i32; 3]) -> bool {
    let t = mw.path_type_static(x, y, z);
    let malus = w.get::<NavParams>(e).map_or(t.malus(), |n| n.malus[t as usize].unwrap_or_else(|| t.malus()));
    mw.facts.get(mw.world.block(x, y - 1, z)).solid_render && malus == 0.0
}

/// `LongJumpToRandomPos.start` : les cases de la boîte sauf celle de l'animal, pondérées par le carré de la distance.
fn candidates([x, y, z]: [i32; 3], width: i32, height: i32) -> Vec<([i32; 3], i32)> {
    let mut out = Vec::new();
    for dz in -width..=width {
        for dy in -height..=height {
            for dx in -width..=width {
                if (dx, dy, dz) != (0, 0, 0) {
                    out.push(([x + dx, y + dy, z + dz], dx * dx + dy * dy + dz * dz));
                }
            }
        }
    }
    out
}

/// `LongJumpToRandomPos.calculateOptimalJumpVector` : les angles mélangés, le premier arc qui atteint la cible sans bloc.
fn optimal_jump_vector(w: &mut World, e: Entity, p: &Params, mw: &MobWorld, target: Vec3) -> Option<Vec3> {
    let mut angles = ALLOWED_ANGLES;
    for i in (2..=angles.len()).rev() {
        let j = with_rng(w, e, |r| r.next_int(i as i32)) as usize;
        angles.swap(i - 1, j);
    }
    let max = (p.jump_strength * f64::from(JUMP_VELOCITY_MULTIPLIER)) as f32;
    angles.iter().find_map(|&a| jump_vector(w, e, mw, target, max, a))
}

/// `LongJumpUtil.calculateJumpVectorForAngle` avec la vérification de collision de l'arc (`breeze.rs:117-190`).
fn jump_vector(w: &World, e: Entity, mw: &MobWorld, target: Vec3, max_velocity: f32, angle: i32) -> Option<Vec3> {
    let pos = w.get::<Pos>(e)?.0;
    let body = *w.get::<Body>(e)?;
    let plane = Vec3::new(target.x - pos.x, 0.0, target.z - pos.z).normalize().scale(0.5);
    let aim = target.subtract(plane);
    let d = aim.subtract(pos);
    let angrad = angle as f32 * std::f32::consts::PI / 180.0;
    let xz = trig::fdlibm_atan2(d.z, d.x);
    let r2 = d.x * d.x + 0.0 + d.z * d.z;
    let r = r2.sqrt();
    let y = d.y;
    let g = body.gravity;
    let sin2 = jmath::sin(f64::from(2.0 * angrad));
    let cos_sq = jmath::pow(jmath::cos(f64::from(angrad)), 2.0);
    let (sin, cos) = (jmath::sin(f64::from(angrad)), jmath::cos(f64::from(angrad)));
    let v0sq = r2 * g / (r * sin2 - 2.0 * y * cos_sq);
    if v0sq < 0.0 {
        return None;
    }
    let v0 = v0sq.sqrt();
    if v0 > f64::from(max_velocity) {
        return None;
    }
    let (v0r, v0y) = (v0 * cos, v0 * sin);
    // `ceil(r / v0r) * 2` points along the arc, each step clear of blocks (`clear_transition`, boîte debout : repli).
    let samples = (r / v0r).ceil() as i32 * 2;
    let (sin_xz, cos_xz) = (jmath::sin(xz), jmath::cos(xz));
    let mut ri = 0.0;
    let mut previous: Option<Vec3> = None;
    for _ in 0..samples - 1 {
        ri += r / f64::from(samples);
        let yi = sin / cos * ri - jmath::pow(ri, 2.0) * g / (2.0 * v0sq * jmath::pow(cos, 2.0));
        let p = Vec3::new(pos.x + ri * cos_xz, pos.y + yi, pos.z + ri * sin_xz);
        if previous.is_some_and(|q| !clear_transition(mw, body.width, body.height, q, p)) {
            return None;
        }
        previous = Some(p);
    }
    Some(Vec3::new(v0r * jmath::cos(xz), v0y, v0r * jmath::sin(xz)).scale(f64::from(0.95_f32)))
}

/// `LongJumpUtil.isClearTransition` : la boîte tient aux points espacés de neuf dixièmes de son petit côté.
fn clear_transition(mw: &MobWorld, width: f32, height: f32, from: Vec3, to: Vec3) -> bool {
    let (width, height) = (f64::from(width), f64::from(height));
    let d = to.subtract(from);
    let min = width.min(height);
    let checks = (d.length() / min).ceil() as i32;
    let step = d.normalize().scale(min * f64::from(0.9_f32));
    let half = width / 2.0;
    let mut p = from;
    (0..checks).all(|i| {
        p = if i == checks - 1 { to } else { p.add_vector(step) };
        mw.no_collision(Aabb::new(Vec3::new(p.x - half, p.y, p.z - half), Vec3::new(p.x + half, p.y + height, p.z + half)))
    })
}

/// `LongJumpMidJump` : le bond (vitesse de l'arc, frottement coupé, le corps tourné vers son cap), puis le vol.
fn leap_now(w: &mut World, e: Entity, jump: Vec3) {
    let now = now_of(w, e);
    let len = jump.length();
    // `jump_boost` (repli : pas de bonus lu, DOUTE) : boost 0.
    let boost = 0.0;
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.discard_friction = true;
    }
    if let Some(h) = w.get::<Head>(e).map(|h| h.body)
        && let Some(mut y) = w.get_mut::<Yaw>(e)
    {
        y.0 = h;
    }
    set_velocity(w, e, jump.scale((len + boost) / len));
    w.entity_mut(e).remove::<(Preparing, Leap)>().insert(Airborne { since: now });
}

/// `LongJumpMidJump.canContinue` : tant que le corps est en l'air (la grenouille hors de l'eau), 100 ticks au plus.
fn run_airborne(w: &mut World, e: Entity) {
    let Some(a) = w.get::<Airborne>(e).copied() else { return };
    let now = now_of(w, e);
    let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    let Some(st) = w.get::<LongJumpState>(e).copied() else { return };
    let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
    let landed = now > a.since && on_ground;
    let swam = st.p.swim_rules && in_water;
    if landed || swam || now >= a.since + MID_JUMP_DURATION {
        w.resource_mut::<Requests>().0.push(Req::End(e, st.slot));
    }
}

/// `Look` : `LookControl.setLookAt` (sans `Commands` : le même effet que `goals::aim`).
fn look(w: &mut World, e: Entity, at: [f64; 3]) {
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

    /// La grenouille avec la marche seule de l'espèce, le but injecté par sa configuration de test.
    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 3, &Params { preferred: None, ..Params::DEFAULT })];
    /// Une chèvre d'essai : la copie de la vache avec les paramètres de la chèvre, un cooldown court pour le test du drapeau.
    static GOAT_ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 2, &Params { cooldown: (10, 10), width: 5, height: 5, preferred: None, swim_rules: false, gated_by_ram: true, ..Params::DEFAULT })];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    fn species_named(h: &Herd, name: &str) -> Species {
        *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).expect("species of the test table")
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// A frog (or a goat built from a cow) on the slab, its goal alone, awake and armed.
    fn animal(h: &mut Herd, goat: bool) -> Entity {
        let (name, entries) = if goat { ("minecraft:goat", &GOAT_ENTRIES[..]) } else { ("minecraft:frog", &ENTRIES[..]) };
        let base = if goat { species_named(h, "minecraft:cow") } else { species_named(h, name) };
        let mut sp = base;
        sp.name = name;
        sp.goals = Config { entries };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 31);
        h.world.flush();
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        // The ram's cooldown is on the goat from its creation (the ram module's `equip`): ready, here.
        h.world.entity_mut(e).insert(crate::behaviours::ram::RamCooldown(0));
        crate::goals::arm_timers(&mut h.world, e);
        e
    }

    /// Un animal sans but à l'arrêt : la préparation se lance après le compte à rebours initial, dure 40 ticks, puis le bond
    /// emporte le corps vers le haut : témoin, la vitesse verticale et l'état `Airborne`.
    #[test]
    fn a_frog_starts_preparing_after_its_cooldown_and_leaps_after_forty_ticks() {
        let Some(mut h) = herd() else { return };
        let e = animal(&mut h, false);
        let mut started = None;
        for t in 1..=200 {
            h.tick();
            if running(&h, e) {
                started = Some(t);
                break;
            }
        }
        let started = started.expect("the frog starts a long jump within 200 ticks");
        assert!((95..=145).contains(&started), "the first cooldown is 100..140: started at {started}");
        let mut leapt = None;
        for t in 1..=60 {
            h.tick();
            if h.world.get::<Airborne>(e).is_some() {
                leapt = Some(t);
                break;
            }
        }
        let leapt = leapt.expect("the frog leaps once its preparation is over");
        assert!((38..=42).contains(&leapt), "forty ticks of preparation: leapt {leapt} ticks after the start");
        let v = h.world.get::<Vel>(e).unwrap().0;
        assert!(v.y > 0.0 || h.world.get::<Pos>(e).unwrap().0.y > 0.6, "the leap lifts the frog: vertical speed {}", v.y);
        assert!(h.world.get::<Drive>(e).unwrap().discard_friction, "the friction is discarded in the flight");
    }

    /// Pendant la préparation le but tient, le regard se pose sur la case choisie, et le corps reste en place.
    #[test]
    fn during_the_preparation_the_frog_looks_at_its_spot_and_stays_put() {
        let Some(mut h) = herd() else { return };
        let e = animal(&mut h, false);
        for _ in 0..200 {
            h.tick();
            if running(&h, e) {
                break;
            }
        }
        assert!(running(&h, e), "the goal runs");
        let start = h.world.get::<Pos>(e).unwrap().0;
        let mut looked = false;
        for _ in 0..30 {
            h.tick();
    
            looked |= h.world.get::<Leap>(e).is_some_and(|l| l.look != [0.0; 3]);
            assert!(h.world.get::<Airborne>(e).is_none(), "no leap before the 40 ticks");
        }
        let p = h.world.get::<Pos>(e).unwrap().0;
        assert!(looked, "the chosen spot is set as the look target of the preparation");
        assert!((p.x - start.x).abs() < 0.05 && (p.z - start.z).abs() < 0.05, "the frog stays put while it prepares: {p:?} from {start:?}");
        assert!(running(&h, e), "the goal still runs");
    }

    /// Le vol s'arrête à l'atterrissage : le but finit, le frottement revient, un nouveau compte à rebours part.
    #[test]
    fn the_flight_ends_at_the_landing_and_a_new_cooldown_starts() {
        let Some(mut h) = herd() else { return };
        let e = animal(&mut h, false);
        let mut landed_at = None;
        for t in 1..=400 {
            h.tick();
            let airborne = h.world.get::<Airborne>(e).is_some();
            if airborne && h.world.get::<Contact>(e).unwrap().on_ground && t > 2 {
                // Une vraie retombée : le corps touche le sol après l'air.
                landed_at = Some(t);
            }
            if landed_at.is_some() && !running(&h, e) {
                break;
            }
        }
        assert!(landed_at.is_some(), "the frog comes back to the ground after its leap");
        assert!(!running(&h, e), "the goal ends at the landing");
        assert!(h.world.get::<Airborne>(e).is_none(), "the flight state is gone");
        assert!(!h.world.get::<Drive>(e).unwrap().discard_friction, "the friction is back");
        let st = *h.world.get::<LongJumpState>(e).unwrap();
        assert!(st.ready_at > now_of(&h.world, e) + 40, "a new cooldown runs after the landing");
    }

    /// La chèvre d'essai ne saute que pendant le compte à rebours du bélier (`gated_by_ram`) : témoin, le même animal
    /// saute quand la date du bélier est future, et ne saute pas sans elle.
    #[test]
    fn a_goat_leaps_only_while_its_ram_cools() {
        let Some(mut h) = herd() else { return };
        let e = animal(&mut h, true);
        for _ in 0..100 {
            h.tick();
        }
        assert!(!running(&h, e), "without a ram cooldown the goat does not leap");
        h.world.entity_mut(e).insert(crate::behaviours::ram::RamCooldown(u64::MAX / 2));
        let mut started = false;
        for _ in 0..200 {
            h.tick();
            started |= running(&h, e);
        }
        assert!(started, "while its ram cools the goat starts its long jump");
    }
}
