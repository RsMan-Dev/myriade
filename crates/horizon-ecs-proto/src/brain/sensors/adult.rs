//! `AdultSensor` (`Sensor::AdultSameType`, `animal_brain.rs:661-668`) and `AdultSensorAnyType` (`:673`, `sense_adult` `:814-826`):
//! every scan, the nearest visible creature that is an adult (not a baby, not a player) is the `nearest_visible_adult`. `SAME`
//! takes the entity's own species; `ANY` takes the species named by the tag of its parameters (`followable_friendly_mobs`).
//! The list is `visible_mobs`, nearest first (`living.rs`). Nothing found: the memory is erased.
use crate::behaviours::Cap;
use crate::brain::sensors::living;
use crate::brain::*;
use crate::cow::*;
use crate::targeting::{info, Who};

pub static SAME: Sensor = Sensor { name: "minecraft:adult_same_type", mems: &[&NEAREST_VISIBLE_ADULT], scan: scan_same, requires: &[Cap::Targets], ..Sensor::DEFAULT };
pub static ANY: Sensor = Sensor { name: "minecraft:adult_any_type", mems: &[&NEAREST_VISIBLE_ADULT], scan: scan_any, requires: &[Cap::Targets], ..Sensor::DEFAULT };

/// The species' value of `ANY`: the entity type tag whose species are followed (`#minecraft:followable_friendly_mobs`,
/// `data/minecraft/tags/entity_type/followable_friendly_mobs.json`).
pub struct Params {
    pub tag: &'static str,
}
impl Params {
    pub const DEFAULT: Params = Params { tag: "minecraft:followable_friendly_mobs" };
}

fn scan_same(w: &mut World, e: Entity, _: &Ctx) {
    let own = w.get::<SpeciesId>(e).map(|s| s.0);
    let found = find_closest(w, e, |w, who| adult_species(w, who).is_some_and(|sp| Some(sp) == own));
    store(w, e, found);
}

fn scan_any(w: &mut World, e: Entity, c: &Ctx) {
    // `AdultSensorAnyType` returns before it writes when the list is absent: the memory keeps its value.
    if !present(w, e, &VISIBLE_MOBS) {
        return;
    }
    let tag = c.get::<Params>().tag;
    let found = find_closest(w, e, |w, who| adult_species(w, who).is_some_and(|sp| species_has(w, tag, sp)));
    store(w, e, found);
}

/// The species of a creature that is an adult; none for a player or a baby.
fn adult_species(w: &World, who: Who) -> Option<u8> {
    if matches!(who, Who::Player(_)) {
        return None;
    }
    let i = info(w, who)?;
    (!i.baby).then_some(i.species)
}

/// `#tag` contains the name of the species `sp` (the data of the process, `MobData::entity_has`).
fn species_has(w: &World, tag: &str, sp: u8) -> bool {
    let Some(name) = w.resource::<SpeciesTable>().0.iter().find(|s| s.id == sp).map(|s| s.name) else { return false };
    w.resource::<Level>().mw.0.data.entity_has(tag, name)
}

fn store(w: &mut World, e: Entity, found: Option<Who>) {
    match found {
        Some(who) => set(w, e, &NEAREST_VISIBLE_ADULT, MemValue::Who(who), None),
        None => erase(w, e, &NEAREST_VISIBLE_ADULT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{spawn_species, species_table, ActiveUntil, Herd, SpeciesTable};

    const CAPS: &[Cap] = &[Cap::Transform, Cap::Solid, Cap::Body, Cap::Living, Cap::Locomotion, Cap::Targets, Cap::Goals, Cap::Brain];
    static TAG: Params = Params { tag: "minecraft:followable_friendly_mobs" };
    static NO_TAG: Params = Params { tag: "minecraft:no_such_tag" };
    static ENTRIES: [SensorEntry; 3] = [SensorEntry::new(&SAME, 20), SensorEntry::new(&ANY, 20).with(&TAG), SensorEntry::new(&living::SENSOR, 20)];

    /// A cow with a brain whose sensors hold the memory; the test calls the scans itself.
    fn brained(h: &mut Herd) -> Entity {
        let def = BrainDef {
            sensors: &ENTRIES,
            activities: &[ActivityDef { name: "core", slot: 1, requires: &[] }],
            pick: &[],
            initial: &[],
            behaviours: &[],
        };
        let plan: &'static Plan = Box::leak(Box::new(Plan::build(&def, CAPS).unwrap()));
        let e = creature(h, "minecraft:cow", 0.5, 0.5, false);
        h.world.entity_mut(e).insert(bundle(plan));
        start(&mut h.world, e);
        e
    }
    fn creature(h: &mut Herd, name: &str, x: f64, z: f64, baby: bool) -> Entity {
        let sp = *h.world.resource::<SpeciesTable>().0.iter().find(|s| s.name == name).expect("a known species");
        let seed = 0x51 + u64::from(sp.id) * 977 + (x * 13.0) as u64;
        let e = spawn_species(&mut h.world, &sp, x, z, seed);
        h.world.get_mut::<ActiveUntil>(e).unwrap().0 = u64::MAX;
        if baby {
            h.world.entity_mut(e).insert(GrowAt(u64::MAX));
        }
        e
    }
    fn visible(h: &mut Herd, e: Entity, list: Vec<Who>) {
        set(&mut h.world, e, &VISIBLE_MOBS, MemValue::Whos(list), None);
    }
    fn nearest(h: &Herd, e: Entity) -> Option<Who> {
        match get(&h.world, e, &NEAREST_VISIBLE_ADULT) {
            Some(MemValue::Who(w)) => Some(*w),
            _ => None,
        }
    }
    fn ctx(params: &'static (dyn std::any::Any + Sync)) -> Ctx {
        Ctx { id: 0, nth: 0, params }
    }

    /// The nearest visible same-species adult: the pig (other species) and the baby cow nearer are skipped, the adult cow is
    /// chosen; without it in the list, nothing is remembered.
    #[test]
    fn the_nearest_adult_of_the_same_species_is_remembered() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h);
        let pig = creature(&mut h, "minecraft:pig", 1.5, 0.5, false);
        let baby = creature(&mut h, "minecraft:cow", 2.5, 0.5, true);
        let adult = creature(&mut h, "minecraft:cow", 6.5, 0.5, false);
        let far = creature(&mut h, "minecraft:cow", 9.5, 0.5, false);
        visible(&mut h, e, vec![Who::Entity(pig), Who::Entity(baby), Who::Entity(adult), Who::Entity(far)]);
        scan_same(&mut h.world, e, &ctx(&()));
        assert_eq!(nearest(&h, e), Some(Who::Entity(adult)), "témoin : le bon adulte de même espèce");
        visible(&mut h, e, vec![Who::Entity(pig), Who::Entity(baby)]);
        scan_same(&mut h.world, e, &ctx(&()));
        assert_eq!(nearest(&h, e), None, "sans adulte de même espèce, la mémoire est effacée");
    }

    /// `ANY` follows the tag: a sheep or a pig (in the tag) is chosen, a zombie (not in it) is not, a baby is not; the
    /// parameter's tag is the one read.
    #[test]
    fn the_nearest_adult_of_the_tagged_species_is_remembered() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h);
        let zombie = creature(&mut h, "minecraft:zombie", 1.5, 0.5, false);
        let baby_sheep = creature(&mut h, "minecraft:sheep", 2.5, 0.5, true);
        let pig = creature(&mut h, "minecraft:pig", 6.5, 0.5, false);
        visible(&mut h, e, vec![Who::Entity(zombie), Who::Entity(baby_sheep), Who::Entity(pig)]);
        scan_any(&mut h.world, e, &ctx(&TAG));
        assert_eq!(nearest(&h, e), Some(Who::Entity(pig)), "témoin : l'adulte d'une espèce du tag");
        scan_any(&mut h.world, e, &ctx(&NO_TAG));
        assert_eq!(nearest(&h, e), None, "un tag sans espèces ne retient personne");
    }

    /// Without the visible list, `ANY` leaves the memory as it is (its early return); a player is never chosen.
    #[test]
    fn without_the_list_any_keeps_its_memory_and_players_are_skipped() {
        let mut h = Herd::with_slab(40, true, false);
        let e = brained(&mut h);
        let pig = creature(&mut h, "minecraft:pig", 6.5, 0.5, false);
        visible(&mut h, e, vec![Who::Entity(pig)]);
        scan_any(&mut h.world, e, &ctx(&TAG));
        assert_eq!(nearest(&h, e), Some(Who::Entity(pig)));
        erase(&mut h.world, e, &VISIBLE_MOBS);
        scan_any(&mut h.world, e, &ctx(&TAG));
        assert_eq!(nearest(&h, e), Some(Who::Entity(pig)), "sans liste, la mémoire garde sa valeur");
        h.world.resource_mut::<crate::goals::Online>().set(crate::goals::Player { id: 1, x: 3.5, y: 1.0, z: 0.5, main: 0, off: 0, ..Default::default() });
        visible(&mut h, e, vec![Who::Player(1)]);
        scan_any(&mut h.world, e, &ctx(&TAG));
        assert_eq!(nearest(&h, e), None, "un joueur n'est jamais un adulte suivi");
    }
}
