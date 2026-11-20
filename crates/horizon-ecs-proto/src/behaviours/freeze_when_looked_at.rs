//! `EndermanFreezeWhenLookedAt` (fiche 20, `enderman.rs:370-397`): an enderman whose target is a player stops and turns its
//! head to him while he stares at it (`stared_at`: the view within a cone of 0.025 / distance, line of sight, no gaze
//! disguise, in range). It runs while the stare holds (`canContinueToUse` = `canUse`). The framework has no event for a
//! change of the player's look or place: while the enderman targets a player and the goal does not run, its ask is armed
//! every 2 ticks, as the `Mob` asks (DOUTE: a poll, only for an enderman with a player target).
use super::*;
use crate::hostile::PlayerState;
use crate::targeting::{Eye, Target, Who};
use crate::{cow::*, goals::*};
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `distance_sqr <= 256` (16 blocks) from the player's feet.
    pub range_sqr: f64,
    /// `0.025` in `dot > 1 - 0.025 / distance`.
    pub gaze_k: f64,
    /// The helmet items that wear the gaze disguise (`#minecraft:gaze_disguise_equipment`, read by the species: the
    /// framework does not read item tags). Empty: no disguise.
    pub gaze_disguise: &'static [u32],
}
impl Params {
    pub const DEFAULT: Self = Self { range_sqr: 256.0, gaze_k: 0.025, gaze_disguise: &[] };
}
/// The parameters of the instance and its slot (for `Req::End`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Freezer {
    pub range_sqr: f64,
    pub gaze_k: f64,
    pub disguise: &'static [u32],
    pub slot: u8,
}
/// The goal runs: the enderman holds still and looks at the player.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Freezing;

pub static MODULE: Module = Module {
    name: "freeze_when_looked_at",
    flags: JUMP | MOVE,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Head, Cap::Locomotion, Cap::Targets],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Freezer { range_sqr: p.range_sqr, gaze_k: p.gaze_k, disguise: p.gaze_disguise, slot: inst.slot });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((freeze_wake, freeze_run).chain().in_set(slot));
}
/// `stared_at` of the enderman's player, and the range: the player when the stare holds.
fn stare(on: &Online, ps: &PlayerState, mw: &MobWorld, pos: Vec3, eye_y: f64, f: &Freezer, id: u32) -> Option<Player> {
    let p = on.get(id)?;
    if !ps.alive(id) || (!f.disguise.is_empty() && f.disguise.contains(&ps.helmet(id))) {
        return None;
    }
    if (pos.x - p.x).powi(2) + (pos.y - p.y).powi(2) + (pos.z - p.z).powi(2) > f.range_sqr {
        return None;
    }
    let look = ps.gaze(id);
    let dir = [pos.x - p.x, eye_y - p.eye(), pos.z - p.z];
    let dist = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if dist < 1.0e-6 {
        return None;
    }
    let dot = (look[0] * dir[0] + look[1] * dir[1] + look[2] * dir[2]) / dist;
    (dot > 1.0 - f.gaze_k / dist && mw.line_of_sight(Vec3::new(p.x, p.eye(), p.z), Vec3::new(pos.x, eye_y, pos.z))).then_some(p)
}
fn watched(w: &World, e: Entity) -> Option<Player> {
    let Who::Player(id) = w.get::<Target>(e)?.who else { return None };
    let pos = w.get::<Pos>(e)?.0;
    let eye = w.get::<Eye>(e).map_or(1.0, |v| v.0);
    let f = *w.get::<Freezer>(e)?;
    stare(w.resource::<Online>(), w.resource::<PlayerState>(), w.resource::<Level>().mw.0, pos, pos.y + eye, &f, id)
}
/// Not watched now: while the enderman targets a player, the ask comes back in 2 ticks (the `Mob` asks every 2 ticks).
fn rearm(w: &mut World, e: Entity, inst: Inst) {
    if matches!(w.get::<Target>(e).map(|t| t.who), Some(Who::Player(_))) {
        crate::goals::arm(w, e, inst.slot, 2);
    }
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    if watched(w, e).is_some() {
        return true;
    }
    rearm(w, e, inst);
    false
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Freezing);
    stop_nav(w, e);
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    w.entity_mut(e).remove::<Freezing>();
    rearm(w, e, inst);
}
/// A player became the enderman's target: the goal is asked at the next selector tick.
fn freeze_wake(ev: Res<Events>, q: Query<(&Target, &GoalSet), With<Freezer>>, mut reqs: ResMut<Requests>) {
    for e in ev.target_acquired.iter() {
        if let Ok((t, g)) = q.get(e)
            && matches!(t.who, Who::Player(_))
            && let Some(slot) = g.slot_of(&MODULE)
        {
            reqs.0.push(Req::Start(e, slot));
        }
    }
}
/// `FreezeWhenLookedAt.tick`: look at the player's eyes; the goal ends when the stare is over.
fn freeze_run(
    on: Res<Online>,
    ps: Res<PlayerState>,
    level: Res<Level>,
    mut q: Query<(Aim, &Target, &Eye, &Freezer), With<Freezing>>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    let mw = level.mw.0;
    for ((e, pos, mut look, looking, _, _), t, eye, f) in &mut q {
        let Who::Player(id) = t.who else {
            reqs.0.push(Req::End(e, f.slot));
            continue;
        };
        match stare(&on, &ps, mw, pos.0, pos.0.y + eye.0, f, id) {
            Some(p) => aim(&mut cmd, e, &mut look, looking, [p.x, p.eye(), p.z], None),
            None => reqs.0.push(Req::End(e, f.slot)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{Herd, SpeciesTable, spawn_species};
    use crate::species::Config;
    use crate::targeting::{Target, set_target};

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 1, &Params::DEFAULT)];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    /// An enderman (a cow named so) at x, with the goal; its player is `id` at px, looking at the enderman with `angle` (rad)
    /// away from the straight line, in the vertical plane of the look.
    fn scene(h: &mut Herd, x: f64) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:enderman";
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, x, 0.5, 31);
        h.world.flush();
        e
    }
    /// Player 1 at horizontal distance d from the enderman, its gaze turned `angle` from the straight line to it.
    fn stare_from(h: &mut Herd, e: Entity, d: f64, angle: f64) {
        let pos = h.world.get::<Pos>(e).unwrap().0;
        let eye_y = pos.y + h.world.get::<Eye>(e).unwrap().0;
        let (px, pz) = (pos.x + d, pos.z);
        h.world.resource_mut::<Online>().set(Player { id: 1, x: px, y: pos.y, z: pz, main: 0, off: 0, ..Default::default() });
        let pe = [px, pos.y + 1.62, pz];
        let dir = [pos.x - px, eye_y - pe[1], pos.z - pz];
        let n = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        let d = [dir[0] / n, dir[1] / n, dir[2] / n];
        let up = [0.0, 1.0, 0.0];
        let perp = [d[1] * up[2] - d[2] * up[1], d[2] * up[0] - d[0] * up[2], d[0] * up[1] - d[1] * up[0]];
        let pn = (perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2]).sqrt();
        let g: [f64; 3] = std::array::from_fn(|i| angle.cos() * d[i] + angle.sin() * perp[i] / pn);
        h.world.resource_mut::<PlayerState>().set_gaze(1, g);
        set_target(&mut h.world, e, Target::player(1, 100));
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }

    /// Player 6 blocks away staring straight at the enderman: the goal starts within a few ticks, stops the walk, and the
    /// head looks at the player's eyes; when he turns away the goal ends.
    #[test]
    fn a_stare_freezes_the_enderman_and_ends_when_the_player_turns_away() {
        let Some(mut h) = herd() else { return };
        let e = scene(&mut h, 0.5);
        stare_from(&mut h, e, 6.0, 0.0);
        ticks(&mut h, 4);
        assert!(running(&h, e), "a stare from 6 blocks freezes the enderman");
        let look = h.world.get::<Look>(e).unwrap().target;
        assert!((look[0] - 6.5).abs() < 1e-6 && (look[2] - 0.5).abs() < 1e-6, "it looks at the player: {look:?}");
        stare_from(&mut h, e, 6.0, std::f64::consts::FRAC_PI_2);
        ticks(&mut h, 4);
        assert!(!running(&h, e), "a player who looks away ends the freeze");
    }

    /// The cone narrows with the distance: at 6 blocks a 4 degree turn still freezes (cos > 1 - 0.025 / 6) and 6 degrees
    /// does not; at 14 blocks 2 degrees freezes and 5 degrees does not.
    #[test]
    fn the_cone_of_view_narrows_with_the_distance() {
        let Some(mut h) = herd() else { return };
        let e = scene(&mut h, 0.5);
        let deg = |d: f64| d.to_radians();
        stare_from(&mut h, e, 6.0, deg(4.0));
        ticks(&mut h, 4);
        assert!(running(&h, e), "4 degrees at 6 blocks: frozen");
        stare_from(&mut h, e, 6.0, deg(6.0));
        ticks(&mut h, 4);
        assert!(!running(&h, e), "6 degrees at 6 blocks: not frozen");
        stare_from(&mut h, e, 14.0, deg(2.0));
        ticks(&mut h, 4);
        assert!(running(&h, e), "2 degrees at 14 blocks: frozen");
        stare_from(&mut h, e, 14.0, deg(5.0));
        ticks(&mut h, 4);
        assert!(!running(&h, e), "5 degrees at 14 blocks: not frozen");
    }

    /// Beyond 16 blocks no freeze, however straight the stare; within it, the same stare freezes (the positive witness).
    #[test]
    fn beyond_sixteen_blocks_there_is_no_freeze() {
        let Some(mut h) = herd() else { return };
        let e = scene(&mut h, 0.5);
        stare_from(&mut h, e, 10.0, 0.0);
        ticks(&mut h, 4);
        assert!(running(&h, e), "a straight stare from 10 blocks freezes");
        stare_from(&mut h, e, 17.0, 0.0);
        ticks(&mut h, 4);
        assert!(!running(&h, e), "17 blocks: no freeze");
    }

    /// A gaze disguise (the item of `gaze_disguise`) stops the freeze; without it the same stare freezes.
    #[test]
    fn a_gaze_disguise_stops_the_freeze() {
        let Some(mut h) = herd() else { return };
        let e = scene(&mut h, 0.5);
        stare_from(&mut h, e, 6.0, 0.0);
        ticks(&mut h, 4);
        assert!(running(&h, e), "no helmet: frozen");
        h.world.resource_mut::<PlayerState>().set_helmet(1, 77);
        ticks(&mut h, 4);
        assert!(running(&h, e), "a helmet not in the disguise list does not end the freeze");
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:enderman";
        const DISGUISE: Params = Params { gaze_disguise: &[77], ..Params::DEFAULT };
        static DIS: [Entry; 1] = [Entry::new(&MODULE, 1, &DISGUISE)];
        sp.goals = Config { entries: &DIS };
        let d = spawn_species(&mut h.world, &sp, 40.5, 0.5, 32);
        h.world.flush();
        stare_from(&mut h, d, 6.0, 0.0);
        h.world.resource_mut::<PlayerState>().set_helmet(1, 77);
        ticks(&mut h, 4);
        assert!(!running(&h, d), "the carved pumpkin in the disguise list: no freeze");
    }
}
