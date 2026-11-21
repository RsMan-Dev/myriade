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

/// The goal slot of the module on the animal.
#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct Slot(u8);

pub static MODULE: Module = Module {
    name: "still_under_riders",
    flags: MOVE,
    equip,
    arm,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert((PlayerWatch::new(p.reach as f32), p, Slot(inst.slot)));
}
/// The module's tick, called only while a creature that carries it lives (no registered system).
fn tick(w: &mut World) {
    crate::goals::tick_state(w, watched);
}
/// A player enters the watch: the goal starts; a player leaves: the goal ends (its stop starts the countdown).
/// A player who rides the ghast does not count (`scanPlayerAboveGhast` skips the players whose root vehicle is a happy ghast): one who
/// takes a seat ends the stillness, one who gets off while still in reach starts it again.
fn watched(
    ev: Res<Events>,
    mut reqs: ResMut<Requests>,
    mut commands: Commands,
    q: Query<(&Slot, &PlayerWatch, Option<&crate::mount::Mount>, Has<Above>)>,
    ridden: Option<Res<crate::seat::RiddenVehicles>>,
    mounts: Query<&crate::mount::Mount>,
) {
    // The player rides ANOTHER flying harnessed mount (`scanPlayerAboveGhast`: its root vehicle is a happy ghast): not above this one.
    let on_other_ghast = |id: u32| {
        ridden.as_ref().is_some_and(|r| {
            r.0.iter().any(|v| {
                mounts.get(*v).is_ok_and(|m| matches!(m.steer, crate::mount::Steer::Fly { .. }) && [m.rider, m.passenger].contains(&Some(crate::targeting::Who::Player(id))))
            })
        })
    };
    let seats =|m: Option<&crate::mount::Mount>, id: u32| m.is_some_and(|m| [m.rider, m.passenger].contains(&Some(crate::targeting::Who::Player(id))));
    let rider = |m: Option<&crate::mount::Mount>| m.is_some_and(|m| [m.rider, m.passenger].iter().any(|r| matches!(r, Some(crate::targeting::Who::Player(_)))));
    for (e, id) in ev.player_entered.iter() {
        if let Ok((s, _, m, _)) = q.get(e)
            && !seats(m, id)
            && !on_other_ghast(id)
        {
            commands.entity(e).insert(Above);
            reqs.0.push(Req::Start(e, s.0));
        }
    }
    for (e, _) in ev.player_left.iter() {
        if let Ok((s, _, _, true)) = q.get(e) {
            commands.entity(e).remove::<Above>().insert(Leaving);
            reqs.0.push(Req::End(e, s.0));
        }
    }
    for e in ev.mounted.iter() {
        if let Ok((s, _, m, true)) = q.get(e)
            && rider(m)
        {
            commands.entity(e).remove::<Above>().insert(Leaving);
            reqs.0.push(Req::End(e, s.0));
        }
    }
    for e in ev.dismounted.iter() {
        if let Ok((s, w, m, false)) = q.get(e)
            && !rider(m)
            && w.any_inside()
        {
            commands.entity(e).insert(Above);
            reqs.0.push(Req::Start(e, s.0));
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
