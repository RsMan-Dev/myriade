//! The looting RULE of the dolphin (`game/mobs/foxes.rs:101-128`, fiche 110 §5): no goal, no flag. When the dolphin changes
//! column (`CellMoves`, read by the tick of this module before it is cleared), the items of its box grown by `reach`
//! (1 x 0 x 1) that are pickable are taken when its hand is EMPTY: the whole stack goes into the hand (`whole_stack`).
//! DOUTE (fiche 110 §5, BESOIN DU CADRE « objet apparu / entré dans l'eau près d'une créature ») : an item that drifts to a
//! dolphin that does not move is not seen; the Mob looks every tick. Not ported: the drop chance of the hand (2,0) and the
//! `entity_grief` of the Mob (`mob_griefing` is read, as `picks_up`).
use super::*;
use crate::cow::*;
use crate::goals::*;
use crate::item::{self, Item};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The box of the animal grown by this much (x, y, z) is where the items are looked for.
    pub reach: [f64; 3],
    /// The whole stack is taken (`true`, the dolphin) or one item (`false`).
    pub whole_stack: bool,
    /// `canPickUpLoot` of the species.
    pub picks_up: bool,
}
impl Params {
    pub const DEFAULT: Self = Self { reach: [1.0, 0.0, 1.0], whole_stack: true, picks_up: true };
}
/// The rule's data of the animal.
#[derive(Component, Clone, Copy, Debug)]
pub struct Pickup {
    reach: [f64; 3],
    whole_stack: bool,
    picks_up: bool,
}

pub static MODULE: Module = Module {
    name: "item_pickup",
    flags: 0,
    equip,
    fired: never,
    can_use: never,
    tick: Some(tick),
    requires: &[Cap::Body, Cap::Goals],
    ..Module::DEFAULT
};
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    let p = *params::<Params>(p);
    w.entity_mut(e).insert(Pickup { reach: p.reach, whole_stack: p.whole_stack, picks_up: p.picks_up });
}
/// A rule, not a goal: nothing starts it.
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
fn tick(w: &mut World) {
    crate::goals::tick_state(w, moved);
}
/// The animals that changed column this tick and carry the rule look at their items.
fn moved(moves: Res<CellMoves>, q: Query<(), With<Pickup>>, mut reqs: ResMut<Requests>) {
    for &e in &moves.0 {
        if q.contains(e) {
            reqs.0.push(Req::Call(e, look));
        }
    }
}
/// The rule now: the first pickable item of the grown box is taken into an empty hand (`Mob.aiStep`'s loop stops there
/// because the hand is then full).
pub fn look(w: &mut World, e: Entity) {
    let Some(p) = w.get::<Pickup>(e).copied() else { return };
    if !p.picks_up || !mob_griefing(w) || main_hand(w, e).is_some() {
        return;
    }
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return };
    let area = b.inflate(Vec3::new(p.reach[0], p.reach[1], p.reach[2]));
    for it in item::items_in(w, area) {
        if !item::pickable(w, it) {
            continue;
        }
        let Some(stack) = w.get::<Item>(it).map(|i| i.stack.count) else { continue };
        let count = if p.whole_stack { stack } else { 1 };
        let Some(taken) = item::pick_up(w, e, it, count) else { continue };
        set_main_hand(w, e, Some(taken));
        return;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::Entry;
    use crate::cow::{ActiveUntil, Herd, SpeciesTable, spawn_species};
    use crate::species::Config;
    use horizon_client_lib::items::ItemStack;
    use horizon_core::geometry::Vec3 as V;

    fn config() -> Config {
        let entries: &'static [Entry] = Box::leak(Box::new([Entry::new(&MODULE, 7, &Params::DEFAULT)]));
        Config { entries }
    }
    /// A dolphin with the rule alone, and a stack of 5 on the ground at its feet.
    fn world() -> Option<(Herd, Entity)> {
        std::env::var_os("HORIZON_TEST_RESOURCES")?;
        let mut h = Herd::with_slab(30, false, false);
        crate::goals::set_lazy_look(&mut h.world, false);
        let table = h.world.resource::<SpeciesTable>().0.clone();
        let mut sp = *table.iter().find(|s| s.name == "minecraft:dolphin").unwrap();
        sp.goals = config();
        let d = spawn_species(&mut h.world, &sp, 0.5, 0.5, 61);
        h.world.get_mut::<ActiveUntil>(d).unwrap().0 = u64::MAX;
        for _ in 0..5 {
            h.tick();
        }
        let y = h.world.get::<Pos>(d).unwrap().0.y;
        item::drop_item(&mut h.world, ItemStack::new("minecraft:cod", 5), V::new(0.5, y, 0.5), V::ZERO, 0);
        Some((h, d))
    }

    /// A column change lets the dolphin take the whole stack into its empty hand (witness: the hand is empty before).
    #[test]
    fn a_dolphin_takes_the_whole_stack_into_an_empty_hand() {
        let Some((mut h, d)) = world() else {
            eprintln!("skipped: HORIZON_TEST_RESOURCES unset");
            return;
        };
        assert!(main_hand(&h.world, d).is_none(), "témoin : la main est vide au départ");
        h.world.resource_mut::<CellMoves>().0.push(d);
        h.tick();
        let held = main_hand(&h.world, d).map(|s| (s.item.to_string(), s.count));
        assert_eq!(held, Some(("minecraft:cod".to_string(), 5)), "la pile entière va en main");
    }
}
