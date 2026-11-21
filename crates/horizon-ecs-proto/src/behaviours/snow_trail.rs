//! `SnowGolem.aiStep` (fiche 52): a snow golem leaves snow behind it, and burns in hot places. Not a goal: no flag, no selector, a
//! rule with a tick. Each tick of its trail: in a hot place (`snow_golem_melts`, or a temperature above 1) it takes 1 fire damage;
//! with mob griefing, each corner of its box (+-0.25 on x and z) at its feet row takes a snow layer where the cell is air and the
//! cell below supports snow. The trail runs while the golem is awake (`Moving`) or hot; it is woken by a change of column (the
//! spawn included, through `CellMoves`).
//! DOUTE: a golem that moves inside its column is not woken (no event of movement): its corners are traced only at a change of
//! column (the Mob traces at every tick of movement).
use super::*;
use crate::{cow::*, goals::*};
use crate::rules::snow_trail as rules;
use horizon_client_lib::mobs::MobWorld;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub melt_temperature: f64,
    pub corner: f64,
}
impl Params {
    pub const DEFAULT: Self = Self { melt_temperature: 1.0, corner: 0.25 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Trail {
    pub def: &'static Params,
    pub slot: u8,
}
/// The trail runs for this golem (SparseSet: only the running ones are visited).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Trailing;

pub static MODULE: Module = Module {
    name: "snow_trail",
    flags: 0,
    equip,
    systems,
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, inst: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert(Trail { def: params::<Params>(p), slot: inst.slot });
}
fn systems(s: &mut Schedule, slot: GoalSlot) {
    s.add_systems((wake, run).chain().in_set(slot));
}
/// A golem that changed its column (the spawn included) runs its trail.
fn wake(moved: Res<CellMoves>, trails: Query<(), With<Trail>>, mut cmd: Commands) {
    for e in moved.0.iter() {
        if trails.contains(*e) {
            cmd.entity(*e).insert(Trailing);
        }
    }
}
fn run(
    mut q: Query<(Entity, &Trail, &Pos, Option<&Moving>), With<Trailing>>,
    level: Res<Level>,
    mut damage: ResMut<DamageQueue>,
    mut changes: ResMut<BlockChanges>,
    mut cmd: Commands,
) {
    let mw = level.mw.0;
    for (e, tr, p, moving) in &mut q {
        let (x, y, z) = (p.0.x.floor() as i32, p.0.y.floor() as i32, p.0.z.floor() as i32);
        let hot = rules::melts(mw.dimension_type.snow_golem_melts, f64::from(mw.temperature(x, y, z)), tr.def.melt_temperature);
        if hot {
            damage.add(e, 1.0, Cause::OnFire);
        }
        if mw.mob_griefing {
            trail(mw, p.0.x, p.0.z, y, tr.def.corner, &mut changes);
        }
        if !hot && moving.is_none() {
            cmd.entity(e).remove::<Trailing>();
        }
    }
}
/// The four corners, in the Mob's order: a snow layer on each air cell over a support, once per cell.
fn trail(mw: &MobWorld, x: f64, z: f64, y: i32, corner: f64, changes: &mut BlockChanges) {
    let mut placed = [[i32::MIN; 3]; 4];
    let mut n = 0;
    for cell in rules::corner_cells(x, z, y, corner) {
        let [xx, _, zz] = cell;
        if mw.facts.is_air(mw.world.block(xx, y, zz)) && rules::supports(mw, [xx, y - 1, zz]) && !placed[..n].contains(&cell) {
            placed[n] = cell;
            n += 1;
            changes.0.push((xx, y, zz, "minecraft:snow"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 255, &Params::DEFAULT)]));
        Config { entries }
    }
    /// Snow cells on the feet row (y 1, the ground is row 0) between x 0 and 8.
    fn snow_cells(h: &Herd) -> usize {
        let mw = h.world.resource::<Level>().mw.0;
        (0..=8).filter(|&x| mw.block_name(x, 1, 0) == "minecraft:snow").count()
    }
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab_awake(200, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:cow").unwrap();
        sp.goals = config();
        let c = spawn_species(&mut h.world, &sp, 0.5, 0.5, 77);
        h.world.get_mut::<ActiveUntil>(c).unwrap().0 = u64::MAX;
        h.world.resource_mut::<crate::goals::Entered>().0.push(c);
        h.tick();
        Some((h, c))
    }
    fn ticks(h: &mut Herd, n: usize) {
        for _ in 0..n {
            h.tick();
        }
    }

    /// Walking across the slab leaves snow on the ground row behind the golem (the witness: the count rises).
    #[test]
    fn a_walking_golem_leaves_snow_behind_it() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        let before = snow_cells(&h);
        crate::goals::go_to_at(&mut h.world, c, [8.5, 0.0, 0.5], 1.0, 1);
        ticks(&mut h, 300);
        assert!(snow_cells(&h) > before, "témoin : la neige s'est posée sous les pas");
    }

    /// Once the golem stops, its trail stops: the snow count no longer changes.
    #[test]
    fn the_trail_stops_when_the_golem_stops() {
        let Some((mut h, c)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        crate::goals::go_to_at(&mut h.world, c, [8.5, 0.0, 0.5], 1.0, 1);
        ticks(&mut h, 300);
        let at_rest = snow_cells(&h);
        assert!(at_rest > 0, "témoin : de la neige a été posée");
        ticks(&mut h, 200);
        assert_eq!(snow_cells(&h), at_rest, "à l'arrêt, plus de neige posée");
        assert!(h.world.get::<Trailing>(c).is_none(), "la traînée est arrêtée");
    }
}
