//! `NeutralMob`'s persistent anger (`mobs/neutral_anger.rs`, `polar_bear.rs:172-196`). While the entity holds a target, its
//! anger is renewed; when the target is lost, the anger counts down `400 + nextInt(381)` ticks of the entity's own time,
//! then it ends. No flag and no start: a state module that the rules making a mob angry read (`is_angry`, `anger_at`).
//! Woken by the target's events only (`target_acquired`, `target_lost`); nothing polls.
use super::*;
use crate::cow::now_of;
use crate::goals::{Events, GoalSlot, Req, Requests};
use crate::targeting::{Target, Who};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub base: i32,
    pub spread: i32,
    /// Added to the movement speed while angry (a grown zombified piglin's `minecraft:attacking` modifier: 0.05).
    pub boost: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { base: 400, spread: 381, boost: 0.0 };
}

/// `holding`: a target is held (no countdown). `until`: the end of the countdown after the loss. `at`: whom the anger is
/// for, `at_player`: the Mob's `flag2` (angry at a player).
#[derive(Component, Clone, Copy, Debug)]
pub struct Anger {
    pub def: &'static Params,
    pub slot: u8,
    pub holding: bool,
    pub until: u64,
    pub at: Option<Who>,
    pub at_player: bool,
    /// The boost is on the speed.
    pub boosted: bool,
}
/// The countdown runs: its marker. Absent while the target is held or when there is no anger.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Angry;

pub static MODULE: Module = Module {
    name: "neutral_anger",
    flags: 0,
    equip,
    fired,
    can_use,
    systems,
    requires: &[Cap::Goals, Cap::Targets],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let def = params::<Params>(p);
    w.entity_mut(e).insert(Anger { def, slot: inst.slot, holding: false, until: 0, at: None, at_player: false, boosted: false });
}
/// A state module: nothing starts it.
fn can_use(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
/// The countdown's deadline: a countdown still running is armed again (the wheel holds at most `WHEEL_MAX` ticks); at its
/// end the anger stops.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(a) = w.get::<Anger>(e).copied() else { return false };
    if w.get::<Angry>(e).is_none() {
        return false;
    }
    if now_of(w, e) < a.until {
        arm_to(w, e, a.slot, a.until);
    } else {
        w.entity_mut(e).remove::<Angry>();
        let mut m = w.get_mut::<Anger>(e).unwrap();
        m.at = None;
        m.at_player = false;
        drop(m);
        set_boost(w, e, false);
    }
    false
}

fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(hear.in_set(slot));
}
/// The target events of this tick, for the entities that carry the anger (the others cost nothing).
fn hear(ev: Res<Events>, mut reqs: ResMut<Requests>, q: Query<(), With<Anger>>) {
    if q.is_empty() {
        return;
    }
    for e in ev.target_acquired.iter().filter(|&e| q.contains(e)) {
        reqs.0.push(Req::Call(e, acquired));
    }
    for e in ev.target_lost.iter().filter(|&e| q.contains(e)) {
        reqs.0.push(Req::Call(e, lost));
    }
}
/// A target is held: the anger is renewed (the countdown stops), for this target.
fn acquired(w: &mut World, e: Entity) {
    let Some(who) = w.get::<Target>(e).map(|t| t.who) else { return };
    let Some(mut a) = w.get_mut::<Anger>(e) else { return };
    a.holding = true;
    a.at = Some(who);
    a.at_player = matches!(who, Who::Player(_));
    drop(a);
    set_boost(w, e, true);
    w.entity_mut(e).remove::<Angry>();
}
/// The target is lost: the countdown starts from `base + nextInt(spread)` ticks (one draw, the law of the Mob's last draw).
fn lost(w: &mut World, e: Entity) {
    let Some(a) = w.get::<Anger>(e).copied() else { return };
    if !a.holding {
        return;
    }
    let wait = a.def.base + crate::goals::with_rng(w, e, |r| r.next_int(a.def.spread));
    let until = now_of(w, e) + wait as u64;
    {
        let mut m = w.get_mut::<Anger>(e).unwrap();
        m.holding = false;
        m.until = until;
    }
    w.entity_mut(e).insert(Angry);
    arm_to(w, e, a.slot, until);
}

/// Whether the entity is angry: it holds a target or its countdown runs.
pub fn is_angry(w: &World, e: Entity) -> bool {
    w.get::<Anger>(e).is_some_and(|a| a.holding) || w.get::<Angry>(e).is_some()
}
/// Whom the anger is for, if it is angry.
pub fn anger_at(w: &World, e: Entity) -> Option<Who> {
    if is_angry(w, e) {
        w.get::<Anger>(e).and_then(|a| a.at)
    } else {
        None
    }
}

/// Arms the deadline at `at` (the entity's own time), as far as the wheel reaches.
fn arm_to(w: &mut World, e: Entity, slot: u8, at: u64) {
    let now = now_of(w, e);
    crate::goals::arm(w, e, slot, at.saturating_sub(now).min(i32::MAX as u64) as i32);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{species, spawn_species, Herd, Species};
    use crate::species::Config;
    use crate::targeting::{clear_target, set_target};

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 255, &Params::DEFAULT)];

    /// Two cows: `e` carries the anger, `foe` is the target it holds (alive, so the targeting keeps it).
    fn pair(seed: u64) -> (Herd, Entity, Entity) {
        let mut h = Herd::with_slab(8, true, false);
        let mut sp: Species = *species();
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, seed);
        let foe = spawn_species(&mut h.world, &sp, 3.5, 0.5, seed + 1);
        crate::goals::arm_all(&mut h.world);
        (h, e, foe)
    }
    fn hold(h: &mut Herd, e: Entity, foe: Entity) {
        let mut t = Target::new(Who::Entity(foe), u16::MAX, u8::MAX);
        t.must_see = false;
        t.follow_scale = f64::INFINITY;
        set_target(&mut h.world, e, t);
    }
    fn angry_marker(h: &Herd, e: Entity) -> bool {
        h.world.get::<Angry>(e).is_some()
    }

    /// Held: angry with no countdown. Lost: angry for a countdown of 400 to 780 ticks, then calm. A target taken again
    /// before the end renews the anger, which then stops counting.
    #[test]
    fn a_held_target_keeps_the_anger_and_its_loss_starts_a_countdown() {
    if crate::cow::skip_unless_walker("a_held_target_keeps_the_anger_and_its_loss_starts_a_countdown") {
        return;
    }
        let (mut h, e, foe) = pair(11);
        hold(&mut h, e, foe);
        for _ in 0..30 {
            h.tick();
        }
        assert!(is_angry(&h.world, e) && !angry_marker(&h, e), "held: not angry, or counting down");
        assert_eq!(anger_at(&h.world, e), Some(Who::Entity(foe)), "the anger is not for the held target");
        clear_target(&mut h.world, e);
        h.tick();
        assert!(angry_marker(&h, e), "lost target: no countdown started");
        let lost_at = now_of(&h.world, e);
        let until = h.world.get::<Anger>(e).unwrap().until;
        assert!((400..=780).contains(&(until - lost_at)), "countdown {} outside 400..=780", until - lost_at);
        let mut ended = None;
        for _ in 0..1000 {
            h.tick();
            if !angry_marker(&h, e) {
                ended = Some(now_of(&h.world, e));
                break;
            }
        }
        let ended = ended.expect("the anger never ended");
        assert!(ended >= until && ended <= until + 2, "ended at {ended}, countdown end {until}");
        assert!(!is_angry(&h.world, e) && anger_at(&h.world, e).is_none(), "calm, but still angry or still for someone");
    }

    /// A target held again during the countdown stops it: the anger does not end at the old deadline.
    #[test]
    fn a_target_taken_again_before_the_end_renews_the_anger() {
    if crate::cow::skip_unless_walker("a_target_taken_again_before_the_end_renews_the_anger") {
        return;
    }
        let (mut h, e, foe) = pair(12);
        hold(&mut h, e, foe);
        h.tick();
        clear_target(&mut h.world, e);
        h.tick();
        assert!(angry_marker(&h, e), "no countdown after the loss (witness)");
        for _ in 0..100 {
            h.tick();
        }
        hold(&mut h, e, foe);
        for _ in 0..800 {
            h.tick();
        }
        assert!(is_angry(&h.world, e) && !angry_marker(&h, e), "the held target did not renew the anger past its countdown");
    }
}

/// The anger's speed boost goes on (not for a baby) or off, once.
fn set_boost(w: &mut World, e: Entity, on: bool) {
    let Some(a) = w.get::<Anger>(e).copied() else { return };
    if a.def.boost == 0.0 || a.boosted == on || (on && w.get::<crate::cow::GrowAt>(e).is_some()) {
        return;
    }
    w.get_mut::<Anger>(e).unwrap().boosted = on;
    if let Some(mut b) = w.get_mut::<crate::cow::Body>(e) {
        b.speed += if on { a.def.boost } else { -a.def.boost };
    }
}
