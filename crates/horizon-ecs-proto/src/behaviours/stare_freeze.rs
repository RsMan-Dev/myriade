//! `Creaking.checkCanMove` as a module: an entity that stands STILL (`Stilled`: its path and move control wait, its knockback is
//! nil) while a player looks at it, and is ROUSED (`Roused`: it takes that player as attack target) when a player looks at it from
//! within `wake_range_sqr`. The look test is `isLookingAtMe(player, 0.5, false, true, ...)`: the view cone (`dot > dot_min`) at the
//! eyes, the middle and the feet + 0.5, and a visual line of sight within `reach`. A roused entity ignores a stare through a
//! `disguise` helmet. It loses its roused state, and its target, when no attackable player is near.
//!
//! Nothing is polled. The rule is asked again by EVENT: a player's view vector changed (`Events.gaze_changed`), a player moved
//! (`Online.seen`), or a roused free entity that changed cell (`CellMoves`); only the carriers near that player are asked (the
//! spatial index). State is a marker, set and removed on the transition (`Stilled`, `Roused`); the brain reads it through
//! `is_roused` and `can_move` (parameters of the attack behaviours) and through the memory `STILLED` (the activity's requirement).
//! Not events (so not seen until the next one): a helmet change, a game-mode change, a block that opens or closes the line of sight.
use super::*;
use crate::brain::{self, MemKey, MemValue, Shape};
use crate::hostile::PlayerState;
use crate::targeting::{self, Eye, Target, Who};
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

/// The brain memory a stilled entity carries (an activity that needs a free entity requires it absent).
pub static STILLED: MemKey = MemKey::new("horizon:stilled", Shape::Unit);

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `Creaking.ACTIVATION_RANGE_SQ`: a stare from within it rouses (144 = 12 blocks).
    pub wake_range_sqr: f64,
    /// The cone: `dot > dot_min` (`isLookingAtMe(.., 0.5, ..)`).
    pub dot_min: f64,
    /// The line of sight reaches 128 blocks.
    pub reach: f64,
    /// The players watched are those within this many blocks (the follow range: the entity's `NearestPlayers`).
    pub near: f64,
    /// The helmets that hide a player from a roused entity (`#gaze_disguise_equipment`, read by the species). Empty: none.
    pub disguise: &'static [u32],
}
impl Params {
    pub const DEFAULT: Self = Self { wake_range_sqr: 144.0, dot_min: 0.5, reach: 128.0, near: 32.0, disguise: &[] };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct StareFreeze(pub Params);
/// `Creaking.isActive`: roused by a stare, it hunts the player.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Roused;
/// The knockback resistance an entity had before it was stilled (restored when it moves again).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
struct Braced(f64);

pub static MODULE: Module = Module {
    name: "stare_freeze",
    flags: 0,
    equip,
    arm,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Targets, Cap::Locomotion, Cap::Brain],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(StareFreeze(*params::<Params>(p)));
}
/// Creation, entry into the active zone: the rule is asked once.
fn arm(w: &mut World, e: Entity, _: Inst) {
    evaluate(w, e);
}
/// `Creaking.isActive`, for the parameters of the attack behaviours.
pub fn is_roused(w: &World, e: Entity) -> bool {
    w.get::<Roused>(e).is_some()
}
/// `Creaking.canMove`.
pub fn can_move(w: &World, e: Entity) -> bool {
    w.get::<Stilled>(e).is_none()
}

/// The ticks of the module (no system in the schedule): who must be asked again this tick.
fn tick(w: &mut World) {
    crate::goals::tick_state(w, asked);
}
#[allow(clippy::type_complexity)]
fn asked(ev: Res<Events>, on: Res<Online>, idx: Res<Index>, moves: Res<CellMoves>, q: Query<(&Pos, &StareFreeze)>, free: Query<(), (With<Roused>, Without<Stilled>, With<StareFreeze>)>, mut cmd: Commands) {
    if q.is_empty() {
        return;
    }
    let mut todo: Vec<Entity> = Vec::new();
    let near = |x: f64, z: f64, todo: &mut Vec<Entity>| {
        idx.for_radius(x, z, 32.0, |c| {
            if let Ok((pos, s)) = q.get(c)
                && (pos.0.x - x).powi(2) + (pos.0.z - z).powi(2) <= (s.0.near + 1.0).powi(2)
            {
                todo.push(c);
            }
        });
    };
    for id in ev.gaze_changed.iter() {
        if let Some(p) = on.get(id) {
            near(p.x, p.z, &mut todo);
        }
    }
    for (old, new) in &on.seen {
        for p in [old, new].into_iter().flatten() {
            near(p.x, p.z, &mut todo);
        }
    }
    // A roused entity that is free and walks may walk into a player's view: asked when it changes cell.
    for &e in &moves.0 {
        if free.contains(e) {
            todo.push(e);
        }
    }
    todo.sort_unstable();
    todo.dedup();
    for e in todo {
        cmd.queue(move |w: &mut World| evaluate(w, e));
    }
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < f64::from(1.0e-5_f32) { [0.0; 3] } else { [v[0] / len, v[1] / len, v[2] / len] }
}
/// `LivingEntity.isLookingAtMe(player, 0.5, false, true, eyeY, y + 0.5, (eyeY + y) / 2)`.
fn looked_at(w: &World, pos: Vec3, eye_y: f64, p: Player, p_: &Params) -> bool {
    let look = normalize(w.resource::<PlayerState>().gaze(p.id));
    let mw = w.resource::<Level>().mw.0;
    [eye_y, pos.y + 0.5, (eye_y + pos.y) / 2.0].into_iter().any(|y| {
        let d = normalize([pos.x - p.x, y - p.eye(), pos.z - p.z]);
        if look[0] * d[0] + look[1] * d[1] + look[2] * d[2] <= p_.dot_min {
            return false;
        }
        let (from, to) = (Vec3::new(p.x, p.eye(), p.z), Vec3::new(pos.x, y, pos.z));
        to.distance(from) <= p_.reach && mw.visual_clear_between(from, to)
    })
}

/// `Creaking.checkCanMove`, asked at an event: rouses, calms, stills or frees the entity.
pub fn evaluate(w: &mut World, e: Entity) {
    let (Some(&StareFreeze(p)), Some(pos), Some(eye)) = (w.get::<StareFreeze>(e), w.get::<Pos>(e).map(|v| v.0), w.get::<Eye>(e).map(|v| v.0)) else { return };
    let roused = w.get::<Roused>(e).is_some();
    let was_still = w.get::<Stilled>(e).is_some();
    let (mut still, mut wake, mut potential) = (false, None, false);
    if let Some(id) = crate::brain::sensors::nearest_players::near_player(w, e)
        && let Some(player) = w.resource::<Online>().get(id)
    {
        let ps = w.resource::<PlayerState>();
        if ps.alive(id) && !ps.creative(id) && !ps.spectator(id) {
            potential = true;
            let disguised = !p.disguise.is_empty() && p.disguise.contains(&ps.helmet(id));
            if (!roused || !disguised) && looked_at(w, pos, pos.y + eye, player, &p) {
                if roused {
                    still = true;
                } else if (pos.x - player.x).powi(2) + (pos.y - player.y).powi(2) + (pos.z - player.z).powi(2) < p.wake_range_sqr {
                    wake = Some(id);
                    still = true;
                }
            }
        }
    }
    if let Some(id) = wake {
        w.entity_mut(e).insert(Roused);
        brain::set(w, e, &brain::ATTACK_TARGET, MemValue::Who(Who::Player(id)), None);
        targeting::set_target(w, e, Target::new(Who::Player(id), u16::MAX, u8::MAX));
    } else if roused && !potential {
        w.entity_mut(e).remove::<Roused>();
        brain::erase(w, e, &brain::ATTACK_TARGET);
        targeting::clear_target(w, e);
    }
    if still == was_still {
        return;
    }
    if still {
        // `Mob.stopInPlace`, and no knockback while looked at.
        let old = w.get::<Resist>(e).map_or(0.0, |r| r.0);
        w.entity_mut(e).insert((Stilled, Braced(old), Resist(1.0)));
        if let Some(mut d) = w.get_mut::<Drive>(e) {
            d.zza = 0.0;
            d.xxa = 0.0;
            d.yya = 0.0;
            d.speed = 0.0;
            d.jump = false;
        }
        if let Some(mut v) = w.get_mut::<Vel>(e) {
            v.0 = Vec3::ZERO;
        }
        brain::set(w, e, &STILLED, MemValue::Unit, None);
    } else {
        let old = w.get::<Braced>(e).map_or(0.0, |b| b.0);
        w.entity_mut(e).remove::<(Stilled, Braced)>().insert(Resist(old));
        brain::erase(w, e, &STILLED);
    }
}
// lab
