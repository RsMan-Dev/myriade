//! A boat's hurt, break, use and chest (`hurt_boat_result`, `interact_boat`, `open_chest_boat_container` in the host). The wear of a hit
//! decays one tick at a time on a wheel deadline, only while `Hurt` is present. The loot is not copied here: `take_loot` hands the host
//! the table and the seed of a chest boat, and the host rolls it.
use crate::boat_leash;
use crate::boat_state::{Chest, Hurt, Wood, CHEST_SLOTS};
use crate::cow::{after, Pos};
use crate::goals::{self, Online};
use crate::host_out_boat_hurt::BoatHurtEvent;
use crate::frame_item::DropRng;
use crate::item::spawn_at_location;
use crate::leash::{self, Holder};
use crate::seat;
use crate::species::boats::entity_id;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::random::LegacyRandom;

/// What a hit did: `Ignored` when the boat cannot be hurt now, `Broken` when it breaks (`drop`: its item and contents fall).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitResult {
    Ignored,
    Hurt,
    Broken { drop: bool },
}

/// What a right click did: `Mounted` a seat, `Opened` the chest, `Leashed` by a lead, `Unleashed` by the holder's empty hand, or `Pass`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UseResult {
    Mounted,
    Opened,
    Leashed,
    Unleashed,
    Pass,
}

/// The source of a hit: `invulnerable` when it cannot hurt the boat (`isInvulnerableToBase`); `entity_drops` is the game rule
/// `ENTITY_DROPS` (the host's), without which a break drops nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct HurtFlags {
    pub invulnerable: bool,
    pub entity_drops: bool,
}

/// The game rule `ENTITY_DROPS` as the host last set it; absent, the rule's default (on).
#[derive(Resource, Clone, Copy)]
pub struct EntityDrops(pub bool);

/// The blows a blast gives the boats when the host plays them (`host`): the host runs its mods' `vehicle_damage` hook and its ordinary hit
/// on each, in order, so a blast is no second path. Entity, damage, attacker.
#[derive(Resource, Default)]
pub struct BoatBlows {
    pub host: bool,
    pub queue: Vec<(Entity, f32, Option<Who>)>,
}

#[derive(Resource, Default)]
pub struct BoatHurtEvents(pub Vec<BoatHurtEvent>);

/// The events since the last drain.
pub fn drain(w: &mut World) -> Vec<BoatHurtEvent> {
    w.get_resource_mut::<BoatHurtEvents>().map_or_else(Vec::new, |mut e| std::mem::take(&mut e.0))
}

/// A hit on boat `e`: `Hurt` is set to its ten ticks and the damage grows by ten times `amount`. Past 40 damage, or from a creative
/// player, the boat breaks; it drops its item only when the attacker is not creative and the drops are on. A break also takes the lead.
pub fn hit(w: &mut World, e: Entity, amount: f32, attacker: Option<Who>, creative: bool, flags: HurtFlags) -> HitResult {
    if flags.invulnerable || w.get::<Wood>(e).is_none() {
        return HitResult::Ignored;
    }
    // The shake is idle at zero: `hurtDir` stays as the last hit left it (it is data), only the countdown stops.
    let fresh = w.get::<Hurt>(e).is_none_or(|h| h.time == 0 && h.damage == 0.0);
    // `VehicleEntity.DATA_ID_HURTDIR` is defined as 1: the first hit turns it to -1.
    let mut h = w.get::<Hurt>(e).copied().unwrap_or(Hurt { time: 0, dir: 1, damage: 0.0 });
    h.time = 10;
    h.dir = if h.dir > 0 { -1 } else { 1 };
    h.damage += amount * 10.0;
    w.entity_mut(e).insert(h);
    if fresh {
        // Vanilla's tracker sends before the entities tick: the first state seen is the full ten, the first decay is one tick later.
        after(w, e, 2, hurt_step);
    }
    w.get_resource_or_insert_with(BoatHurtEvents::default).0.push(BoatHurtEvent::Hit { entity: e, damage: amount, by: attacker });
    if !creative && h.damage <= 40.0 {
        return HitResult::Hurt;
    }
    let drop = !creative && flags.entity_drops;
    let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let lead = leash::detach(w, e).is_some() && drop;
    if drop {
        break_drops(w, e, lead);
    }
    w.entity_mut(e).despawn();
    w.get_resource_or_insert_with(BoatHurtEvents::default).0.push(BoatHurtEvent::Broke { at, by: attacker });
    HitResult::Broken { drop }
}

/// `AbstractBoat.destroy`'s drops: the boat's item, the stacks of its chest and the lead it held fall where the boat stood, each thrown
/// as `spawnAtLocation` throws it. The boat's item is the boat's own id (`minecraft:oak_boat`, `minecraft:oak_chest_boat`).
fn break_drops(w: &mut World, e: Entity, lead: bool) {
    let at = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let wood = w.get::<Wood>(e).map_or(0, |x| x.0);
    let chest = w.get::<Chest>(e).is_some();
    let slots = w.get_mut::<Chest>(e).map(|mut c| std::mem::take(&mut c.slots)).unwrap_or_default();
    let mut stacks = vec![ItemStack::new(entity_id(wood, chest), 1)];
    stacks.extend(slots.into_iter().flatten());
    if lead {
        stacks.push(ItemStack::new("minecraft:lead", 1));
    }
    // The throws' generator is seeded by one draw of the drops' own generator (`DropRng`, the world's seed).
    let seed = w.get_resource_or_insert_with(DropRng::default).0.next();
    let mut rng = LegacyRandom::new(i64::from(seed));
    for stack in stacks {
        spawn_at_location(w, stack, at, &mut rng);
    }
}

/// One tick of wear: `time` and `damage` go down to zero, then `Hurt` is removed. The chain runs only while `Hurt` is present.
fn hurt_step(w: &mut World, e: Entity) {
    let Some(mut h) = w.get::<Hurt>(e).copied() else { return };
    h.time = (h.time - 1).max(0);
    h.damage = (h.damage - 1.0).max(0.0);
    w.entity_mut(e).insert(h);
    if h.time != 0 || h.damage != 0.0 {
        after(w, e, 1, hurt_step);
    }
}

/// A player's right click on boat `e` with the item of `hand` (0 main, else off). A lead leashes it; the holder's empty hand takes the
/// lead off; a sneak (or no seat free) on a chest boat opens it; otherwise the player takes a seat, unless the boat is out of control
/// (`out_of_control`, the host's) or ridden by a creature's guest (`ridden_by_guest`, the host's).
pub fn boat_use(w: &mut World, e: Entity, player: u32, hand: u8, sneaking: bool, out_of_control: bool, ridden_by_guest: bool) -> UseResult {
    if w.get::<Wood>(e).is_none() {
        return UseResult::Pass;
    }
    let item = w.get_resource::<Online>().and_then(|o| o.get(player)).map(|p| if hand == 0 { p.main } else { p.off });
    if leash::is_leashed(w, e) {
        if item == Some(0) && leash::holder_of(w, e) == Some(Holder::Player(player)) && leash::detach(w, e).is_some() {
            return UseResult::Unleashed;
        }
    } else if item == Some(goals::lead()) {
        return if boat_leash::attach(w, e, Holder::Player(player)) { UseResult::Leashed } else { UseResult::Pass };
    }
    if w.get::<Chest>(e).is_some() && (sneaking || !seat::free_seat(w, e)) {
        return UseResult::Opened;
    }
    if sneaking || out_of_control || ridden_by_guest {
        return UseResult::Pass;
    }
    if seat::seat_player(w, player, e, None) {
        UseResult::Mounted
    } else {
        UseResult::Pass
    }
}

/// The slots of the chest of boat `e`; None on a boat without a chest.
pub fn items(w: &World, e: Entity) -> Option<&[Option<ItemStack>]> {
    w.get::<Chest>(e).map(|c| c.slots.as_slice())
}

/// Sets slot `slot` of the chest of boat `e`; false when there is no such slot.
pub fn set_slot(w: &mut World, e: Entity, slot: usize, stack: Option<ItemStack>) -> bool {
    let Some(mut c) = w.get_mut::<Chest>(e) else { return false };
    if slot >= CHEST_SLOTS {
        return false;
    }
    let Some(s) = c.slots.get_mut(slot) else { return false };
    *s = stack;
    true
}

/// Takes the loot table and its seed of boat `e`, once: the host rolls them. None when there is nothing pending.
pub fn take_loot(w: &mut World, e: Entity) -> Option<(String, i64)> {
    let l = w.get_mut::<Chest>(e)?.loot.take()?;
    Some((l.table, l.seed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boat_state;
    use crate::cow::Index;
    use crate::goals::Player;
    use crate::item::Item;
    use crate::leash::holder_of;

    fn world() -> World {
        let mut w = crate::cow::Herd::new(0, 1.0, true, false, 1).world;
        w.insert_resource(Index::square(32));
        set_hand(&mut w, goals::lead());
        w
    }

    fn set_hand(w: &mut World, main: u32) {
        let p = Player { id: 2, x: 0.0, y: 64.0, z: 0.0, main, off: 0, head: 0, chest: 0, legs: 0, feet: 0 };
        w.resource_mut::<Online>().set(p);
    }

    fn drops() -> HurtFlags {
        HurtFlags { entity_drops: true, ..Default::default() }
    }

    #[test]
    fn a_creative_hit_breaks_the_boat_without_drop() {
        let mut w = world();
        let e = boat_state::spawn(&mut w, 0, false, [0.0, 64.0, 0.0], 0.0);
        assert_eq!(hit(&mut w, e, 1.0, Some(Who::Player(2)), true, drops()), HitResult::Broken { drop: false });
    }

    #[test]
    fn four_hits_of_one_keep_the_boat_and_the_fifth_breaks_it() {
        let mut w = world();
        let e = boat_state::spawn(&mut w, 0, false, [0.0, 64.0, 0.0], 0.0);
        for _ in 0..4 {
            assert_eq!(hit(&mut w, e, 1.0, None, false, drops()), HitResult::Hurt);
        }
        assert_eq!(hit(&mut w, e, 1.0, None, false, drops()), HitResult::Broken { drop: true });
    }

    #[test]
    fn no_drop_without_the_entity_drops_rule() {
        let mut w = world();
        let e = boat_state::spawn(&mut w, 0, false, [0.0, 64.0, 0.0], 0.0);
        for _ in 0..4 {
            assert_eq!(hit(&mut w, e, 1.0, None, false, HurtFlags::default()), HitResult::Hurt);
        }
        assert_eq!(hit(&mut w, e, 1.0, None, false, HurtFlags::default()), HitResult::Broken { drop: false });
    }

    #[test]
    fn an_invulnerable_hit_is_ignored() {
        let mut w = world();
        let e = boat_state::spawn(&mut w, 0, false, [0.0, 64.0, 0.0], 0.0);
        assert_eq!(hit(&mut w, e, 5.0, None, false, HurtFlags { invulnerable: true, entity_drops: true }), HitResult::Ignored);
        assert!(w.get::<Hurt>(e).is_none());
    }

    #[test]
    fn a_chest_boat_keeps_its_27_slots_through_save_and_load() {
        let mut w = world();
        let e = boat_state::spawn(&mut w, 0, true, [0.0, 64.0, 0.0], 0.0);
        let c = boat_state::save(&w, e).expect("saved");
        let n = boat_state::load(&mut w, &c).expect("loaded");
        assert_eq!(items(&w, n).map(|s| s.len()), Some(CHEST_SLOTS));
    }

    #[test]
    fn a_lead_in_hand_leashes_the_boat_and_the_empty_hand_takes_it_off() {
        let mut w = world();
        let e = boat_state::spawn(&mut w, 0, false, [0.0, 64.0, 0.0], 0.0);
        assert_eq!(boat_use(&mut w, e, 2, 0, false, false, false), UseResult::Leashed);
        assert_eq!(holder_of(&w, e), Some(Holder::Player(2)));
        set_hand(&mut w, 0);
        assert_eq!(boat_use(&mut w, e, 2, 0, false, false, false), UseResult::Unleashed);
        assert!(holder_of(&w, e).is_none());
    }

    fn ground(w: &mut World) -> Vec<String> {
        let mut names: Vec<String> = crate::queries::with::<&'static Item, (), _>(w, |q, w| q.iter(w).map(|it| it.stack.item.to_string()).collect());
        names.sort();
        names
    }

    #[test]
    fn a_break_drops_the_lead_it_held_on_the_ground() {
        let mut w = world();
        let e = boat_state::spawn(&mut w, 0, false, [0.0, 64.0, 0.0], 0.0);
        boat_use(&mut w, e, 2, 0, false, false, false);
        assert_eq!(hit(&mut w, e, 5.0, None, false, drops()), HitResult::Broken { drop: true });
        assert!(holder_of(&w, e).is_none());
        assert_eq!(ground(&mut w), ["minecraft:lead", "minecraft:oak_boat"]);
    }

    #[test]
    fn a_broken_chest_boat_leaves_its_stacks_on_the_ground() {
        let mut w = world();
        let e = boat_state::spawn(&mut w, 0, true, [0.0, 64.0, 0.0], 0.0);
        set_slot(&mut w, e, 0, Some(ItemStack::new("minecraft:stick", 3)));
        set_slot(&mut w, e, 26, Some(ItemStack::new("minecraft:apple", 1)));
        assert_eq!(hit(&mut w, e, 5.0, None, false, drops()), HitResult::Broken { drop: true });
        assert!(w.get_entity(e).is_err());
        assert_eq!(ground(&mut w), ["minecraft:apple", "minecraft:oak_chest_boat", "minecraft:stick"]);
    }

    #[test]
    fn the_hurt_goes_idle_once_it_has_decayed_keeping_its_direction() {
        let mut w = world();
        let e = boat_state::spawn(&mut w, 0, false, [0.0, 64.0, 0.0], 0.0);
        hit(&mut w, e, 1.0, None, false, drops());
        for _ in 0..10 {
            hurt_step(&mut w, e);
        }
        let h = w.get::<Hurt>(e).unwrap();
        assert_eq!((h.time, h.damage, h.dir), (0, 0.0, -1));
    }
}
