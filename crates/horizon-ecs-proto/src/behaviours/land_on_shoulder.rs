//! `LandOnOwnersShoulderGoal` (`Mob` `tame.rs`, `LandOnOwnersShoulder`): a tame creature that is not ordered to sit and is older than
//! `age` ticks lands on its owner's shoulder when its box touches the owner's; the host does the landing (`HostOut::shoulder`) and
//! removes it. Flags 0: it takes no control. What wakes it: the owner entering the watch (`PlayerWatch`, laid by `follow_owner` or
//! here), the deadline of its age, the end of a sit order; while it runs, a system visits it each tick. Deviations: the pose of
//! sitting (`!isInSittingPose`) is the order itself here; the age is the creature's own time (`Clock::of`), not the `Mob`'s counter.
use super::*;
use crate::rules::land_on_shoulder::arm_wait;
use crate::{cow::*, goals::*, hostile::PlayerState, leash::Leash, tame, targeting::{player_box, Who}};

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// `canSitOnShoulder`: older than this many ticks.
    pub age: u64,
    /// Blocks within which the owner wakes the goal (the watch; touching needs far less).
    pub near: f32,
}
impl Params {
    pub const DEFAULT: Self = Self { age: 100, near: 5.0 };
}
/// The two shoulders of every player (left, right), each carrying the creature perched on it. THE state of the shoulders: the only
/// place that knows them, behind this narrow interface by player id, empty at rest (a player with both shoulders free has no entry).
/// When players become entities, this same type moves onto the player entity as a component and `Shoulders::of` is its query: the
/// module does not change. Written by event: `put` (landed), `release` (got off), `clear` (the player left, died or dived).
#[derive(Resource, Default, Debug)]
pub struct Shoulders(std::collections::HashMap<u32, [Option<Entity>; 2]>);
impl Shoulders {
    /// `Player.setEntityOnShoulder`: the left shoulder if free, else the right; `false` when both are taken.
    pub fn put(&mut self, player: u32, e: Entity) -> bool {
        let s = self.0.entry(player).or_default();
        if s.contains(&Some(e)) {
            return false;
        }
        match s.iter_mut().find(|x| x.is_none()) {
            Some(x) => {
                *x = Some(e);
                true
            }
            None => false,
        }
    }
    pub fn has_room(&self, player: u32) -> bool {
        self.0.get(&player).is_none_or(|s| s.iter().any(Option::is_none))
    }
    /// The creature got off (or died): its shoulder is free.
    pub fn release(&mut self, player: u32, e: Entity) {
        if let Some(s) = self.0.get_mut(&player) {
            s.iter_mut().filter(|x| **x == Some(e)).for_each(|x| *x = None);
            if s.iter().all(Option::is_none) {
                self.0.remove(&player);
            }
        }
    }
    /// The player left: both shoulders are forgotten.
    pub fn clear(&mut self, player: u32) {
        self.0.remove(&player);
    }
    pub fn on(&self, player: u32) -> [Option<Entity>; 2] {
        self.0.get(&player).copied().unwrap_or_default()
    }
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Shoulder {
    p: &'static Params,
    slot: u8,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Landing;

pub static MODULE: Module = Module {
    name: "land_on_shoulder",
    flags: 0,
    equip,
    arm,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Tamable, Cap::Body],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = params::<Params>(p);
    // Only a world that has such a creature has the resource: registering one for every world moves the ids of the others' components.
    w.init_resource::<Shoulders>();
    w.entity_mut(e).insert(Shoulder { p, slot: inst.slot });
    if w.get::<PlayerWatch>(e).is_none() {
        w.entity_mut(e).insert(PlayerWatch::new(p.near));
    }
}
/// Entering the active zone: asked once now (the owner may already be near), and once when the age passes `age`.
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let Some(s) = w.get::<Shoulder>(e).map(|s| s.p) else { return };
    w.resource_mut::<Requests>().0.push(Req::Start(e, inst.slot));
    let age = w.resource::<Clock>().of(e);
    if age <= s.age {
        crate::goals::arm(w, e, inst.slot, arm_wait(age, s.age));
    }
}
fn tick(w: &mut World) {
    tick_state(w, owner_events);
    tick_state(w, land_run);
}
fn owner_events(ev: Res<Events>, q: Query<(&tame::Owner, &Shoulder)>, mut reqs: ResMut<Requests>) {
    for (e, id) in ev.player_entered.iter() {
        if let Ok((o, s)) = q.get(e)
            && o.0 == Some(Who::Player(id))
        {
            reqs.0.push(Req::Start(e, s.slot));
        }
    }
    for (e, id) in ev.player_left.iter() {
        if let Ok((o, s)) = q.get(e)
            && o.0 == Some(Who::Player(id))
        {
            reqs.0.push(Req::End(e, s.slot));
        }
    }
}
/// The owner, when it is there, not a spectator.
fn owner_of(w: &World, e: Entity) -> Option<crate::goals::Player> {
    let p = tame::owner_player(w, e)?;
    (!w.resource::<PlayerState>().spectator(p.id)).then_some(p)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let Some(s) = w.get::<Shoulder>(e).map(|s| s.p) else { return false };
    owner_of(w, e).is_some_and(|p| w.resource::<Shoulders>().has_room(p.id)) && !is_occupied(w, e, OCC_SITTING) && w.resource::<Clock>().of(e) > s.age
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Landing);
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Landing>();
}
/// `tick`: boxes touching, not sitting, not on a lead: the host is asked once and the goal ends (the host removes the creature).
fn land_run(
    on: Res<Online>,
    ps: Res<PlayerState>,
    mut ev: ResMut<Events>,
    mut sh: ResMut<Shoulders>,
    q: Query<(Entity, &Bounds, &tame::Owner, &Shoulder, Option<&Occupied>, Option<&Leash>), With<Landing>>,
    mut reqs: ResMut<Requests>,
) {
    for (e, b, o, s, occ, leash) in &q {
        let Some(Who::Player(id)) = o.0 else { continue };
        let Some(p) = on.get(id).filter(|p| !ps.spectator(p.id)) else { continue };
        if occ.is_some_and(|o| o.0 & OCC_SITTING != 0) || leash.is_some() || !b.0.intersects(player_box(&p)) {
            continue;
        }
        if !sh.put(id, e) {
            continue;
        }
        ev.host.shoulder(e, id);
        reqs.0.push(Req::End(e, s.slot));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{species, spawn_species, Herd};
    use crate::host_out::HostEvent;
    use crate::species::Config;

    /// A world with `n` creatures of the process species whose only goal is this one, tame to player 1, who stands `dx` blocks away.
    fn world(n: usize, dx: f64) -> (Herd, Vec<Entity>) {
        let mut h = Herd::with_slab_awake(60, true, false);
        let mut sp = *species();
        let entries: &'static [Entry] = Box::leak(vec![Entry::new(&MODULE, 3, &Params::DEFAULT)].into_boxed_slice());
        sp.goals = Config { entries };
        let es: Vec<Entity> = (0..n).map(|i| spawn_species(&mut h.world, &sp, 0.5, 0.5, 70 + i as u64)).collect();
        for &e in &es {
            tame::set_owner(&mut h.world, e, Who::Player(1));
        }
        h.world.resource_mut::<Online>().set(Player { id: 1, x: 0.5 + dx, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        (h, es)
    }
    fn landings(h: &mut Herd) -> usize {
        h.drain_host_out().iter().filter(|e| matches!(e, HostEvent::ParrotToShoulder { .. })).count()
    }
    /// The owner touching an adult creature gets it on the left shoulder, once; a young one waits for its age.
    #[test]
    fn a_creature_touching_its_owner_lands_once_after_its_age() {
        if crate::cow::skip_unless_walker("land on shoulder") || crate::cow::skip_unless_caps(&[Cap::Tamable], "land on shoulder") {
            return;
        }
        let (mut h, es) = world(1, 0.0);
        for _ in 0..90 {
            h.tick();
        }
        assert_eq!(landings(&mut h), 0, "too young");
        for _ in 0..40 {
            h.tick();
        }
        assert_eq!(landings(&mut h), 1);
        assert_eq!(h.world.resource::<Shoulders>().on(1), [Some(es[0]), None]);
    }
    /// An ordered-to-sit creature, or one 3 blocks from the player, does not land; the third of three finds both shoulders taken.
    #[test]
    fn a_sitting_or_far_creature_does_not_land_and_two_shoulders_hold_two() {
        if crate::cow::skip_unless_walker("land on shoulder") || crate::cow::skip_unless_caps(&[Cap::Tamable], "land on shoulder") {
            return;
        }
        let (mut h, es) = world(1, 0.0);
        tame::order_sit(&mut h.world, es[0], true);
        for _ in 0..130 {
            h.tick();
        }
        assert_eq!(landings(&mut h), 0, "sitting");
        let (mut h, _) = world(1, 3.0);
        for _ in 0..130 {
            h.tick();
        }
        assert_eq!(landings(&mut h), 0, "far");
        let (mut h, _) = world(3, 0.0);
        for _ in 0..130 {
            h.tick();
        }
        assert_eq!(landings(&mut h), 2, "two shoulders");
    }
}
