//! Fiche 103, `AnimalPanic` (`horizon-client/src/mobs/animal_panic.rs:11-160`) : la panique des animaux à cerveau (chameau,
//! grenouille, nautile). Module NEUF, distinct de `panic` (qui s'arrête à l'arrivée). Départ après un dégât de cause
//! « panique » (`LastDamage`, lu au tick suivant `Events.hurt`), durée `min + nextInt(spread)`, un nouveau point à chaque
//! fin de chemin, point d'eau si en feu. Pas de système : `tick` (règle du cadre, LISEZMOI « Les ticks des modules »).
use super::*;
use crate::brain::LastDamage;
use crate::cow::*;
use crate::goals::*;
use crate::mount::Mount;
use crate::spots::{self, Pick};
use crate::targeting::Who;

/// Which rider holds the panic back: a player (`m.rider`, the camel), or any rider (the husk: `extra.rider` too).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Riders {
    Player,
    Any,
}

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The speed of the run (4,0 camel, 2,0 frog, 1,6 nautilus).
    pub speed: f64,
    pub riders: Riders,
    /// The camel stands up at once when the panic starts (`CamelPanic.start`).
    pub stand_up: bool,
    /// Duration: `min + nextInt(spread)` ticks.
    pub min: u64,
    pub spread: i32,
    /// Land spot: `h` blocks around, `v` up or down. Water within `water` blocks while burning.
    pub h: i32,
    pub v: i32,
    pub water: i32,
    /// Ticks after a damage during which the panic can start.
    pub window: u64,
}
impl Params {
    pub const DEFAULT: Self = Self { speed: 2.0, riders: Riders::Player, stand_up: false, min: 100, spread: 21, h: 5, v: 4, water: 5, window: 40 };
}

/// The parameters and the state of the running panic: its rank and its end (the own time after which it stops).
#[derive(Component, Clone, Copy, Debug)]
pub struct Fleeing {
    pub p: Params,
    pub slot: u8,
    pub end: u64,
}
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct AnimalPanicking;

pub static MODULE: Module = Module {
    name: "animal_panic",
    flags: MOVE,
    retry_on_end: true,
    equip,
    fired,
    can_use,
    start,
    stop,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Living, Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Fleeing { p: *params::<Params>(p), slot: inst.slot, end: 0 });
}
fn tick(w: &mut World) {
    tick_state(w, hurt_run);
    tick_state(w, path_run);
}
/// A damage that panics the animal, read the tick after its event: the goal asks to start (the window is checked by `can_use`).
fn hurt_run(ev: Res<Events>, clock: Res<Clock>, mut reqs: ResMut<Requests>, q: Query<(&Fleeing, Option<&LastDamage>, Has<AnimalPanicking>)>) {
    for e in ev.hurt.iter() {
        let Ok((f, last, running)) = q.get(e) else { continue };
        if running {
            continue;
        }
        if last.is_some_and(|d| d.panic && clock.of(e).saturating_sub(d.at) <= f.p.window) {
            reqs.0.push(Req::Start(e, f.slot));
        }
    }
}
/// A running panic whose path is over takes its next spot (`Mob`: every tick the navigation is done).
fn path_run(mut reqs: ResMut<Requests>, q: Query<Entity, (With<AnimalPanicking>, Without<Following>, Without<NeedsPath>)>) {
    for e in &q {
        reqs.0.push(Req::Call(e, next_spot));
    }
}
fn next_spot(w: &mut World, e: Entity) {
    let Some(f) = w.get::<Fleeing>(e).copied() else { return };
    if let Some(p) = spot(w, e, &f.p) {
        go_to_at(w, e, p, f.p.speed, 1);
    }
}
/// `panic_pos`: the water within reach while burning, else a land spot.
fn spot(w: &mut World, e: Entity, p: &Params) -> Option<[f64; 3]> {
    let now = now_of(w, e);
    if w.get::<Burning>(e).is_some_and(|b| b.until > now)
        && let Some(s) = water_spot(w, e, p.water)
    {
        return Some(s);
    }
    spots::pick_for(w, e, Pick::Land { h: p.h, v: p.v })
}
/// `animal_water_pos`: the nearest water block; a body that rounds up to two blocks wide also needs the water to its
/// south, east and south-east. Nothing when the animal stands in a block with a collision shape.
fn water_spot(w: &mut World, e: Entity, range: i32) -> Option<[f64; 3]> {
    use horizon_client_lib::blocks::Fluid;
    let pos = w.get::<Pos>(e)?.0;
    let wide = w.get::<Body>(e).is_some_and(|b| f64::from(b.width).ceil() == 2.0);
    let mw = w.resource::<Level>().mw.0;
    let at = [pos.x.floor() as i32, pos.y.floor() as i32, pos.z.floor() as i32];
    if mw.data.state(mw.world.block(at[0], at[1], at[2])).top != 0.0 {
        return None;
    }
    let water = |x: i32, y: i32, z: i32| mw.fluid(x, y, z) == Fluid::Water;
    mw.nearest_block(at, range, 1, |p, _| {
        water(p[0], p[1], p[2]) && (!wide || (water(p[0], p[1], p[2] + 1) && water(p[0] + 1, p[1], p[2]) && water(p[0] + 1, p[1], p[2] + 1)))
    })
    .map(|p| [f64::from(p[0]) + 0.5, f64::from(p[1]), f64::from(p[2]) + 0.5])
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let now = now_of(w, e);
    let Some(f) = w.get::<Fleeing>(e).copied() else { return false };
    let rider = w.get::<Mount>(e).and_then(|m| m.rider);
    let held = match (f.p.riders, rider) {
        (Riders::Player, Some(Who::Player(_))) | (Riders::Any, Some(_)) => true,
        _ => false,
    };
    !held && w.get::<LastDamage>(e).is_some_and(|d| d.panic && now.saturating_sub(d.at) <= f.p.window)
}
fn start(w: &mut World, e: Entity, inst: Inst) {
    let now = now_of(w, e);
    let Some(f) = w.get::<Fleeing>(e).copied() else { return };
    if f.p.stand_up {
        super::posture::stand_up_instantly(w, e);
    }
    let dur = f.p.min + with_rng(w, e, |r| r.next_int(f.p.spread)) as u64;
    w.entity_mut(e).insert((Fleeing { end: now + dur, ..f }, AnimalPanicking));
    stop_nav(w, e);
    if let Some(p) = spot(w, e, &f.p) {
        go_to_at(w, e, p, f.p.speed, 1);
    }
    // One tick after the end: the `Mob` stops when `age > end`.
    crate::goals::arm(w, e, inst.slot, (dur + 1) as i32);
}
/// The end of the panic is its deadline. A deadline that is not the panic's own (a replayed start) asks `can_use`.
fn fired(w: &mut World, e: Entity, inst: Inst) -> bool {
    if w.get::<AnimalPanicking>(e).is_none() {
        return true;
    }
    let now = now_of(w, e);
    if w.get::<Fleeing>(e).is_some_and(|f| now > f.end) {
        w.resource_mut::<Requests>().0.push(Req::End(e, inst.slot));
    }
    false
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<AnimalPanicking>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    /// A cow with this module only (the species of the test), awake, on the flat ground of the test world.
    fn world(riders: Riders) -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(60, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 1, if riders == Riders::Any { &ANY } else { &PARAMS })]));
        sp.goals = Config { entries };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 11);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        crate::goals::arm_timers(&mut h.world, e);
        h.tick();
        Some((h, e))
    }
    static PARAMS: Params = Params { speed: 4.0, stand_up: false, ..Params::DEFAULT };
    static ANY: Params = Params { riders: Riders::Any, ..PARAMS };
    /// Puts a damage of a panic cause on the animal, as `apply_damage` does: its `LastDamage` and the `hurt` event.
    fn hurt(h: &mut Herd, e: Entity) {
        let now = h.world.resource::<Clock>().of(e);
        h.world.entity_mut(e).insert(LastDamage { at: now, panic: true, by: None });
        h.world.resource_mut::<Events>().hurt.push(e);
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<AnimalPanicking>(e).is_some()
    }
    /// A hit starts the panic at once; the run covers ground; its end comes 100 to 121 ticks later.
    /// Witnesses: no panic before the hit, the animal moves while it runs.
    #[test]
    fn a_hit_starts_a_panic_that_runs_then_stops_after_its_duration() {
        let Some((mut h, e)) = world(Riders::Player) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        h.tick();
        assert!(!running(&h, e), "témoin : sans dégât, pas de panique");
        hurt(&mut h, e);
        h.tick();
        h.tick();
        assert!(running(&h, e), "départ au tick qui suit le dégât");
        let start = h.world.resource::<Clock>().of(e);
        let at0 = h.world.get::<Pos>(e).unwrap().0;
        let mut moved = 0.0;
        let mut last = at0;
        let mut stopped_at = None;
        for _ in 0..200 {
            h.tick();
            let p = h.world.get::<Pos>(e).unwrap().0;
            moved += ((p.x - last.x).powi(2) + (p.z - last.z).powi(2)).sqrt();
            last = p;
            if !running(&h, e) && stopped_at.is_none() {
                stopped_at = Some(h.world.resource::<Clock>().of(e));
            }
        }
        assert!(moved > 3.0, "il court : {moved} blocs");
        let stop = stopped_at.expect("la panique finit");
        let dur = stop - start;
        assert!((100..=122).contains(&dur), "durée {dur}");
    }
    /// A rider that is a player holds the camel back: no panic from a hit. Control: without the rider it starts.
    #[test]
    fn a_player_rider_holds_the_panic_back() {
        let Some((mut h, e)) = world(Riders::Player) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        h.tick();
        hurt(&mut h, e);
        h.tick();
        h.tick();
        assert!(running(&h, e), "témoin : sans cavalier, il panique");
        let Some((mut m, mount_e)) = world(Riders::Player) else { return };
        m.world.entity_mut(mount_e).insert(Mount { rider: Some(Who::Player(1)), ..Mount::default() });
        m.tick();
        hurt(&mut m, mount_e);
        m.tick();
        m.tick();
        assert!(!running(&m, mount_e), "un joueur cavalier retient la panique");
    }
    /// Stop: the goal ends when its duration is over; a later hit (in the window) starts it again.
    #[test]
    fn it_stops_at_its_end_and_a_later_hit_starts_it_again() {
        let Some((mut h, e)) = world(Riders::Player) else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        h.tick();
        hurt(&mut h, e);
        h.tick();
        h.tick();
        for _ in 0..200 {
            h.tick();
        }
        assert!(!running(&h, e), "arrêt après la durée");
        hurt(&mut h, e);
        h.tick();
        h.tick();
        assert!(running(&h, e), "un nouveau dégât relance la panique");
    }
}
