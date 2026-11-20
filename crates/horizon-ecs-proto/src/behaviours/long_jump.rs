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
    /// A start from this block found no spot to leap to (every candidate is walkable or has no arc): the search is not repeated from it before the
    /// date (the `Mob` repeats it at each start: up to 400 path searches; deviation, a block placed meanwhile is seen at the date).
    futile: Option<([i32; 3], u64)>,
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
/// How long a start from a block that found nothing is not repeated from it.
const FUTILE_FOR: u64 = 600;
const MID_JUMP_DURATION: u64 = 100;
/// `JUMP_VELOCITY_MULTIPLIER` (f32 dans le `Mob`).
const JUMP_VELOCITY_MULTIPLIER: f32 = 3.571_428_8;

fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(LongJumpState { slot: inst.slot, p, ready_at: 0, armed: None, futile: None });
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
        if ram.0 <= now {
            // The ram is on: the activity wakes when the ram sets its cooldown (`ram::set_cooldown` wakes this goal), no polling.
            return false;
        }
        if navigating(w, e) {
            // Repli (GUIDE règle 4) : aucun événement ne dit quand la navigation finit : une redemande à +2, un seul fil par animal.
            schedule(w, e, now + 2);
            return false;
        }
    }
    let p = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let block = block_of(p);
    let on_ground = w.get::<Contact>(e).is_some_and(|c| c.on_ground);
    let honey = level_mw(w).block_name(block[0], block[1], block[2]) == "minecraft:honey_block";
    let mut can = on_ground && !fluids.in_water && !fluids.in_lava && !honey;
    if can && st.futile.is_some_and(|(b, until)| b == block && now < until) {
        can = false;
    }
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
    let Some(leap) = w.get::<Leap>(e) else { return };
    let pos = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let in_water = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
    let valid = leap.initial == Some(pos) && leap.tries > 0 && !in_water && !(leap.chosen.is_none() && leap.candidates.is_empty());
    // Only a search over all the candidates proves that there is nothing (with a wish for a preferred block the first draw empties the list).
    let found_nothing = leap.initial == Some(pos) && leap.chosen.is_none() && leap.candidates.is_empty() && !leap.wanting_preferred;
    if !valid {
        if found_nothing {
            let now = now_of(w, e);
            if let Some(mut st) = w.get_mut::<LongJumpState>(e) {
                st.futile = Some((block_of(pos), now + FUTILE_FOR));
            }
        }
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
    // Moved out and back (the 400 candidates of a box are not copied at each tick).
    let Some(mut leap) = w.get_mut::<Leap>(e).map(|mut l| std::mem::take(&mut *l)) else { return };
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
                    *c = std::mem::take(&mut leap);
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
        // `create_path_raw(&[target], 0, 8.0).reached` (`frog.rs:860-866`) : une case qu'un chemin de 8 blocs atteint se marche, pas de bond.
        if walkable(w, e, &p, target) {
            continue;
        }
        leap.chosen = Some(v);
        leap.prepare = 0;
        return;
    }
}

/// Whether a path of at most 8 blocks reaches the block (`Mob.create_path_raw(&[target], 0, 8.0).reached`): the search `pathfind` makes, without
/// touching the animal. The frog's is the amphibious one that prefers shallow water (`nav.rs:20`), the goat's the walking one.
fn walkable(w: &World, e: Entity, p: &Params, target: [i32; 3]) -> bool {
    use horizon_client_lib::mobs::path::{Evaluator, Finder, PathMob};
    let (Some(pos), Some(body), Some(c), Some(sp)) = (w.get::<Pos>(e), w.get::<Body>(e), w.get::<Contact>(e), w.get::<NavParams>(e)) else { return false };
    let in_fluid = w.get::<FluidState>(e).is_some_and(|f| f.in_water);
    if !c.on_ground && !in_fluid {
        return false;
    }
    let pm = PathMob {
        position: pos.0,
        width: body.width,
        height: body.height,
        on_ground: c.on_ground,
        in_water: in_fluid,
        max_up_step: body.step_height,
        max_fall_distance: crate::cow::comfortable_fall_distance(0.0),
        malus: sp.malus,
        can_float: sp.can_float,
        can_open_doors: sp.can_open_doors,
        can_pass_doors: sp.can_pass_doors,
        can_walk_over_fences: sp.can_walk_over_fences,
        stands_on_lava: false,
    };
    crate::goals::FINDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let evaluator = if p.swim_rules { Evaluator::Amphibious { prefers_shallow: true } } else { Evaluator::Walk };
    Finder::new(w.resource::<Level>().mw.0, &pm, evaluator).find(&[target], 8.0, 0, (sp.follow_range * 16.0).floor() as i32).is_some_and(|path| path.reached)
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
    let angles = crate::jump_arc::shuffle(ALLOWED_ANGLES, |i| with_rng(w, e, |r| r.next_int(i)));
    let max = (p.jump_strength * f64::from(JUMP_VELOCITY_MULTIPLIER)) as f32;
    let (pos, body) = (w.get::<Pos>(e)?.0, *w.get::<Body>(e)?);
    // The arc is tested with the standing box (fallback: the pose's box is not read).
    crate::jump_arc::first_reaching(&angles, pos, body.gravity, target, max, Some((mw, crate::jump_arc::Footprint { width: body.width, height: body.height })))
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
        // The animal stands on a pillar in a moat 5 blocks deep and 2 wide: no path of 8 blocks reaches the ground around (the `Mob`
        // refuses to leap where it can walk, `frog.rs:860-866`).
        for x in -2..=2 {
            for z in -2..=2 {
                if (x, z) != (0, 0) {
                    for y in -4..=0 {
                        h.set_block(x, y, z, "minecraft:air");
                    }
                }
            }
        }
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
        // What `ram::set_cooldown` does when it sets the date.
        crate::goals::wake(&mut h.world, e, &MODULE);
        let mut started = false;
        for _ in 0..200 {
            h.tick();
            started |= running(&h, e);
        }
        assert!(started, "while its ram cools the goat starts its long jump");
    }
}
