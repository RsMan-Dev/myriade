//! Fiche 100, posture assis / debout du chameau (`mobs/camel.rs:296-318`, `:172-183`, `living.rs:1535-1544`) : une RÈGLE,
//! pas un but (priorité 255, sans drapeau, jamais demandée). Assis ou en transition, le chameau ne marche pas. La pose change
//! en temps propre (`now_of`). L'occupation `OCC_SITTING` couvre l'assise ET le lever : c'est `refuse_to_move`, donc
//! `horizon:is_sitting` lu par les buts du chameau vaut `refuse_to_move`. Écarts : pas de crochet « un point à marcher est
//! posé » (le lever de `camel.rs:172-183`), pas de boîte par pose (BESOIN DU CADRE).
use super::*;
use crate::cow::*;
use crate::goals::*;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// Ticks to sit down (`SIT_DOWN`), ticks to stand up (`STAND_UP`).
    pub sit_down: u64,
    pub stand_up: u64,
}
impl Params {
    pub const DEFAULT: Self = Self { sit_down: 40, stand_up: 52 };
}

/// The pose, and the own time of its last change (`since`, negative at creation: a camel is born standing, its pose done).
#[derive(Component, Clone, Copy, Debug)]
pub struct Posture {
    pub sitting: bool,
    pub since: i64,
    pub sit_down: u64,
    pub stand_up: u64,
}
/// A transition is running: the tick holds the walk and ends the transition at its own time.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Holding;

pub static MODULE: Module = Module {
    name: "posture",
    flags: 0,
    equip,
    can_use: never,
    tick: Some(tick),
    requires: &[Cap::Goals, Cap::Locomotion],
    ..Module::DEFAULT
};
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    let now = now_of(w, e) as i64;
    w.entity_mut(e).insert(Posture { sitting: false, since: now - p.stand_up as i64 - 1, sit_down: p.sit_down, stand_up: p.stand_up });
}

impl Posture {
    /// `refuse_to_move` at own time `now`: sitting, or in a transition.
    pub fn refuses(&self, now: i64) -> bool {
        self.sitting || now - self.since < transition(self)
    }
    /// Sitting with the sit-down over (the camel may stand up).
    pub fn sat(&self, now: i64) -> bool {
        self.sitting && now - self.since >= transition(self)
    }
}
fn transition(p: &Posture) -> i64 {
    if p.sitting { p.sit_down as i64 } else { p.stand_up as i64 }
}
/// `pose_time` (`camel.rs:72-74`): own ticks since the last change of pose.
pub fn pose_time(w: &World, e: Entity) -> i64 {
    now_of(w, e) as i64 - w.get::<Posture>(e).map_or(0, |p| p.since)
}
/// `sit_down` (`camel.rs:300-306`): a standing camel sits; the walk is held for the sit-down, the occupation is taken.
pub fn sit_down(w: &mut World, e: Entity) {
    let now = now_of(w, e) as i64;
    let Some(mut p) = w.get_mut::<Posture>(e) else { return };
    if p.sitting {
        return;
    }
    p.sitting = true;
    p.since = now;
    drop(p);
    occupy(w, e, OCC_SITTING);
    w.entity_mut(e).insert(Holding);
}
/// `stand_up` (`camel.rs:307-313`): a sitting camel stands; the walk and the occupation stay held for the stand-up.
pub fn stand_up(w: &mut World, e: Entity) {
    let now = now_of(w, e) as i64;
    let Some(mut p) = w.get_mut::<Posture>(e) else { return };
    if !p.sitting {
        return;
    }
    p.sitting = false;
    p.since = now;
    drop(p);
    w.entity_mut(e).insert(Holding);
}
/// `stand_up_instantly` (`camel.rs:315-318`): standing at once, no transition left.
pub fn stand_up_instantly(w: &mut World, e: Entity) {
    let now = now_of(w, e) as i64;
    let Some(mut p) = w.get_mut::<Posture>(e) else { return };
    p.sitting = false;
    p.since = now - p.stand_up as i64 - 1;
    drop(p);
    release(w, e, OCC_SITTING);
    w.entity_mut(e).remove::<Holding>();
}
pub fn is_sitting(w: &World, e: Entity) -> bool {
    w.get::<Posture>(e).is_some_and(|p| p.sitting)
}
/// `refuseToMove` (`camel.rs:296-298`): sitting, or in a transition.
pub fn refuses_to_move(w: &World, e: Entity) -> bool {
    w.get::<Posture>(e).is_some_and(|p| p.sitting || pose_time(w, e) < transition(p))
}

fn tick(w: &mut World) {
    tick_state(w, water_run);
    tick_state(w, holding_run);
}
/// `ai_step` (`camel.rs:433-435`): a sitting camel that enters water stands up at once (the event is read once).
fn water_run(ev: Res<Events>, clock: Res<Clock>, mut cmds: Commands, mut q: Query<(&mut Posture, Option<&mut Occupied>)>) {
    for e in ev.entered_fluid.iter() {
        let Ok((mut p, occ)) = q.get_mut(e) else { continue };
        if !p.sitting {
            continue;
        }
        p.sitting = false;
        p.since = clock.of(e) as i64 - p.stand_up as i64 - 1;
        if let Some(mut o) = occ {
            o.0 &= !OCC_SITTING;
        }
        cmds.entity(e).remove::<Holding>();
    }
}
/// The transitions in progress hold the walk (`Drive`); when the transition is over the mark goes, and a stand-up releases
/// the occupation. DOUTE: the control writes `Drive` after this tick, so the hold may be overwritten in the same tick.
fn holding_run(clock: Res<Clock>, mut cmds: Commands, mut q: Query<(Entity, &mut Posture, Option<&mut Drive>, Option<&mut Occupied>), With<Holding>>) {
    for (e, p, drive, occ) in &mut q {
        if let Some(mut d) = drive {
            d.zza = 0.0;
            d.xxa = 0.0;
        }
        if (clock.of(e) as i64) - p.since < transition(&p) {
            continue;
        }
        if !p.sitting
            && let Some(mut o) = occ
        {
            o.0 &= !OCC_SITTING;
        }
        cmds.entity(e).remove::<Holding>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    /// A camel with this module only: the species of the test is one entry of its configuration.
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(40, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 255, &Params::DEFAULT)]));
        sp.goals = Config { entries };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.tick();
        Some((h, e))
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }
    /// Sitting refuses to move for the whole sit-down (40 ticks) and stays refused; standing up refuses for 52 ticks.
    /// Witness: a standing camel does not refuse to move, and the first refusal is the sit-down.
    #[test]
    fn a_sit_down_holds_the_walk_for_forty_ticks_and_a_stand_up_for_fifty_two() {
        let Some((mut h, e)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        assert!(!refuses_to_move(&h.world, e), "témoin : debout, il marche");
        sit_down(&mut h.world, e);
        assert!(refuses_to_move(&h.world, e), "assis en transition : refus");
        ticks(&mut h, 39);
        assert!(refuses_to_move(&h.world, e), "encore en transition à 39 ticks");
        ticks(&mut h, 3);
        assert!(refuses_to_move(&h.world, e), "assis : refus");
        assert!(is_sitting(&h.world, e) && h.world.get::<Occupied>(e).is_some_and(|o| o.0 & OCC_SITTING != 0));
        stand_up(&mut h.world, e);
        ticks(&mut h, 51);
        assert!(refuses_to_move(&h.world, e), "lever en transition à 51 ticks");
        ticks(&mut h, 3);
        assert!(!refuses_to_move(&h.world, e), "debout après 52 ticks");
        assert!(h.world.get::<Occupied>(e).is_some_and(|o| o.0 & OCC_SITTING == 0), "l'occupation est retirée à la fin du lever");
        assert!(h.world.get::<Holding>(e).is_none(), "la marque de transition part avec elle");
    }
    /// Sitting in water: the camel stands at once, the tick after the event (the event is read one tick later).
    /// Witness: before the event it sits.
    #[test]
    fn a_sitting_camel_that_enters_water_stands_up_at_once() {
        let Some((mut h, e)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        sit_down(&mut h.world, e);
        ticks(&mut h, 50);
        assert!(is_sitting(&h.world, e), "témoin : assis avant l'eau");
        h.world.resource_mut::<Events>().entered_fluid.push(e);
        h.tick();
        assert!(!is_sitting(&h.world, e), "debout au tick qui suit l'entrée dans l'eau");
        assert!(!refuses_to_move(&h.world, e), "sans transition");
    }
    /// A camel born standing is past its stand-up: it walks at once.
    #[test]
    fn a_new_camel_stands_and_walks_from_its_first_tick() {
        let Some((h, e)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        assert!(pose_time(&h.world, e) >= 52, "pose_time {}", pose_time(&h.world, e));
        assert!(!refuses_to_move(&h.world, e));
    }
}
