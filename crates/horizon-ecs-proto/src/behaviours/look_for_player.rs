//! `EndermanLookForPlayerGoal` (the enderman's target): a player who stares at it, in sight and within the follow range,
//! becomes its target after `aggro_ticks` ticks, its eyes turning to him meanwhile. The target is kept at any distance
//! while the player is alive, not creative and not a spectator. Close up, a stare asks for a teleport; far away, the
//! target asks for one after `far_ticks` ticks. The teleport is the rule `R:enderman`, not here: the module counts the
//! requests (`asked_near`, `asked_far`). Wake: a player who moves (`Online.seen`) within the follow range (no timer, no poll).
use super::*;
use crate::targeting::{self, Eye, FollowRange, Target, Who};
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;
use crate::rules::look_for_player::gaze_hits;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `reducedTickDelay(5)`: the ticks the eyes stay on the player before he is the target.
    pub aggro_ticks: i32,
    /// Squared distances: a stare closer than `near_sqr` asks for a teleport; beyond `far_sqr` the target asks for one.
    pub near_sqr: f64,
    pub far_sqr: f64,
    /// `reducedTickDelay(30)`: the far ticks counted before the teleport towards the target.
    pub far_ticks: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { aggro_ticks: 3, near_sqr: 16.0, far_sqr: 256.0, far_ticks: 15 };
}
/// The state of the goal: `pending` while the eyes are on the player (the countdown), `pick` the player `can_use` chose.
#[derive(Component, Clone, Copy, Debug)]
pub struct Stare {
    pub slot: u8,
    pub p: &'static Params,
    pub pending: bool,
    pub aggro: i32,
    pub teleport: i32,
    pub pick: u32,
    /// The teleports the rule must make: close up (`teleport_random`), towards the target (`teleport_towards`).
    pub asked_near: u32,
    pub asked_far: u32,
}
/// The goal runs (its tick system visits it only then).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Staring;

pub static MODULE: Module = Module {
    name: "look_for_player",
    flags: TARGET,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Targets, Cap::Head],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    w.entity_mut(e).insert(Stare { slot: inst.slot, p, pending: false, aggro: 0, teleport: 0, pick: 0, asked_near: 0, asked_far: 0 });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((stare_wake, stare_run).chain().in_set(slot));
}
/// The stare, the sight and the distance of a player, as the `Mob` reads them (`EndermanLookForPlayerGoal.isAngerInducing`
/// without the angry-at-player part: `NeutralAnger`, fiche 38, is not here).
fn stared(level: &Level, ps: &crate::hostile::PlayerState, p: Player, feet: Vec3, eye: f64) -> bool {
    if !ps.alive(p.id) {
        return false;
    }
    // DOUBT: `wears_gaze_disguise` (the pumpkin, #gaze_disguise_equipment) is not modelled: the helmet is not read.
    let look = ps.gaze(p.id);
    let me = Vec3::new(feet.x, feet.y + eye, feet.z);
    let at_player = Vec3::new(p.x, p.eye(), p.z);
    let dir = [me.x - at_player.x, me.y - at_player.y, me.z - at_player.z];
    gaze_hits(look, dir) && level.mw.0.line_of_sight(at_player, me)
}
fn eligible(ps: &crate::hostile::PlayerState, id: u32) -> bool {
    ps.alive(id) && !ps.creative(id) && !ps.spectator(id)
}
/// Whether the player is the enderman's target now: a stare for the countdown, the same for the target.
fn sees(level: &Level, feet: Vec3, eye: f64, p: Player) -> bool {
    level.mw.0.line_of_sight_for(Some(feet.y), Vec3::new(feet.x, feet.y + eye, feet.z), Vec3::new(p.x, p.eye(), p.z))
}
/// A player moved within the follow range of an enderman: the enderman's goal is asked (the selector asks `can_use`).
fn stare_wake(on: Res<Online>, index: Res<Index>, q: Query<(&Pos, &FollowRange, &GoalSet), With<Stare>>, mut reqs: ResMut<Requests>) {
    if q.is_empty() {
        return;
    }
    for (_, new) in &on.seen {
        let Some(p) = *new else { continue };
        index.for_radius(p.x, p.z, 64.0, |c| {
            let Ok((pos, range, g)) = q.get(c) else { return };
            if d2(pos.0, Vec3::new(p.x, p.y, p.z)) <= range.0 * range.0
                && let Some(slot) = g.slot_of(&MODULE)
            {
                reqs.0.push(Req::Start(c, slot));
            }
        });
    }
}
/// `can_use`: the nearest player within the follow range who is eligible, stares at the enderman and is in sight.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return false };
    let eye = w.get::<Eye>(e).map_or(1.0, |l| l.0);
    let range = w.get::<FollowRange>(e).map_or(16.0, |f| f.0);
    let level = w.resource::<Level>();
    let ps = w.resource::<crate::hostile::PlayerState>();
    let mut best: Option<(u32, f64)> = None;
    w.resource::<Online>().for_near(pos.x, pos.z, range, |p| {
        let d = d2(pos, Vec3::new(p.x, p.y, p.z));
        if d <= range * range
            && eligible(ps, p.id)
            && best.is_none_or(|b| d < b.1)
            && stared(level, ps, p, pos, eye)
            && sees(level, pos, eye, p)
        {
            best = Some((p.id, d));
        }
    });
    match best {
        Some((id, _)) => {
            let mut st = w.get_mut::<Stare>(e).unwrap();
            st.pick = id;
            st.pending = true;
            true
        }
        None => false,
    }
}
/// `start`: the countdown and the teleport count begin; the goal runs (`Staring`).
fn start(w: &mut World, e: Entity, _: Inst) {
    {
        let mut st = w.get_mut::<Stare>(e).unwrap();
        st.aggro = st.p.aggro_ticks;
        st.teleport = 0;
    }
    w.entity_mut(e).insert(Staring);
}
/// `stop`: the countdown is off; the target is cleared when this goal holds it.
fn stop(w: &mut World, e: Entity, inst: Inst) {
    if let Some(mut st) = w.get_mut::<Stare>(e) {
        st.pending = false;
    }
    w.entity_mut(e).remove::<Staring>();
    if matches!(w.get::<Target>(e), Some(t) if t.slot == inst.slot) {
        targeting::clear_target(w, e);
    }
}
/// `LookForPlayer.canContinueToUse` and `tick`: the goal ends when its player is no longer eligible, or (while pending)
/// when the stare is over; the eyes turn to the player (`turn_to`: 10 degrees a tick on both axes).
fn stare_run(
    on: Res<Online>,
    ps: Res<crate::hostile::PlayerState>,
    level: Res<Level>,
    mut q: Query<(Aim, &Eye, &mut Stare), With<Staring>>,
    mut reqs: ResMut<Requests>,
    mut cmd: Commands,
) {
    for ((e, pos, mut look, looking, _, _), eye, mut st) in &mut q {
        let Some(p) = on.get(st.pick) else {
            reqs.0.push(Req::End(e, st.slot));
            continue;
        };
        if !eligible(&ps, p.id) {
            reqs.0.push(Req::End(e, st.slot));
            continue;
        }
        let feet = pos.0;
        if st.pending {
            if !stared(&level, &ps, p, feet, eye.0) {
                reqs.0.push(Req::End(e, st.slot));
                continue;
            }
            aim_clamped(&mut cmd, e, &mut look, looking, [p.x, p.eye(), p.z], Some(10.0), Some(10.0));
            st.aggro -= 1;
            if st.aggro <= 0 {
                st.pending = false;
                let (slot, id) = (st.slot, p.id);
                cmd.queue(move |w: &mut World| {
                    // `setAITarget` with no sight rule and no range: `ignoreLineOfSight` (`must_see` off, the follow never ends).
                    let mut t = Target::new(Who::Player(id), 0, slot);
                    t.must_see = false;
                    t.follow_scale = f64::INFINITY;
                    targeting::set_target(w, e, t);
                });
            }
        } else {
            let d = d2(feet, Vec3::new(p.x, p.y, p.z));
            if stared(&level, &ps, p, feet, eye.0) {
                if d < st.p.near_sqr {
                    st.asked_near += 1;
                    cmd.queue(move |w: &mut World| {
                        super::ender_teleport::random(w, e);
                    });
                }
                st.teleport = 0;
            } else {
                let far = d > st.p.far_sqr;
                let t = st.teleport;
                if far {
                    st.teleport += 1;
                }
                if far && t >= st.p.far_ticks {
                    // DOUBT: the rule's `teleport_towards` may fail; the count is taken as done here.
                    st.asked_far += 1;
                    st.teleport = 0;
                    let (at, eye_y) = (Vec3::new(p.x, p.y, p.z), p.eye());
                    cmd.queue(move |w: &mut World| {
                        super::ender_teleport::towards(w, e, at, eye_y);
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 1, &Params::DEFAULT)];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    /// A cow body renamed enderman, whose only target goal is the stare.
    fn enderman(h: &mut Herd, x: f64, z: f64) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:enderman";
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, x, z, 7);
        h.world.flush();
        e
    }
    /// The player at `d` blocks east of the enderman (same height), looking at it or not.
    fn player(h: &mut Herd, e: Entity, d: f64, looks: bool) {
        let p = h.world.get::<Pos>(e).unwrap().0;
        let eye = h.world.get::<targeting::Eye>(e).unwrap().0;
        let (px, py, pz) = (p.x + d, p.y, p.z);
        let at = [p.x, p.y + eye, p.z];
        let from = [px, py + 1.62, pz];
        let dir = [at[0] - from[0], at[1] - from[1], at[2] - from[2]];
        let n = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        let gaze = if looks { [dir[0] / n, dir[1] / n, dir[2] / n] } else { [-dir[0] / n, -dir[1] / n, -dir[2] / n] };
        let mut ps = h.world.resource_mut::<crate::hostile::PlayerState>();
        ps.set_gaze(0, gaze);
        drop(ps);
        h.world.resource_mut::<Online>().set(Player { id: 0, x: px, y: py, z: pz, main: 0, off: 0, ..Default::default() });
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    fn target_of(h: &Herd, e: Entity) -> Option<Who> {
        h.world.get::<Target>(e).map(|t| t.who)
    }

    /// A staring player six blocks away in sight is taken as the target; the countdown runs first.
    #[test]
    fn a_staring_player_in_sight_becomes_the_target_after_the_countdown() {
        let Some(mut h) = herd() else { return };
        let e = enderman(&mut h, 0.5, 0.5);
        h.tick();
        player(&mut h, e, 6.0, true);
        let mut started = None;
        for _ in 0..10 {
            h.tick();
            if running(&h, e) && started.is_none() {
                started = Some(now_of(&h.world, e));
            }
        }
        let start = started.expect("the stare starts the goal");
        assert_eq!(target_of(&h, e), Some(Who::Player(0)), "after the countdown (start {start}) the player is the target");
    }

    /// Without a stare the goal never starts; the same player who looks at the enderman does start it (the witness).
    #[test]
    fn a_player_who_looks_away_is_not_chosen_and_one_who_looks_is() {
        let Some(mut h) = herd() else { return };
        let e = enderman(&mut h, 0.5, 0.5);
        h.tick();
        player(&mut h, e, 6.0, false);
        for _ in 0..6 {
            h.tick();
        }
        assert!(!running(&h, e) && target_of(&h, e).is_none(), "a player who looks away is ignored");
        player(&mut h, e, 6.0, true);
        for _ in 0..6 {
            h.tick();
        }
        assert!(running(&h, e), "the same player looking at it starts the goal");
    }

    /// A creative player is never taken; the target of a player who stops being eligible is dropped and the goal ends.
    #[test]
    fn a_creative_player_is_never_taken_and_the_target_is_dropped_when_he_turns_creative() {
        let Some(mut h) = herd() else { return };
        let e = enderman(&mut h, 0.5, 0.5);
        h.tick();
        h.world.resource_mut::<crate::hostile::PlayerState>().set_creative(0, true);
        player(&mut h, e, 6.0, true);
        for _ in 0..10 {
            h.tick();
        }
        assert!(!running(&h, e), "a creative player does not start the goal");
        h.world.resource_mut::<crate::hostile::PlayerState>().set_creative(0, false);
        player(&mut h, e, 6.0, true);
        for _ in 0..10 {
            h.tick();
        }
        assert_eq!(target_of(&h, e), Some(Who::Player(0)), "the same player, not creative, is the target");
        h.world.resource_mut::<crate::hostile::PlayerState>().set_creative(0, true);
        for _ in 0..4 {
            h.tick();
        }
        assert!(!running(&h, e) && target_of(&h, e).is_none(), "turned creative: the goal ends and the target is dropped");
    }

    /// The target is kept far beyond the follow range (no range rule), and far away it asks for a teleport after fifteen ticks.
    #[test]
    fn the_target_is_kept_far_away_and_a_far_target_asks_for_a_teleport() {
        let Some(mut h) = herd() else { return };
        let e = enderman(&mut h, 0.5, 0.5);
        h.tick();
        player(&mut h, e, 6.0, true);
        for _ in 0..10 {
            h.tick();
        }
        assert_eq!(target_of(&h, e), Some(Who::Player(0)));
        player(&mut h, e, 30.0, false);
        for _ in 0..40 {
            h.tick();
        }
        assert_eq!(target_of(&h, e), Some(Who::Player(0)), "kept at 30 blocks, not looking");
        assert!(h.world.get::<Stare>(e).unwrap().asked_far >= 1, "a far target asks for a teleport");
    }
}
