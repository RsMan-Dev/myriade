//! The looting block of `Mob.aiStep` for a creature that can pick up loot (`game/mobs/mob_looting.rs`, `Mob.pickUpItem` / `equipItemIfPossible`): a RULE
//! without a flag and without a goal. When it changes column (`CellMoves`), the items of its box grown by `reach` (1, 0, 1), in the order they joined, are
//! taken when `wants_to_pick_up` (the species' refusals, as parameters) and they beat what the creature has in the slot they are made for (armor, then
//! toughness, then enchantments for a piece of armor; the preferred weapons, then the attack damage for a hand: `equip::can_replace_current_item`, the
//! `Mob`'s own function, on the numbers the item table gives). What it replaces falls by the drop chance (0,085: `max(nextFloat - 0.1, 0) < chance`), what it
//! took is never lost at death (`Looted`: the drop chance becomes 2,0) and it never despawns. A piece of armor that beats nothing falls back to an empty main
//! hand. Not ported: the enchantments, the damage and the name of the stack (the table gives ids: `Facts` carry zeros), the `CanEquip` hook, `Drowned`'s
//! nautilus shell beyond `never_replace`, the stack of a hand taken whole (one item is taken), a pickup woken by an item that lands beside a creature that does
//! not move (the `Mob` looks every tick; here a creature looks when it changes column).
use super::*;
use crate::items::{self, worn_in};
use crate::rules::pick_up_gear::{drop_chance, drops_old};
use crate::{cow::*, goals::*, item};
use horizon_client_lib::mobs::equip::{can_replace_current_item, Facts, Slot as MSlot};
use horizon_core::geometry::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Params {
    /// The box of the creature is grown by this much to find the items.
    pub reach: [f64; 3],
    /// The items it never takes (names or `#tags`: the spears of a skeleton, the glow ink of a zombie), and, when not empty, the only ones it takes
    /// (`PiglinBrute.wantsToPickUp`: a golden axe).
    pub refuses: &'static [&'static str],
    pub only: &'static [&'static str],
    /// `Mob.getPreferredWeaponType`: a tag; a weapon of the tag beats one outside it.
    pub preferred_weapons: Option<&'static str>,
    /// What it never replaces (`Drowned.canReplaceCurrentItem`: the nautilus shell).
    pub never_replace: &'static [&'static str],
    /// `DropChances` of an equipment slot by default.
    pub drop_chance: f32,
}
impl Params {
    pub const DEFAULT: Self = Self { reach: [1.0, 0.0, 1.0], refuses: &[], only: &[], preferred_weapons: None, never_replace: &[], drop_chance: 0.085 };
}
#[derive(Component, Clone, Copy, Debug)]
pub struct Gear {
    p: &'static Params,
}
/// The slots whose item the creature picked up (a bit per `Slot` ordinal of `slot_bit`): they drop at its death whatever the roll.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Looted(pub u8);

pub static MODULE: Module = Module { name: "pick_up_gear", flags: 0, equip, fired: never, can_use: never, tick: Some(tick), requires: &[Cap::Body, Cap::Goals], ..Module::DEFAULT };
fn never(_: &mut World, _: Entity, _: Inst) -> bool {
    false
}
fn equip(w: &mut World, e: Entity, _: Inst, p: &'static (dyn std::any::Any + Sync)) {
    w.entity_mut(e).insert((Gear { p: params::<Params>(p) }, Looted::default()));
}
/// A module tick: called only while a creature that loots lives (no registered system).
fn tick(w: &mut World) {
    tick_state(w, moved);
}
fn moved(moves: Res<CellMoves>, items: Res<item::ItemIndex>, q: Query<(), With<Gear>>, mut reqs: ResMut<Requests>) {
    // Without a dropped item in the world no look can find anything: no request per cell move (the chase of a crowd moves cells all the time).
    if moves.0.is_empty() || items.is_empty() || q.is_empty() {
        return;
    }
    for &e in &moves.0 {
        if q.contains(e) {
            reqs.0.push(Req::Call(e, look));
        }
    }
}
pub fn slot_bit(s: Slot) -> u8 {
    1 << match s {
        Slot::Main => 0,
        Slot::Off => 1,
        Slot::Feet => 2,
        Slot::Legs => 3,
        Slot::Chest => 4,
        Slot::Head => 5,
        Slot::Body => 6,
        Slot::Saddle => 7,
    }
}
fn listed(names: &[&str], id: u32) -> bool {
    names.iter().any(|n| items::is(n, id))
}
fn facts(id: u32) -> Facts {
    let (armor, armor_toughness) = items::armor_of(id);
    Facts { armor, armor_toughness, attack_damage: items::attack_damage_of(id), ..Facts::default() }
}
fn mob_slot(s: Slot) -> MSlot {
    match s {
        Slot::Head => MSlot::Head,
        Slot::Chest => MSlot::Chest,
        Slot::Legs => MSlot::Legs,
        Slot::Feet => MSlot::Feet,
        Slot::Off => MSlot::OffHand,
        _ => MSlot::MainHand,
    }
}
/// `Mob.canReplaceCurrentItem` for the item `new` against what is in `slot` now (0: nothing).
fn can_replace(g: &Gear, slot: Slot, new: u32, current: u32) -> bool {
    if current == 0 {
        return true;
    }
    if listed(g.p.never_replace, current) {
        return false;
    }
    let preferred = g.p.preferred_weapons.map(|t| (items::has_tag(t, new), items::has_tag(t, current)));
    can_replace_current_item(&facts(new), Some(&facts(current)), mob_slot(slot), preferred)
}
/// The looting of this creature now: every item of its grown box that is pickable and that it wants and can use is taken (one of the stack), in order.
pub fn look(w: &mut World, e: Entity) {
    let Some(g) = w.get::<Gear>(e).copied() else { return };
    if !mob_griefing(w) {
        return;
    }
    let Some(b) = w.get::<Bounds>(e).map(|b| b.0) else { return };
    let reach = b.inflate(Vec3::new(g.p.reach[0], g.p.reach[1], g.p.reach[2]));
    for it in item::items_in(w, reach) {
        if !item::pickable(w, it) {
            continue;
        }
        let Some(name) = w.get::<item::Item>(it).map(|i| i.stack.item.to_string()) else { continue };
        let id = items::item_id(&name);
        if id == 0 || listed(g.p.refuses, id) || (!g.p.only.is_empty() && !listed(g.p.only, id)) {
            continue;
        }
        // `getEquipmentSlotForItem`: the slot the item is worn in, else the main hand.
        let mut slot = worn_in(id).filter(|s| matches!(s, Slot::Head | Slot::Chest | Slot::Legs | Slot::Feet | Slot::Off)).unwrap_or(Slot::Main);
        let mut replace = can_replace(&g, slot, id, equipped(w, e, slot));
        if !replace && !matches!(slot, Slot::Main | Slot::Off) {
            // An armor piece that beats nothing it is worn with falls back to an empty main hand.
            slot = Slot::Main;
            replace = equipped(w, e, Slot::Main) == 0;
        }
        if !replace {
            continue;
        }
        let Some(taken) = item::pick_up(w, e, it, 1) else { continue };
        let old = equipped(w, e, slot);
        let looted = w.get::<Looted>(e).copied().unwrap_or_default().0;
        let chance = drop_chance(looted & slot_bit(slot) != 0, g.p.drop_chance);
        let drop_old = old != 0 && drops_old(with_rng(w, e, |r| r.next_float()), chance);
        crate::equipment::set_slot(w, e, slot, id);
        if slot == Slot::Main {
            set_main_hand(w, e, Some(taken));
        }
        w.entity_mut(e).insert(Looted(looted | slot_bit(slot)));
        if drop_old && let (Some(at), Some(stack)) = (w.get::<Pos>(e).map(|p| p.0), items::item_name(old)) {
            item::drop_item(w, horizon_client_lib::items::ItemStack::new(stack, 1), at, Vec3::ZERO, 10);
        }
    }
}
