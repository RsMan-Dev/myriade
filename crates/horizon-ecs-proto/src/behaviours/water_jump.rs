//! `Dolphin.DolphinJumpGoal` (fiche 109, `dolphin.rs:190-272`, enregistré priorité 5 par `aquatic.rs:177`) : le dauphin bondit
//! hors de l'eau quand les blocs devant lui (positions 0, 1, 4, 5, 6, 7 dans sa direction de nage) sont de l'eau libre, avec deux
//! blocs d'air au-dessus. Le but n'a pas de minuterie propre : `canUse` tire `nextInt(5)` à chaque demande, donc une attente
//! géométrique (`idle_wait`) armée à la place, puis une attente fraîche à chaque refus de géométrie et à chaque fin du but.
use super::*;
use crate::cow::*;
use crate::goals::*;
use horizon_client_lib::blocks::Fluid;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `Jump(interval)` garde `reduced(interval)` (`dolphin.rs:197-202`) : 5 pour l'intervalle 10 de `aquatic.rs:177`.
    pub interval: i32,
    /// Les décalages de blocs devant le dauphin qui doivent tous être de l'eau libre.
    pub offsets: &'static [i32],
    /// La poussée horizontale de départ, et la poussée verticale.
    pub push: f64,
    pub lift: f64,
    /// Seuil de vitesse verticale au carré (`0,03`) et limite de l'inclinaison (`10`) de `canContinue`.
    pub pitch_threshold: f64,
    pub pitch_limit: f32,
    /// Le retour de l'inclinaison vers 0 (`rot_lerp`, `dolphin.rs:265`).
    pub lerp: f32,
    /// Le tag des blocs que le dauphin ne saute pas (`dolphin.rs:216-230`).
    pub forbid_tag: &'static str,
}
impl Params {
    pub const DEFAULT: Self = Self {
        interval: 5,
        offsets: &[0, 1, 4, 5, 6, 7],
        push: 0.6,
        lift: 0.7,
        pitch_threshold: 0.03,
        pitch_limit: 10.0,
        lerp: 0.2,
        forbid_tag: "minecraft:blocks_dolphin_jump",
    };
}
/// L'état : les paramètres et le rang de l'entrée.
#[derive(Component, Clone, Copy, Debug)]
pub struct Leaper {
    p: Params,
    slot: u8,
}
/// Le but tourne.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Leaping;

pub static MODULE: Module = Module {
    name: "water_jump",
    flags: JUMP | MOVE,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    // `Dolphin.JumpGoal.isInterruptable` est faux (`dolphin.rs:209-211`).
    interruptable: false,
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Leaper { p, slot: inst.slot });
}
/// La première demande : une attente géométrique (le tirage `nextInt(5)` de chaque demande, rendu par la loi de l'attente).
fn arm(w: &mut World, e: Entity, inst: Inst) {
    schedule(w, e, inst);
}
fn schedule(w: &mut World, e: Entity, inst: Inst) {
    let n = w.get::<Leaper>(e).map_or(5, |l| l.p.interval);
    let wait = crate::goals::with_rng(w, e, |r| idle_wait(r, n));
    crate::goals::arm(w, e, inst.slot, wait);
}
/// L'échéance est le tirage réussi de `canUse` : la demande suit.
fn fired(_: &mut World, _: Entity, _: Inst) -> bool {
    true
}
/// `canUse` : la géométrie devant le dauphin ; un refus réarme une attente fraîche (la demande suivante refait le tirage).
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(p) = w.get::<Leaper>(e).map(|l| l.p) else { return false };
    if geometry_ok(w, e, &p) {
        return true;
    }
    schedule(w, e, inst);
    false
}
/// `start` : la poussée de départ selon la direction de nage, puis le tick de la même passe (`GoalSelector.tick`).
fn start(w: &mut World, e: Entity, _: Inst) {
    let Some(p) = w.get::<Leaper>(e).map(|l| l.p) else { return };
    let (sx, sz) = motion_step(w.get::<Yaw>(e).map_or(0.0, |y| y.0));
    impulse(w, e, Vec3::new(f64::from(sx) * p.push, p.lift, f64::from(sz) * p.push));
    stop_nav(w, e);
    w.entity_mut(e).insert(Leaping);
    let (v, pitch) = (w_vel(w, e), w.get::<Drive>(e).map_or(0.0, |d| d.pitch));
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.pitch = next_pitch(v, pitch, &p);
    }
}
/// `stop` : l'inclinaison revient à 0 ; une attente fraîche pour le prochain saut.
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Leaping>();
    if let Some(mut d) = w.get_mut::<Drive>(e) {
        d.pitch = 0.0;
    }
    schedule(w, e, inst);
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, leap_run);
}
/// `JumpGoal.canContinue` (fin si faux), puis `tick` (l'inclinaison). Un dauphin qui n'a pas de but ne coûte rien.
#[allow(clippy::type_complexity)]
fn leap_run(mut reqs: ResMut<Requests>, mut q: Query<(Entity, &Leaper, &Vel, &Contact, &FluidState, &mut Drive), With<Leaping>>) {
    for (e, l, vel, contact, fluid, mut drive) in &mut q {
        let vy2 = vel.0.y * vel.0.y;
        let keep = vy2 >= l.p.pitch_threshold || f64::from(drive.pitch) == 0.0 || drive.pitch.abs() >= l.p.pitch_limit || !fluid.in_water;
        if !(keep && !contact.on_ground) {
            reqs.0.push(Req::End(e, l.slot));
            continue;
        }
        drive.pitch = next_pitch(vel.0, drive.pitch, &l.p);
    }
}
fn w_vel(w: &World, e: Entity) -> Vec3 {
    w.get::<Vel>(e).map_or(Vec3::new(0.0, 0.0, 0.0), |v| v.0)
}
/// L'inclinaison du tick (`dolphin.rs:263-269`), en degrés : retour vers 0 quand la montée est faible, sinon l'angle de la vitesse.
fn next_pitch(v: Vec3, pitch: f32, p: &Params) -> f32 {
    let horizontal = (v.x * v.x + v.z * v.z).sqrt();
    if v.y * v.y < p.pitch_threshold && pitch != 0.0 {
        pitch + p.lerp * crate::cow::wrap(0.0 - pitch)
    } else if (v.x * v.x + v.y * v.y + v.z * v.z).sqrt() > 1e-5 {
        (-v.y).atan2(horizontal).to_degrees() as f32
    } else {
        pitch
    }
}
/// La direction de nage du `motion_step` (`dolphin.rs:14-23`) : sud, ouest, nord, est selon `yaw / 90 + 0,5`.
fn motion_step(yaw: f32) -> (i32, i32) {
    match ((f64::from(yaw) / 90.0 + 0.5).floor() as i32) & 3 {
        0 => (0, 1),
        1 => (-1, 0),
        2 => (0, -1),
        _ => (1, 0),
    }
}
/// `dolphin.rs:216-230` : pour chaque décalage, l'eau libre au pied, sans le tag interdit, avec deux blocs d'air au-dessus.
fn geometry_ok(w: &World, e: Entity, p: &Params) -> bool {
    let (Some(pos), Some(yaw), Some(level)) = (w.get::<Pos>(e).map(|x| x.0), w.get::<Yaw>(e).map(|y| y.0), w.get_resource::<Level>()) else {
        return false;
    };
    let mw = level.mw.0;
    let (sx, sz) = motion_step(yaw);
    let (x, y, z) = (pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32);
    p.offsets.iter().all(|&i| {
        let (bx, bz) = (x + sx * i, z + sz * i);
        mw.fluid(bx, y, bz) == Fluid::Water
            && !mw.has_tag(p.forbid_tag, mw.block_name(bx, y, bz))
            && mw.facts.is_air(mw.world.block(bx, y + 1, bz))
            && mw.facts.is_air(mw.world.block(bx, y + 2, bz))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 5, &Params::DEFAULT)]));
        Config { entries }
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// Un dauphin seul avec ce but, posé dans une mare de 110 x 110 (eau aux rangées -1 et 0) : hors de l'eau il meurt.
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(60, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        for x in -55..=55 {
            for z in -55..=55 {
                h.set_block(x, -1, z, "minecraft:water");
                h.set_block(x, 0, z, "minecraft:water");
            }
        }
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:dolphin").unwrap();
        sp.goals = config();
        let d = spawn_species(&mut h.world, &sp, 0.5, 0.5, 91);
        h.world.get_mut::<ActiveUntil>(d).unwrap().0 = u64::MAX;
        h.world.get_mut::<Yaw>(d).unwrap().0 = 0.0;
        h.world.resource_mut::<crate::goals::Entered>().0.push(d);
        h.tick();
        Some((h, d))
    }

    /// Le dauphin dans la mare saute : le but démarre, la poussée est vers le sud (z croissant), puis le but se termine
    /// et l'inclinaison revient à 0. Le test s'arrête à la fin du saut : le dauphin n'a pas d'air à l'extérieur du jeu.
    #[test]
    fn a_dolphin_in_a_pool_leaps_south_and_the_leap_ends() {
        let Some((mut h, d)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let (mut started, mut z_at_start, mut ended) = (false, 0.0, false);
        for _ in 0..600 {
            h.tick();
            if !started && running(&h, d) {
                started = true;
                z_at_start = h.world.get::<Pos>(d).unwrap().0.z;
            } else if started && !running(&h, d) {
                ended = true;
                break;
            }
        }
        assert!(started, "témoin : un saut dans la mare");
        assert!(h.world.get::<Pos>(d).unwrap().0.z > z_at_start, "le saut va vers le sud");
        assert!(ended, "le saut finit en revenant dans l'eau");
        assert_eq!(h.world.get::<Drive>(d).unwrap().pitch, 0.0, "l'inclinaison revient à 0 à la fin");
    }

    /// Un bloc de pierre sur la trajectoire (au décalage 4, dans l'eau) : la géométrie refuse ; sans lui, elle accepte.
    #[test]
    fn a_block_in_the_way_refuses_the_leap() {
        let Some((mut h, d)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let p = Params::DEFAULT;
        h.tick();
        assert!(geometry_ok(&h.world, d, &p), "témoin : la mare libre permet le saut");
        h.set_block(0, 0, 4, "minecraft:stone");
        h.tick();
        assert!(!geometry_ok(&h.world, d, &p), "un bloc plein sur la trajectoire refuse le saut");
    }
}
