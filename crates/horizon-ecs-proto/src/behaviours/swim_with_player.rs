//! `Dolphin.SwimWithPlayerGoal` (`mobs/dolphin.rs:127-182`): MOVE | LOOK. `canUse`: the nearest player within 10 blocks (walls do not
//! count), who SWIMS, and whom the animal does not target. `canContinue`: that player still swims and is within 16 blocks (d² < 256).
//! `tick`: walk to the player (`nav_move_to_entity`, reach 1) unless within 2.5 blocks (d² < 6.25), where the walk stops.
//! NOT PORTED, BESOIN DU CADRE: the player's `swimming` state (`Player` in `goals.rs` has no such field: `swimming_of` below answers
//! `None`, so the goal never starts until the framework gives it); the effect `dolphins_grace` (`MobEvent::EffectPlayer`: no
//! potion effects on players in the prototype) and its 1-in-6 draw per tick; the head's look at the player (`look_at_speed`:
//! the look is not written). The decisions are pure functions below, tested with their values.
use super::*;
use crate::rules::swim_with_player::{is_close, may_continue, may_start};
use crate::targeting::{Target, Who};
use crate::{cow::*, goals::*};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed of the walk to the player (`SwimWithPlayerGoal(4.0)`, `aquatic.rs:173`).
    pub speed: f64,
    /// Within this squared distance the animal stops (`6.25` = 2.5 blocks).
    pub near_sqr: f64,
    /// The range of the start, in blocks (`range(10)`), and of the continuation, squared (`256` = 16 blocks).
    pub range: f64,
    pub keep_sqr: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 4.0, near_sqr: 6.25, range: 10.0, keep_sqr: 256.0 };
}
/// The escort: the player followed (its identifier) and the speed of the walk.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Escorting {
    pub player: u32,
    pub speed: f64,
}
/// The entry's settings (read at the start).
#[derive(Component, Clone, Copy, Debug)]
pub struct EscortParams {
    pub speed: f64,
    pub near_sqr: f64,
    pub range: f64,
    pub keep_sqr: f64,
}

pub static MODULE: Module = Module {
    name: "swim_with_player",
    tick: Some(tick),
    flags: MOVE | LOOK,
    equip,
    can_use,
    start,
    stop,
    requires: &[Cap::Locomotion, Cap::Goals],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(EscortParams { speed: p.speed, near_sqr: p.near_sqr, range: p.range, keep_sqr: p.keep_sqr });
}

/// The player's `swimming` state. The prototype's `Player` has no such field: `None` until the framework gives it (BESOIN DU CADRE).
fn swimming_of(_w: &World, _player: u32) -> Option<bool> {
    None
}

/// `canUse` without the walls: the nearest player within `range` (3D), if any, as (id, squared distance).
fn nearest(w: &World, e: Entity, range: f64) -> Option<(u32, f64)> {
    let pos = w.get::<Pos>(e)?.0;
    let on = w.resource::<Online>();
    let mut best: Option<(u32, f64)> = None;
    on.for_near(pos.x, pos.z, range, |p| {
        let d2 = (p.x - pos.x).powi(2) + (p.y - pos.y).powi(2) + (p.z - pos.z).powi(2);
        if d2 <= range * range && best.is_none_or(|(_, b)| d2 < b) {
            best = Some((p.id, d2));
        }
    });
    best
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(p) = w.get::<EscortParams>(e).copied() else { return false };
    let Some((id, d2)) = nearest(w, e, p.range) else { return false };
    let targets_player = w.get::<Target>(e).is_some_and(|t| t.who == Who::Player(id));
    if !may_start(swimming_of(w, id), d2, p.range, targets_player) {
        return false;
    }
    w.entity_mut(e).insert(Escorting { player: id, speed: p.speed });
    true
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let speed = w.get::<Escorting>(e).map_or(1.0, |x| x.speed);
    if let Some(mut n) = w.get_mut::<Nav>(e) {
        n.speed = speed;
    }
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Escorting>();
    stop_nav(w, e);
}

/// The ticks of the escorts, run by `arbitrate` (`Module::tick`) while a creature carries the goal (no system: the module costs the schedule nothing).
/// It ends the goal when the player is gone, no longer swims, or is too far; else it walks to the player, or stops near it.
fn tick(w: &mut World) {
    static ID: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let id = ID.get_or_init(|| crate::species::registry().module_id(&MODULE));
    let carried = id.is_some_and(|i| w.resource::<Carriers>().0.get(i).is_some_and(|n| *n > 0));
    if !carried {
        return;
    }
    let escorts: Vec<(Entity, Escorting)> = crate::queries::with::<(Entity, &'static Escorting), (), _>(w, |q, w| q.iter(w).map(|(e, x)| (e, *x)).collect());
    for (e, x) in escorts {
        let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { continue };
        let Some(p) = w.resource::<Online>().get(x.player) else {
            crate::goals::end(w, e, &MODULE);
            continue;
        };
        let d2 = (p.x - pos.x).powi(2) + (p.y - pos.y).powi(2) + (p.z - pos.z).powi(2);
        let keep = w.get::<EscortParams>(e).map_or(256.0, |k| k.keep_sqr);
        if !may_continue(swimming_of(w, x.player), d2, keep) {
            crate::goals::end(w, e, &MODULE);
        } else if is_close(d2, w.get::<EscortParams>(e).map_or(6.25, |k| k.near_sqr)) {
            stop_nav(w, e);
        } else {
            go_to_at(w, e, [p.x, p.y, p.z], x.speed, 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A swimming player within range that the animal does not target: the goal may start (the positive witness), and each
    /// of its three conditions alone refuses it.
    #[test]
    fn the_goal_starts_for_a_swimming_player_in_range_only() {
        assert!(may_start(Some(true), 64.0, 10.0, false), "swimming, 8 blocks, not targeted: starts");
        assert!(!may_start(Some(false), 64.0, 10.0, false), "a player who does not swim: refused");
        assert!(!may_start(None, 64.0, 10.0, false), "swimming unknown: refused (the framework does not give it yet)");
        assert!(!may_start(Some(true), 121.0, 10.0, false), "11 blocks: out of range");
        assert!(!may_start(Some(true), 64.0, 10.0, true), "the player is the animal's target: refused");
    }

    /// The escort goes on while the player swims within 16 blocks, and stops at the surface.
    #[test]
    fn the_escort_goes_on_within_16_blocks_while_the_player_swims() {
        assert!(may_continue(Some(true), 200.0, 256.0), "swimming at 14 blocks: goes on");
        assert!(!may_continue(Some(true), 300.0, 256.0), "17 blocks: ends");
        assert!(!may_continue(Some(false), 100.0, 256.0), "the player left the water: ends");
    }

    /// Close enough, the walk stops (2 blocks: stop; 3 blocks: keep walking).
    #[test]
    fn the_walk_stops_within_two_and_a_half_blocks() {
        assert!(is_close(4.0, 6.25), "2 blocks: stops");
        assert!(!is_close(9.0, 6.25), "3 blocks: walks on");
    }
}
