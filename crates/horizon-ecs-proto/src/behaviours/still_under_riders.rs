//! A happy ghast holds still while a player is on it (`happy_ghast.rs` `still_tick`, `player_above`, the `move_control` hold): the goal
//! takes MOVE while it runs, so the float and the stroll wait. Event-driven: a `PlayerWatch` wakes it when a player comes into its
//! reach and when one leaves; the ghast keeps still `after` more ticks once the last one is gone (`counter`, species).
//! DOUTE : la boîte de dessus est approchée par un rayon `reach` autour du centre (vanille : boîte d'un bloc de large au-dessus du sommet).
//! DOUTE : plusieurs joueurs dessus : le premier qui part arrête le but ; vanille remet le compteur à `after` par joueur.
//! DOUTE : `StillUntil` ignore les ticks d'âge > 60 de vanille (le décompte part à la sortie).
use super::*;
use crate::cow::*;
use crate::goals::*;
use horizon_core::geometry::Vec3;

/// The still timeout's constants (species): the reach of the watch, and the ticks kept still after the last player left.
#[derive(Component, Clone, Copy, Debug)]
pub struct Params {
    pub reach: f64,
    pub after: u64,
}
impl Params {
    pub const DEFAULT: Self = Self { reach: 2.0, after: 10 };
}
/// Until when the goal may run (`counter`), set when the last player left.
#[derive(Component, Clone, Copy, Debug)]
pub struct StillUntil(pub u64);
/// A player stands on the ghast now.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Above;
/// The player left: the goal's stop starts the countdown.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Leaving;

pub static MODULE: Module = Module {
    name: "still_under_riders",
    flags: MOVE,
    equip,
    arm,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert((PlayerWatch::new(p.reach), p));
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(watched.in_set(slot));
}
/// A player enters the watch: the goal starts; a player leaves: the goal ends (its stop starts the countdown).
fn watched(ev: Res<Events>, mut reqs: ResMut<Requests>, mut commands: Commands, q: Query<(), With<PlayerWatch>>) {
    for (e, _) in ev.player_entered.iter() {
        if q.get(e).is_ok() {
            commands.entity(e).insert(Above);
            reqs.0.push(Req::Start(e, GoalSlot(0)));
        }
    }
    for (e, _) in ev.player_left.iter() {
        if q.get(e).is_ok() {
            commands.entity(e).remove::<Above>().insert(Leaving);
            reqs.0.push(Req::End(e, GoalSlot(0)));
        }
    }
}
/// Wakes the selector at the end of the stillness, if it has one left.
fn arm(w: &mut World, e: Entity, inst: Inst) {
    if let Some(u) = w.get::<StillUntil>(e).map(|u| u.0) {
        let left = u.saturating_sub(now_of(w, e)) as i32;
        if left > 0 {
            crate::goals::arm(w, e, inst.slot, left);
        }
    }
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    w.get::<Above>(e).is_some() || w.get::<StillUntil>(e).is_some_and(|u| now_of(w, e) < u.0)
}
/// Holds still: no speed and no path (`move_control`'s hold).
fn start(w: &mut World, e: Entity, _: Inst) {
    stop_nav(w, e);
    if let Some(mut v) = w.get_mut::<Vel>(e) {
        v.0 = Vec3::ZERO;
    }
}
/// Ends a stillness: a player who left starts the countdown.
fn stop(w: &mut World, e: Entity, inst: Inst) {
    if w.get::<Leaving>(e).is_some() {
        w.entity_mut(e).remove::<Leaving>();
        let after = w.get::<Params>(e).map_or(Params::DEFAULT.after, |p| p.after);
        let until = now_of(w, e) + after;
        w.entity_mut(e).insert(StillUntil(until));
        crate::goals::arm(w, e, inst.slot, after as i32);
    }
}
