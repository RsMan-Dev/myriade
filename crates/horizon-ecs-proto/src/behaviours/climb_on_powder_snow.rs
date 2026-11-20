//! `ClimbOnTopOfPowderSnowGoal` (fiche 11, `mobs/goals.rs:1021-1057`): a walker whose box touches powder snow jumps while
//! the block above its feet is powder snow or has no collision. The wake-up is the `Hazard` bit of the box: there is no
//! `entered_snow` event in the frame yet (DOUTE in the notes), so the wake system visits the holders that have a hazard.
use super::*;
use crate::{cow::*, goals::*};
use horizon_client_lib::mobs::MobWorld;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The species is in `minecraft:powder_snow_walkable_mobs`.
    pub walkable: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { walkable: true };
}
/// The species of the module can climb: only these animals are visited.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct SnowWalker;
/// The goal runs: its jump is asked every tick.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Climbing;

pub static MODULE: Module = Module {
    name: "climb_on_powder_snow",
    flags: JUMP,
    equip,
    can_use,
    start,
    stop,
    systems,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &(dyn std::any::Any + Sync)) {
    if params::<Params>(p).walkable {
        w.entity_mut(e).insert(SnowWalker);
    }
}
fn block_of(p: &Pos) -> [i32; 3] {
    [p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32]
}
/// `Mob.blockPosition().above()` is powder snow, or has no collision.
fn above_passable(mw: &MobWorld<'_>, feet: [i32; 3]) -> bool {
    let above = [feet[0], feet[1] + 1, feet[2]];
    mw.block_name(above[0], above[1], above[2]) == "minecraft:powder_snow" || mw.collision_bounds(above).is_none()
}
fn in_snow(h: Option<&Hazard>) -> bool {
    h.is_some_and(|h| h.0 & HAZ_SNOW != 0)
}
fn can_use(w: &mut World, e: Entity, _: Inst) -> bool {
    let mw = w.resource::<Level>().mw.0;
    let feet = block_of(w.get::<Pos>(e).unwrap());
    w.get::<SnowWalker>(e).is_some() && in_snow(w.get::<Hazard>(e)) && above_passable(mw, feet)
}
fn start(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).insert(Climbing);
    // A started goal is ticked in the tick it starts (GUIDE 9.1).
    w.get_mut::<Jump>(e).unwrap().0 = true;
}
fn stop(w: &mut World, e: Entity, _: Inst) {
    w.entity_mut(e).remove::<Climbing>();
    if let Some(mut j) = w.get_mut::<Jump>(e) {
        j.0 = false;
    }
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((climb_wake, climb_run).chain().in_set(slot));
}
/// A holder whose box touches powder snow and that passes the test asks to start (the selector hears it on its tick).
fn climb_wake(q: Query<(Entity, &Hazard, &GoalSet, &Pos), (With<SnowWalker>, Without<Climbing>)>, level: Res<Level>, mut reqs: ResMut<Requests>) {
    let mw = level.mw.0;
    for (e, h, g, p) in &q {
        if h.0 & HAZ_SNOW == 0 || !above_passable(mw, block_of(p)) {
            continue;
        }
        if let Some(slot) = g.slot_of(&MODULE) {
            reqs.0.push(Req::Start(e, slot));
        }
    }
}
/// `ClimbOnTopOfPowderSnowGoal.tick` (every tick of the run): the jump is asked; the run ends when the test fails.
fn climb_run(mut q: Query<(Entity, &Pos, Option<&Hazard>, &GoalSet, &mut Jump), With<Climbing>>, level: Res<Level>, mut reqs: ResMut<Requests>) {
    let mw = level.mw.0;
    for (e, p, h, g, mut jump) in &mut q {
        if in_snow(h) && above_passable(mw, block_of(p)) {
            jump.0 = true;
        } else {
            jump.0 = false;
            if let Some(slot) = g.slot_of(&MODULE) {
                reqs.0.push(Req::End(e, slot));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::species::Config;
    use crate::{goals::set_lazy_look, cow::{Herd, SpeciesTable, spawn_species, ActiveUntil}};
    use horizon_core::geometry::Vec3;

    fn guard(test: &str) -> bool {
        if std::env::var_os("HORIZON_TEST_RESOURCES").is_none() {
            crate::cow::skip_note(test, "HORIZON_TEST_RESOURCES is not set");
            return true;
        }
        false
    }
    /// A cow on a slab, with the given selector entries, settled on the floor.
    fn herd_with(entries: Vec<Entry>) -> (Herd, Entity) {
        let mut h = Herd::with_slab(20, true, false);
        set_lazy_look(&mut h.world, false);
        let mut sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.goals = Config { entries: Box::leak(entries.into_boxed_slice()) };
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 7);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        for _ in 0..40 {
            h.tick();
        }
        (h, e)
    }
    fn feet(h: &Herd, e: Entity) -> [i32; 3] {
        block_of(h.world.get::<Pos>(e).unwrap())
    }
    /// Puts a block at `at` and moves the animal back onto its place (its hazards are read when its box moves).
    fn put(h: &mut Herd, e: Entity, at: [i32; 3], name: &'static str) {
        h.set_block(at[0], at[1], at[2], name);
        h.tick();
        let p = h.world.get::<Pos>(e).unwrap().0;
        h.teleport(e, p);
    }
    fn running(h: &Herd, e: Entity) -> bool {
        h.world.get::<GoalSet>(e).unwrap().is_running(&MODULE)
    }
    fn jump(h: &Herd, e: Entity) -> bool {
        h.world.get::<Jump>(e).unwrap().0
    }
    /// Whether the goal runs on one of the next `n` ticks (a jump out of the snow ends it, as in the `Mob`).
    fn runs_within(h: &mut Herd, e: Entity, n: usize) -> bool {
        (0..n).any(|_| {
            h.tick();
            running(h, e)
        })
    }
    fn climb_entries(p: &'static Params) -> Vec<Entry> {
        vec![Entry::new(&MODULE, 1, p)]
    }

    #[test]
    fn a_column_of_snow_keeps_the_climb_running_and_jumping() {
        if guard("a_column_of_snow_keeps_the_climb_running_and_jumping") {
            return;
        }
        let (mut h, e) = herd_with(climb_entries(&Params::DEFAULT));
        assert!(!running(&h, e) && !jump(&h, e), "no snow, no climb");
        let f = feet(&h, e);
        put(&mut h, e, f, "minecraft:powder_snow");
        put(&mut h, e, [f[0], f[1] + 1, f[2]], "minecraft:powder_snow");
        for _ in 0..4 {
            h.tick();
        }
        assert!(running(&h, e), "snow at the feet and above: the goal runs");
        for _ in 0..12 {
            h.tick();
            assert!(running(&h, e) && jump(&h, e), "the run asks a jump every tick");
        }
    }

    #[test]
    fn snow_with_air_above_starts_the_climb() {
        if guard("snow_with_air_above_starts_the_climb") {
            return;
        }
        let (mut h, e) = herd_with(climb_entries(&Params::DEFAULT));
        let f = feet(&h, e);
        put(&mut h, e, f, "minecraft:powder_snow");
        assert!(runs_within(&mut h, e, 4), "snow at the feet and air above: the goal starts");
    }

    #[test]
    fn a_solid_block_above_blocks_the_start_until_it_is_removed() {
        if guard("a_solid_block_above_blocks_the_start_until_it_is_removed") {
            return;
        }
        let (mut h, e) = herd_with(climb_entries(&Params::DEFAULT));
        let f = feet(&h, e);
        put(&mut h, e, f, "minecraft:powder_snow");
        put(&mut h, e, [f[0], f[1] + 1, f[2]], "minecraft:stone");
        for _ in 0..6 {
            h.tick();
            assert!(!running(&h, e) && !jump(&h, e), "a full block above: no climb");
        }
        put(&mut h, e, [f[0], f[1] + 1, f[2]], "minecraft:air");
        assert!(runs_within(&mut h, e, 6), "the block above is gone: the goal starts (witness)");
    }

    #[test]
    fn leaving_the_snow_ends_the_run_and_clears_the_jump() {
        if guard("leaving_the_snow_ends_the_run_and_clears_the_jump") {
            return;
        }
        let (mut h, e) = herd_with(climb_entries(&Params::DEFAULT));
        let f = feet(&h, e);
        put(&mut h, e, f, "minecraft:powder_snow");
        put(&mut h, e, [f[0], f[1] + 1, f[2]], "minecraft:powder_snow");
        for _ in 0..4 {
            h.tick();
        }
        assert!(running(&h, e) && jump(&h, e), "witness: the run is on before the cow leaves");
        let p = h.world.get::<Pos>(e).unwrap().0;
        h.teleport(e, Vec3::new(p.x + 5.0, p.y, p.z));
        for _ in 0..6 {
            h.tick();
        }
        assert!(!running(&h, e) && !jump(&h, e), "out of the snow: the goal ends and the jump is off");
    }

    #[test]
    fn a_species_outside_the_walkable_tag_never_climbs() {
        if guard("a_species_outside_the_walkable_tag_never_climbs") {
            return;
        }
        static NOT_WALKABLE: Params = Params { walkable: false };
        let (mut h, e) = herd_with(climb_entries(&NOT_WALKABLE));
        let f = feet(&h, e);
        put(&mut h, e, f, "minecraft:powder_snow");
        assert!(!runs_within(&mut h, e, 6), "not in the tag: no climb");
        let (mut w, we) = herd_with(climb_entries(&Params::DEFAULT));
        let f = feet(&w, we);
        put(&mut w, we, f, "minecraft:powder_snow");
        assert!(runs_within(&mut w, we, 6), "witness: the same scene with the tag starts");
    }
}
