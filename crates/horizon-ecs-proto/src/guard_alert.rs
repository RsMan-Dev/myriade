//! Alerting what guards something (`PiglinAi.angerNearbyPiglins`): a player opens or breaks a guarded container or mines a guarded block, and
//! the creatures that guard it within a radius of the player and are at rest turn on the player (on all of them when only the seen ones count:
//! those with a line of sight to it). The creatures are found by the spatial index; a species guards when its table carries `RULE`. Nothing
//! runs per tick: the host calls `alert` when the player acts.
use crate::behaviours::{admire::Admiring, celebrate::Celebrating, walk_away::Walking};
use crate::cow::{Level, Pos};
use crate::rules::{no_attrs, no_interact, params, Rule};
use crate::targeting::{self, Target, Who};
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;

/// The radius around the player that is searched (`inflate(16.0)` of its box).
const REACH: f64 = 16.0;

/// How a species guards.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The unseen evaluations the target survives (`Target.memory`).
    pub memory: u16,
}
impl Params {
    pub const PIGLIN: Self = Self { memory: 0 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Guards(pub &'static Params);
/// The rule `guard_alert`, parameters `Params`.
pub static RULE: Rule = Rule {
    name: "guard_alert",
    equip: |w, e, p| {
        w.entity_mut(e).insert(Guards(params::<Params>(p)));
    },
    attrs: no_attrs,
    interact: no_interact,
};

/// `PiglinAi.isIdle`: the creature is in no other activity: it fights nothing, admires nothing, celebrates nothing and does not walk away.
pub fn at_rest(w: &World, e: Entity) -> bool {
    w.get::<Target>(e).is_none() && w.get::<Admiring>(e).is_none() && w.get::<Celebrating>(e).is_none() && w.get::<Walking>(e).is_none()
}
/// The living player nearest to `e` within `radius` that it can target (`getNearestVisibleTargetablePlayer`, without the visibility memory).
fn nearest_player(w: &World, e: Entity, radius: f64) -> Option<Who> {
    let at = w.get::<Pos>(e)?.0;
    let area = crate::area::reach_box(w, e, radius);
    let sqr = |id: u32| targeting::info(w, Who::Player(id)).map(|i| (i.pos.x - at.x).powi(2) + (i.pos.y - at.y).powi(2) + (i.pos.z - at.z).powi(2));
    crate::area::players_in(w, area)
        .into_iter()
        .filter(|&id| targeting::can_target(w, e, Who::Player(id)))
        .filter_map(|id| Some((id, sqr(id)?)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| Who::Player(id))
}
/// The guards within reach of `player` are alerted: each one at rest (and in sight when `only_if_seen`) takes the player as target, or with
/// the `universal_anger` rule the nearest player it can target (the player when none). A player that cannot be targeted (creative,
/// spectator) alerts nobody. Returns how many were alerted.
pub fn alert(w: &mut World, player: u32, only_if_seen: bool) -> usize {
    let Some(info) = targeting::info(w, Who::Player(player)).filter(|i| !i.exempt) else { return 0 };
    let universal = w.get_resource::<Level>().is_some_and(|l| l.mw.0.universal_anger);
    let reach = info.bounds.inflate(Vec3::new(REACH, REACH, REACH));
    let mut done = 0;
    for e in crate::area::creatures_in(w, reach, Entity::PLACEHOLDER) {
        let Some(g) = w.get::<Guards>(e).map(|g| g.0) else { continue };
        if !at_rest(w, e) || (only_if_seen && !targeting::sees(w, e, Who::Player(player))) {
            continue;
        }
        let who = if universal { nearest_player(w, e, REACH).unwrap_or(Who::Player(player)) } else { Who::Player(player) };
        let mut t = Target::new(who, g.memory, u8::MAX);
        t.must_see = false;
        targeting::set_target(w, e, t);
        done += 1;
    }
    done
}
