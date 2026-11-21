//! `AbstractSchoolingFish.FollowFlockLeaderGoal` (fiche 44, `aquatic.rs:858-995`): a fish with no school, every ten seconds or
//! so, joins a school of its kind nearby that has room, or gathers the fish around it that follow no one into a school of its
//! own; a follower swims after its leader while the leader lives within 11 blocks. The search is a deadline: each search draws
//! the next delay, and a fish that leads a school does not count down (`frozen`) until its last follower leaves.
use super::*;
use crate::{cow::*, goals::*};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub speed: f64,
    /// The box of the search is the fish's box inflated by this (`bounds.inflate(8)`).
    pub area: f64,
    /// `maxSchoolSize` of the species (cod 8, salmon 5, tropical fish 8: `aquatic.rs:783`).
    pub max_school: i32,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 1.0, area: 8.0, max_school: 8 };
}

/// The school of a fish: whom it follows, how many follow it, the asks left before its next search (`next`), the local time
/// of its only valid deadline (`armed_at`; the older ones match nothing), and the leader's countdown frozen (`frozen`).
#[derive(Component, Clone, Copy, Debug)]
pub struct School {
    pub leader: Option<Entity>,
    pub followers: i32,
    pub next: i32,
    pub armed_at: u64,
    pub frozen: bool,
    pub recalc: i32,
    pub slot: u8,
    pub max_school: i32,
    pub speed: f64,
    pub area: f64,
}
/// The fish is in its goal: it swims after its leader (the systems of the goal run for it only).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Following;

pub static MODULE: Module = Module {
    name: "follow_flock_leader",
    flags: 0,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};

fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(School {
        leader: None,
        followers: 0,
        next: 0,
        armed_at: u64::MAX,
        frozen: false,
        recalc: 0,
        slot: inst.slot,
        max_school: p.max_school,
        speed: p.speed,
        area: p.area,
    });
}
/// `reduced(n)`: `Goal.reducedTickDelay`.
fn reduced(n: i32) -> i32 {
    (n + 1) / 2
}
/// `nextStartTick` drawn on the fish's random: `reduced(200 + nextInt(200) % 20)`.
fn next_start_tick(w: &mut World, e: Entity) -> i32 {
    with_rng(w, e, |r| reduced(200 + r.next_int(200) % 20))
}
/// The first delay, drawn at creation (the `Mob` draws it in the constructor).
fn arm(w: &mut World, e: Entity, _: Inst) {
    let next = next_start_tick(w, e);
    w.get_mut::<School>(e).unwrap().next = next;
    arm_asks(w, e, next);
}
/// Arms the deadline `asks` asks from now: a selector asks `canUse` every 2 ticks of a goal with no flag.
fn arm_asks(w: &mut World, e: Entity, asks: i32) {
    let slot = w.get::<School>(e).unwrap().slot;
    let at = now_of(w, e) + 2 * (asks.max(0) as u64 + 1);
    w.get_mut::<School>(e).unwrap().armed_at = at;
    arm_at(w, e, at, Due::Goal(slot));
}
fn alive(w: &World, e: Entity) -> bool {
    w.get::<Health>(e).is_some_and(|h| h.0 > 0.0)
}
/// `isFollower`: its leader is alive.
fn leads_alive(w: &World, leader: Option<Entity>) -> bool {
    leader.is_some_and(|l| alive(w, l))
}
fn following(w: &World, e: Entity) -> bool {
    leads_alive(w, w.get::<School>(e).and_then(|s| s.leader))
}
/// `canBeFollowed`: it has followers and room for more.
fn can_be_followed(max: i32, followers: i32) -> bool {
    followers > 0 && followers + 1 < max
}
/// A deadline of this fish came up (or a replayed start for a fish that follows): true = ask `canUse` now. A deadline that
/// is not the current one (a stale one, or a deadline of a goal that is running) is nothing.
fn fired(w: &mut World, e: Entity, _: Inst) -> bool {
    if w.get::<Following>(e).is_some() {
        return false;
    }
    if following(w, e) {
        return true;
    }
    let now = now_of(w, e);
    w.get::<School>(e).is_some_and(|s| s.armed_at == now)
}
/// `FollowFlockLeaderGoal.canUse`: a school with a leader that follows no one is not searched; a fish that leads a school
/// does not count down (`frozen`: the countdown resumes when its last follower leaves); a search draws the next delay and
/// then looks for the school to join, or gathers the fish that follow no one around itself.
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let s = *w.get::<School>(e).unwrap();
    if s.followers > 0 {
        w.get_mut::<School>(e).unwrap().frozen = true;
        return false;
    }
    if following(w, e) {
        return true;
    }
    let next = next_start_tick(w, e);
    w.get_mut::<School>(e).unwrap().next = next;
    if search(w, e) {
        return true;
    }
    arm_asks(w, e, next);
    false
}
/// The search of `FollowFlockLeaderGoal.canUse`: the fish of its species in its box inflated by `area`, alive, ordered as
/// `getEntitiesOfClass` orders them (`aquatic.rs`, the sections by x, z, y, then the entry order). The leader is the first one
/// that can be followed, else the fish itself; then the fish that follow no one, in the same order, take the room left, the
/// leader itself is skipped after it was counted (`take` before `filter`, as the `Mob`). Returns whether the fish follows.
fn search(w: &mut World, e: Entity) -> bool {
    let me = *w.get::<School>(e).unwrap();
    let species = w.get::<SpeciesId>(e).unwrap().0;
    let area = w.get::<Bounds>(e).unwrap().0.inflate(Vec3::new(me.area, me.area, me.area));
    let p = w.get::<Pos>(e).unwrap().0;
    let mut near = Vec::new();
    w.resource::<Index>().for_radius(p.x, p.z, me.area, |c| near.push(c));
    // (entity, section, entry order, whom it follows, its followers)
    let mut cands: Vec<(Entity, [i32; 3], u64, Option<Entity>, i32)> = Vec::new();
    for c in near {
        let (Some(sp), Some(b), Some(cs), Some(pos), Some(uid)) =
            (w.get::<SpeciesId>(c), w.get::<Bounds>(c), w.get::<School>(c), w.get::<Pos>(c), w.get::<Uid>(c))
        else {
            continue;
        };
        if sp.0 != species || !alive(w, c) || !b.0.intersects(area) {
            continue;
        }
        let q = pos.0;
        let section = [(q.x.floor() as i32) >> 4, (q.y.floor() as i32) >> 4, (q.z.floor() as i32) >> 4];
        cands.push((c, section, uid.0, cs.leader, cs.followers));
    }
    // `getEntitiesOfClass`: the sections by x, z, y; in each, as they entered it. Here: the entry order is the spawn
    // order (`Uid`), not the order of entry into the section (see the notes: DOUTE).
    cands.sort_by_key(|c| (c.1[0], c.1[2], c.1[1], c.2));
    cands.retain(|c| can_be_followed(me.max_school, c.4) || !leads_alive(w, c.3));
    let (leader, size) = match cands.iter().find(|c| can_be_followed(me.max_school, c.4)) {
        Some(c) => (c.0, c.4 + 1),
        None => (e, me.followers + 1),
    };
    let room = (me.max_school - size).max(0) as usize;
    let picks: Vec<(Entity, Option<Entity>)> = cands.iter().filter(|c| !leads_alive(w, c.3)).take(room).map(|c| (c.0, c.3)).collect();
    for (c, _) in picks {
        if c == leader {
            continue;
        }
        if c == e {
            w.get_mut::<School>(e).unwrap().leader = Some(leader);
        } else if let Some(mut cs) = w.get_mut::<School>(c) {
            cs.leader = Some(leader);
            wake(w, c, &MODULE);
        } else {
            continue;
        }
        if let Some(mut ls) = w.get_mut::<School>(leader) {
            ls.followers += 1;
        }
    }
    following(w, e)
}
/// `FollowFlockLeaderGoal.start`: the path is asked at the first tick. The countdown the fish keeps is what was left of its
/// deadline (or none when it froze at its end).
fn start(w: &mut World, e: Entity, _: Inst) {
    let now = now_of(w, e);
    let mut s = *w.get::<School>(e).unwrap();
    if s.armed_at > now {
        s.next = (((s.armed_at - now) / 2).saturating_sub(1)) as i32;
    } else if s.frozen {
        s.next = 0;
    }
    s.frozen = false;
    s.recalc = 0;
    w.entity_mut(e).insert((s, Following));
}
/// `FollowFlockLeaderGoal.stop`: the leader loses a follower (and, if none is left and it was frozen, searches again at its
/// next ask); the fish counts its next delay from now.
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Following>();
    let leader = w.get_mut::<School>(e).unwrap().leader.take();
    if let Some(l) = leader {
        let freed = match w.get_mut::<School>(l) {
            Some(mut ls) => {
                ls.followers -= 1;
                let freed = ls.followers <= 0 && ls.frozen;
                if freed {
                    ls.frozen = false;
                }
                freed
            }
            None => false,
        };
        if freed {
            arm_asks(w, l, 0);
        }
    }
    let next = w.get::<School>(e).unwrap().next;
    arm_asks(w, e, next);
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems(follow_run.in_set(slot));
}
/// The running followers: the goal ends when the leader is gone or farther than 11 blocks (`canContinueToUse`, checked every
/// tick); every second tick (the ticks where the selector does not ask) the path to the leader is asked again every 5
/// such ticks (`reduced(10)`).
fn follow_run(w: &mut World, q: &mut QueryState<Entity, With<Following>>) {
    let list: Vec<Entity> = q.iter(w).collect();
    for e in list {
        let s = *w.get::<School>(e).unwrap();
        let Some(leader) = s.leader.filter(|&l| alive(w, l)) else {
            end_goal(w, e, s.slot);
            continue;
        };
        let (a, b) = (w.get::<Pos>(e).unwrap().0, w.get::<Pos>(leader).unwrap().0);
        if a.distance_squared(b) > 121.0 {
            end_goal(w, e, s.slot);
            continue;
        }
        if (now_of(w, e) + phase_of(w, e)) % 2 == 1 {
            let mut m = w.get_mut::<School>(e).unwrap();
            m.recalc -= 1;
            if m.recalc <= 0 {
                m.recalc = reduced(10);
                let speed = m.speed;
                drop(m);
                go_to_entity(w, e, leader, speed, 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;

    static ENTRIES: [Entry; 1] = [Entry::new(&MODULE, 5, &Params::DEFAULT)];

    fn herd() -> Option<Herd> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(30, false, false);
        set_lazy_look(&mut h.world, false);
        Some(h)
    }
    /// A fish: the cow's body renamed, with this module alone.
    fn fish(h: &mut Herd, x: f64, z: f64, seed: u64) -> Entity {
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.name = "minecraft:cod";
        sp.goals = Config { entries: &ENTRIES };
        let e = spawn_species(&mut h.world, &sp, x, z, seed);
        h.world.flush();
        h.world.resource_mut::<Requests>().0.push(Req::Arm(e));
        h.world.flush();
        e
    }
    fn school(h: &Herd, e: Entity) -> School {
        *h.world.get::<School>(e).unwrap()
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    /// Asks the fish now (the framework's start request: `canUse` runs at its next selector tick).
    fn ask(h: &mut Herd, e: Entity) {
        let slot = school(h, e).slot;
        h.world.resource_mut::<Requests>().0.push(Req::Start(e, slot));
    }
    fn teleport(h: &mut Herd, e: Entity, x: f64, z: f64) {
        h.teleport(e, Vec3::new(x, 1.0, z));
    }

    /// Departure: a fish that searches alone leads, and the fish near it joins it; the follower starts (witness: it runs,
    /// its leader is the first fish, the leader counts one follower; the leader does not run).
    #[test]
    fn a_fish_joins_the_one_that_leads_and_the_leader_stays_still() {
        let Some(mut h) = herd() else { return };
        let a = fish(&mut h, 14.5, 0.5, 13);
        let b = fish(&mut h, 15.5, 0.5, 47);
        ask(&mut h, a);
        for _ in 0..4 {
            h.tick();
        }
        assert_eq!(school(&h, b).leader, Some(a), "the other fish of the school joins the one that searched");
        assert!(running(&h, b), "the follower runs");
        assert_eq!(school(&h, a).followers, 1, "the leader counts one follower");
        assert!(!running(&h, a), "the leader does not follow anyone");
    }

    /// Pursuit and end: the follower stays while its leader is within 11 blocks (witness after 200 ticks), and stops
    /// when the leader is 20 blocks away: the leader loses its follower.
    #[test]
    fn a_follower_stays_within_eleven_blocks_and_stops_beyond() {
        let Some(mut h) = herd() else { return };
        let a = fish(&mut h, 14.5, 0.5, 13);
        let b = fish(&mut h, 15.5, 0.5, 47);
        ask(&mut h, a);
        for _ in 0..4 {
            h.tick();
        }
        assert!(running(&h, b), "witness: the follower runs before the test");
        for _ in 0..200 {
            h.tick();
        }
        assert!(running(&h, b), "within 11 blocks the follower keeps swimming after its leader");
        teleport(&mut h, a, 14.5 + 20.0, 0.5);
        for _ in 0..4 {
            h.tick();
        }
        assert!(!running(&h, b), "the leader 20 blocks away: the follower stops");
        assert_eq!(school(&h, b).leader, None);
        assert_eq!(school(&h, a).followers, 0, "the leader loses its follower");
    }

    /// Order of the candidates: the school whose leader lies in the first section by x is joined, even when its fish was
    /// spawned after the other one (the entry order of the spawn is the tie-break only within a section).
    #[test]
    fn the_first_section_by_x_chooses_the_school_to_join() {
        let Some(mut h) = herd() else { return };
        let late = fish(&mut h, 17.5, 0.5, 13);
        let early = fish(&mut h, 14.5, 0.5, 47);
        let me = fish(&mut h, 16.5, 0.5, 91);
        for &l in &[early, late] {
            h.world.get_mut::<School>(l).unwrap().followers = 1;
        }
        ask(&mut h, me);
        for _ in 0..4 {
            h.tick();
        }
        assert_eq!(school(&h, me).leader, Some(early), "section x 0 comes before section x 1");
        assert!(running(&h, me), "the searching fish joins the school and runs");
    }
}
