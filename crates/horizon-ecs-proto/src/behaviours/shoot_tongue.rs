//! `ShootTongue` (`mobs/frog.rs:503-700`) with `FrogAttackablesSensor`: a frog walks up to a slime or magma cube of size 1 within 10
//! blocks and in sight, shoots its tongue at 1.75 blocks, draws the prey in (velocity 0.75 toward the frog), and eats it 7 ticks later.
//! The scan is one deadline per 20 own ticks while the goal is not running (the sensor's period), not a system. Deviations (`suivi/ecs-ecarts.md`):
//! the prey goes by `despawn::remove` without damage or drop; the path check of `can_use` is replaced by the timeout (100 ticks): a prey
//! that was not reached is left alone for 100 ticks.
use super::croak::Croaking;
use super::animal_panic::AnimalPanicking;
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::size::Size;
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub detection: f64,
    pub scan_every: i32,
    pub eating_distance: f64,
    pub time_out: i32,
    pub unreachable_for: u64,
    /// The names of the prey (size 1), given by the species (the framework names none).
    pub prey: [&'static str; 2],
}
impl Params {
    pub const DEFAULT: Self =
        Self { detection: 10.0, scan_every: 20, eating_distance: 1.75, time_out: 100, unreachable_for: 100, prey: ["", ""] };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Move,
    Catch,
    Eat,
    Done,
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Tongue {
    pub slot: u8,
    p: Params,
    /// The prey the last scan chose.
    nearest: Option<Entity>,
    target: Option<Entity>,
    /// The prey left alone until this own time.
    skip: Option<(Entity, u64)>,
    phase: Phase,
    time: i32,
    path_counter: i8,
    eat_timer: i8,
}
/// The goal runs (its per-tick step has work).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Shooting;
/// The tongue is out (the pose `USING_TONGUE` of the Mob).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct TongueOut;

pub static MODULE: Module = Module {
    name: "shoot_tongue",
    flags: MOVE | LOOK,
    equip,
    arm,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals, Cap::Locomotion, Cap::Head],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Tongue {
        slot: inst.slot,
        p: *params::<Params>(p),
        nearest: None,
        target: None,
        skip: None,
        phase: Phase::Done,
        time: 0,
        path_counter: 0,
        eat_timer: 0,
    });
}
/// The first scan: `Brain` gives every sensor a random start (`Sensor.randomlyDelayStart`: `nextInt(scanRate)`), the sensor then scans when its
/// counter reaches 1 or less, so the first scan is at own tick `max(1, draw)`.
fn arm(w: &mut World, e: Entity, inst: Inst) {
    let every = w.get::<Tongue>(e).map_or(20, |t| t.p.scan_every);
    let first = crate::goals::with_rng(w, e, |r| r.next_int(every)).max(1);
    crate::goals::arm(w, e, inst.slot, first);
}
/// The next scans, one period later.
fn rearm(w: &mut World, e: Entity, inst: Inst) {
    let every = w.get::<Tongue>(e).map_or(20, |t| t.p.scan_every);
    crate::goals::arm(w, e, inst.slot, every);
}
/// What of a creature the tongue needs: its place, eye and species (`targeting::info`, which every creature with a body has).
struct View {
    pos: Vec3,
    eye: f64,
    species: u8,
}
fn view(w: &World, o: Entity) -> Option<View> {
    let i = crate::targeting::info(w, crate::targeting::Who::Entity(o))?;
    Some(View { pos: i.pos, eye: i.eye, species: i.species })
}
/// `can_eat`: a slime or magma cube of size 1 that lives.
fn edible(w: &World, who: Entity, p: &Params) -> bool {
    let Some(i) = view(w, who) else { return false };
    w.get::<Size>(who).is_some_and(|s| s.0 == 1) && p.prey.iter().any(|n| crate::species::registry().id(n) == Some(usize::from(i.species)))
}
/// `FrogAttackablesSensor`: the nearest edible prey within the detection distance and in sight, but those left alone.
fn scan(w: &World, e: Entity) -> Option<Entity> {
    let t = w.get::<Tongue>(e)?;
    let (pos, eye) = (w.get::<Pos>(e)?.0, w.get::<crate::targeting::Eye>(e)?.0);
    let now = now_of(w, e);
    let mw = w.resource::<Level>().mw.0;
    let mut best: Option<(f64, Entity)> = None;
    w.resource::<Index>().for_radius(pos.x, pos.z, t.p.detection + 2.0, |o| {
        if o == e || t.skip.is_some_and(|(s, until)| s == o && until > now) || !edible(w, o, &t.p) {
            return;
        }
        let Some(i) = view(w, o) else { return };
        let d = d2(pos, i.pos);
        if d < t.p.detection * t.p.detection
            && best.is_none_or(|b| d < b.0)
            && mw.line_of_sight(Vec3::new(pos.x, pos.y + eye, pos.z), Vec3::new(i.pos.x, i.eye, i.pos.z))
        {
            best = Some((d, o));
        }
    });
    best.map(|b| b.1)
}
/// The period of the sensor: scan, then wake the selector when a prey was found.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    let found = scan(w, e);
    if let Some(mut t) = w.get_mut::<Tongue>(e) {
        t.nearest = found;
    }
    if found.is_none() {
        rearm(w, e, inst);
    }
    found.is_some()
}
fn can_use(w: &mut World, e: Entity, inst: Inst) -> bool {
    let Some(t) = w.get::<Tongue>(e).copied() else { return false };
    let Some(id) = t.nearest else { return false };
    let free = w.get::<AnimalPanicking>(e).is_none() && w.get::<Croaking>(e).is_none() && w.get::<Following>(e).is_none() && w.get::<NeedsPath>(e).is_none();
    if free && edible(w, id, &t.p) {
        w.get_mut::<Tongue>(e).unwrap().target = Some(id);
        return true;
    }
    if !free {
        // The ask came while the frog was busy: the next period tries again.
        rearm(w, e, inst);
        return false;
    }
    w.get_mut::<Tongue>(e).unwrap().nearest = None;
    rearm(w, e, inst);
    false
}
fn start(w: &mut World, e: Entity, _: Inst) {
    let Some(t) = w.get::<Tongue>(e).copied() else { return };
    let Some(id) = t.target else { return };
    if let Some(i) = view(w, id) {
        super::ram::look(w, e, [i.pos.x, i.eye, i.pos.z]);
        go_to_at(w, e, [i.pos.x, i.pos.y, i.pos.z], 2.0, 0);
    }
    w.entity_mut(e).insert(Shooting);
    let mut m = w.get_mut::<Tongue>(e).unwrap();
    m.path_counter = 10;
    m.time = 0;
    m.phase = Phase::Move;
}
fn stop(w: &mut World, e: Entity, inst: Inst) {
    let now = now_of(w, e);
    if let Some(mut t) = w.get_mut::<Tongue>(e) {
        // A prey not eaten (timeout, panic, vanished) is left alone for a while.
        if let (Some(id), true) = (t.target, t.phase != Phase::Done || t.time > t.p.time_out) {
            t.skip = Some((id, now + t.p.unreachable_for));
        }
        t.target = None;
        t.nearest = None;
        t.phase = Phase::Done;
    }
    w.entity_mut(e).remove::<(TongueOut, Shooting)>();
    rearm(w, e, inst);
}
fn tick(w: &mut World) {
    tick_state(w, tongue_run);
}
fn tongue_run(q: Query<(Entity, &Tongue), With<Shooting>>, mut reqs: ResMut<Requests>) {
    for (e, _) in &q {
        reqs.0.push(Req::Call(e, step));
    }
}
/// `can_continue` then `tick` of the Mob, every tick of the goal.
fn step(w: &mut World, e: Entity) {
    let Some(mut t) = w.get::<Tongue>(e).copied() else { return };
    let Some(id) = t.target else { return };
    let slot = t.slot;
    let alive = view(w, id).is_some();
    if t.phase == Phase::Done || t.time > t.p.time_out || w.get::<AnimalPanicking>(e).is_some() || !alive {
        w.resource_mut::<Requests>().0.push(Req::End(e, slot));
        return;
    }
    let i = view(w, id).unwrap();
    super::ram::look(w, e, [i.pos.x, i.eye, i.pos.z]);
    t.time += 1;
    let me = w.get::<Pos>(e).map_or(i.pos, |p| p.0);
    match t.phase {
        Phase::Move => {
            if d2(i.pos, me) < t.p.eating_distance * t.p.eating_distance {
                w.entity_mut(e).insert(TongueOut);
                let pull = Vec3::new(me.x - i.pos.x, me.y - i.pos.y, me.z - i.pos.z).normalize().scale(0.75);
                set_velocity(w, id, pull);
                t.eat_timer = 0;
                t.phase = Phase::Catch;
            } else if t.path_counter <= 0 {
                go_to_at(w, e, [i.pos.x, i.pos.y, i.pos.z], 2.0, 0);
                t.path_counter = 10;
            } else {
                t.path_counter -= 1;
            }
        }
        Phase::Catch => {
            t.eat_timer += 1;
            if t.eat_timer > 6 {
                t.phase = Phase::Eat;
                crate::despawn::remove(w, id);
            }
        }
        Phase::Eat => {
            if t.eat_timer >= 10 {
                t.phase = Phase::Done;
            } else {
                t.eat_timer += 1;
            }
        }
        Phase::Done => {}
    }
    if let Some(mut x) = w.get_mut::<Tongue>(e) {
        *x = t;
    }
}
