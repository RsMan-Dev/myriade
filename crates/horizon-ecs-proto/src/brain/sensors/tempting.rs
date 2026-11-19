//! `Tempting` (`AnimalBrain.sense_tempting`, `TemptingSensor.forAnimal`): every scan, the nearest player who is alive, not a
//! spectator, within the tempt range, and holding one of the species' foods in a hand is the `tempting_player`; erased otherwise.
//! No draw and no expiry. The range and the foods are the species' (`Params`): a sensor receives no parameters from the framework
//! yet (`Sensor.scan` takes the entity only), so they are written on the entity by `equip` (BESOIN DU CADRE in the notes).
use crate::behaviours::Cap;
use crate::brain::*;
use crate::cow::*;
use crate::goals::{d2, item_id, Online};
use crate::hostile::PlayerState;
use crate::targeting::Who;
use horizon_core::geometry::Vec3;

pub static SENSOR: Sensor = Sensor { name: "minecraft:tempting", mems: &[&TEMPTING_PLAYER], scan, equip: equip_sensor, requires: &[Cap::Targets], ..Sensor::DEFAULT };
/// The frame calls it once at creation with the entry's parameters.
fn equip_sensor(w: &mut World, e: Entity, c: &Ctx) {
    equip(w, e, c.get::<Params>());
}

/// The species' values: the tempt range in blocks (10 for animals, 16 for the happy ghast) and the foods, by name.
pub struct Params {
    pub range: f64,
    pub foods: &'static [&'static str],
}
impl Params {
    pub const DEFAULT: Params = Params { range: 10.0, foods: &[] };
}

/// The entity's tempting data, written by `equip`: the range and the foods as a bit per item id (as `tempt` does).
#[derive(Component, Clone, Copy, Debug)]
pub struct Tempting {
    pub range: f64,
    pub items: crate::goals::ItemSet,
}

/// Writes the species' tempting data on the entity.
pub fn equip(w: &mut World, e: Entity, p: &Params) {
    let items = crate::goals::ItemSet::of(p.foods.iter().map(|n| item_id(n)));
    w.entity_mut(e).insert(Tempting { range: p.range, items });
}

fn scan(w: &mut World, e: Entity, _: &Ctx) {
    let Some(t) = w.get::<Tempting>(e).copied() else {
        erase(w, e, &TEMPTING_PLAYER);
        return;
    };
    let Some(pos) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let r2 = t.range * t.range;
    let mut best: Option<(u32, f64)> = None;
    let ps = w.resource::<PlayerState>();
    w.resource::<Online>().for_near(pos.x, pos.z, t.range, |pl| {
        let d = d2(pos, Vec3::new(pl.x, pl.y, pl.z));
        if ps.alive(pl.id) && !ps.spectator(pl.id) && d <= r2 && pl.holds_any(t.items) && best.is_none_or(|b| d < b.1) {
            best = Some((pl.id, d));
        }
    });
    match best {
        Some((id, _)) => set(w, e, &TEMPTING_PLAYER, MemValue::Who(Who::Player(id)), None),
        None => erase(w, e, &TEMPTING_PLAYER),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{spawn_species, Herd, SpeciesTable};
    use crate::goals::{Player, CARROT};

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    const FOOD: &[&str] = &["minecraft:carrot"];

    fn brained(h: &mut Herd) -> Entity {
        static ENTRIES: [SensorEntry; 1] = [SensorEntry::new(&SENSOR, 20).with(&Params { range: 10.0, foods: FOOD })];
        let def = BrainDef {
            sensors: &ENTRIES,
            activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }],
            pick: &[],
            initial: &[],
            behaviours: &[],
        };
        let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
        let sp = h.world.resource::<SpeciesTable>().0[0];
        let e = spawn_species(&mut h.world, &sp, 0.5, 0.5, 91);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        h.world.entity_mut(e).insert(bundle(plan));
        // `start` is the frame's creation step: it calls the sensor's `equip` with the entry's parameters.
        start(&mut h.world, e);
        e
    }
    fn player(h: &mut Herd, id: u32, x: f64, main: u32) {
        h.world.resource_mut::<Online>().set(Player { id, x, y: 1.0, z: 0.5, main, off: 0 });
    }
    fn tempting(h: &Herd, e: Entity) -> Option<Who> {
        match get(&h.world, e, &TEMPTING_PLAYER) {
            Some(MemValue::Who(w)) => Some(*w),
            _ => None,
        }
    }

    /// A player holding the food at 8 blocks tempts; the same player at 12 blocks, or with nothing in hand, does not.
    #[test]
    fn a_player_with_food_within_range_tempts() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h);
        player(&mut h, 1, 8.5, CARROT);
        scan(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
        assert_eq!(tempting(&h, e), Some(Who::Player(1)));
        player(&mut h, 1, 12.5, CARROT);
        scan(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
        assert_eq!(tempting(&h, e), None);
        player(&mut h, 1, 8.5, 0);
        scan(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
        assert_eq!(tempting(&h, e), None);
    }

    /// An entity without the species' data (no `equip`) has no tempting player, whatever is held.
    #[test]
    fn without_the_species_data_nobody_tempts() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h);
        player(&mut h, 1, 4.5, CARROT);
        scan(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
        assert_eq!(tempting(&h, e), Some(Who::Player(1)));
        h.world.entity_mut(e).remove::<Tempting>();
        scan(&mut h.world, e, &Ctx { id: 0, nth: 0, params: &() });
        assert_eq!(tempting(&h, e), None);
    }
}
