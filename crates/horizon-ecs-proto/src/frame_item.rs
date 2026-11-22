//! Item frames (`ItemFrame`, glow frames too): a hanging entity that holds one item and turns it. State: `Held` (the stack, its
//! rotation in 0..8, its drop chance) and the sparse `Fixed` and `Invisible`, present only when set. The wall and the survival
//! deadline are `hanging.rs`'s: nothing here ticks. Each act that the host follows (a placement, a put, a turn, an item out, a break)
//! writes one `FrameEvent` with the player who did it; the caller learns whether the act happened from the returned bool.
use crate::flat::Rng;
use crate::goals::Events;
use crate::hanging::{self, Attached, Walls, GLOW_ITEM_FRAME, ITEM_FRAME, KINDS};
use crate::host_out_frames::{FrameEvent, Spot};
use crate::rules::hanging::step;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_items::item_nbt::{item_from_nbt, item_to_nbt};

/// The item of a frame (`ItemFrame.item`), its rotation (`ItemFrame.rotation`, 0..8 in 45 degree steps) and its drop chance
/// (`ItemFrame.dropChance`, 1.0 unless set).
#[derive(Component, Clone, Debug)]
pub struct Held {
    pub stack: Option<ItemStack>,
    pub rotation: u8,
    pub drop_chance: f32,
}
/// The random draws of the drops (`level.random`): a frame's held item falls when a draw is below its `dropChance`.
#[derive(Resource)]
pub struct DropRng(pub Rng);
impl Default for DropRng {
    fn default() -> Self {
        DropRng(Rng(0x9E37_79B9_7F4A_7C15))
    }
}
/// `ItemFrame.fixed`: a frame a player's hit does not break. Sparse: only the fixed frames carry it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Fixed(pub bool);
/// `ItemFrame.invisible`: a frame drawn without its model. Sparse: only the invisible frames carry it.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Invisible(pub bool);

fn emit(w: &mut World, ev: FrameEvent) {
    w.resource_mut::<Events>().host.frame(ev);
}

/// The spot of a hanging entity: its cell, its face, its kind.
pub(crate) fn spot(w: &World, e: Entity) -> Option<Spot> {
    let a = w.get::<Attached>(e)?;
    Some(Spot { at: a.pos, face: a.face, glow: a.kind == GLOW_ITEM_FRAME })
}

/// Where an item falls from a spot: the centre of the cell, a half step back toward the wall (`ItemFrame.calculateBoundingBox`,
/// `Entity.spawnAtLocation` at the entity's position).
pub(crate) fn drop_at(s: Spot) -> [f64; 3] {
    let d = step(s.face);
    std::array::from_fn(|i| f64::from(s.at[i]) + 0.5 - 0.46875 * f64::from(d[i]))
}

/// `ItemFrame.dropItem`: the held item falls when a draw is below the frame's `dropChance`.
fn falls(w: &mut World, e: Entity) -> bool {
    let chance = w.get::<Held>(e).map_or(1.0, |h| h.drop_chance);
    let draw = w.get_resource_or_insert_with(DropRng::default).0.next();
    f64::from(draw) / 4_294_967_296.0 < f64::from(chance)
}

pub(crate) fn holds(w: &World, e: Entity) -> bool {
    w.get::<Held>(e).is_some_and(|h| h.stack.is_some())
}

/// The frame a ray from `eye` along `dir` (a unit vector) crosses first within `reach`, with the distance `t`.
pub fn aimed(w: &World, eye: [f64; 3], dir: [f64; 3], reach: f64) -> Option<(Entity, f64)> {
    hanging::aimed(w, eye, dir, reach, |w, e| w.get::<Held>(e).is_some())
}

/// `ItemFrameItem.useOn`: a frame (glow or not) on `face` of the block `pos`, when the wall holds. None when it does not.
pub fn place(w: &mut World, pos: [i32; 3], face: u8, glow: bool, who: Option<Who>) -> Option<Entity> {
    let walls_hold = w.get_resource::<Walls>().map_or(true, |f| (f.0)(w, pos, face, (1, 1)));
    if !walls_hold {
        return None;
    }
    let kind = if glow { GLOW_ITEM_FRAME } else { ITEM_FRAME };
    let e = hanging::hang(w, pos, face, kind, None);
    w.entity_mut(e).insert(Held { stack: None, rotation: 0, drop_chance: 1.0 });
    emit(w, FrameEvent::Placed { who, spot: Spot { at: pos, face, glow } });
    Some(e)
}

/// `ItemFrame.setRotation`: a frame that holds an item turns one step, modulo 8. False when it is empty.
pub fn rotate(w: &mut World, e: Entity, who: Option<Who>) -> bool {
    if !holds(w, e) {
        return false;
    }
    let Some(s) = spot(w, e) else { return false };
    if let Some(mut h) = w.get_mut::<Held>(e) {
        h.rotation = (h.rotation + 1) % 8;
    }
    emit(w, FrameEvent::Rotated { who, spot: s });
    true
}

/// `ItemFrame.setRotation`: the rotation takes `rotation % 8`. No event, as the setter sends none; False when `e` is not a frame.
pub fn set_rotation(w: &mut World, e: Entity, rotation: u8) -> bool {
    let Some(mut h) = w.get_mut::<Held>(e) else { return false };
    h.rotation = rotation % 8;
    true
}

/// `ItemFrame.setItem` on an empty frame: it takes `stack` (the caller takes it from the hand). The host consumes the `Put`. False
/// when the frame holds an item already.
pub fn put(w: &mut World, e: Entity, stack: ItemStack, who: Option<Who>) -> bool {
    if holds(w, e) {
        return false;
    }
    let Some(s) = spot(w, e) else { return false };
    let Some(mut h) = w.get_mut::<Held>(e) else { return false };
    h.stack = Some(stack.clone());
    drop(h);
    emit(w, FrameEvent::Put { who, spot: s, stack });
    true
}

/// `ItemFrame.setFixed` and `setInvisible` (the command): the flag takes `on`. False when `e` is not a frame.
pub fn set_fixed(w: &mut World, e: Entity, on: bool) -> bool {
    if w.get::<Held>(e).is_none() {
        return false;
    }
    w.entity_mut(e).insert(Fixed(on));
    true
}

/// See `set_fixed`, for the invisible flag.
pub fn set_invisible(w: &mut World, e: Entity, on: bool) -> bool {
    if w.get::<Held>(e).is_none() {
        return false;
    }
    w.entity_mut(e).insert(Invisible(on));
    true
}

/// `ItemFrame.setItem` (the `/item` command): the stack takes the slot as one of it, the rotation kept. No event: the host made the
/// change itself. False when `e` is not a frame.
pub fn set_item(w: &mut World, e: Entity, stack: ItemStack) -> bool {
    let Some(mut h) = w.get_mut::<Held>(e) else { return false };
    h.stack = (stack.count > 0).then(|| ItemStack::new(&*stack.item, 1));
    true
}

/// `ItemFrame.removeItem`: the item comes out of the frame (the frame stays). It falls when `drop` is on and the chance holds.
fn take(w: &mut World, e: Entity, who: Option<Who>, drop: bool) -> Option<ItemStack> {
    let s = spot(w, e)?;
    let fall = drop && falls(w, e);
    let stack = w.get_mut::<Held>(e)?.stack.take()?;
    emit(w, FrameEvent::Removed { who, spot: s, stack: stack.clone(), drop: fall, drop_at: drop_at(s) });
    Some(stack)
}

/// The frame goes (`ItemFrame.dropItem`): the frame (`Broke`) then the held item (`Removed`), which falls by its chance. `creative`:
/// nothing falls. Used by a player's hit and by a survival break (`who` None, not creative).
pub(crate) fn break_out(w: &mut World, e: Entity, spot: Spot, who: Option<Who>, creative: bool) {
    let fall = !creative && holds(w, e) && falls(w, e);
    let stack = w.get_mut::<Held>(e).and_then(|mut h| h.stack.take());
    w.despawn(e);
    emit(w, FrameEvent::Broke { who, spot, drop: !creative, drop_at: drop_at(spot) });
    if let Some(stack) = stack {
        emit(w, FrameEvent::Removed { who, spot, stack, drop: fall, drop_at: drop_at(spot) });
    }
}

/// `ItemFrame.hurtServer` on a frame that is not fixed: a filled frame gives its item up (`take`), an empty one breaks and goes.
/// `creative`: the attacker is in creative mode, so nothing falls. False for a fixed frame, a frame that does not exist, or an
/// entity that is not a frame.
pub fn hit(w: &mut World, e: Entity, who: Option<Who>, creative: bool) -> bool {
    if w.get::<Fixed>(e).is_some_and(|f| f.0) || w.get::<Held>(e).is_none() {
        return false;
    }
    if holds(w, e) {
        return take(w, e, who, !creative).is_some();
    }
    let Some(s) = spot(w, e) else { return false };
    break_out(w, e, s, who, creative);
    true
}

/// The `block_pos` of a saved compound (`BlockAttachedEntity.TAG_BLOCK_POS`).
pub(crate) fn block_pos(c: &Compound) -> Option<[i32; 3]> {
    let Some(Tag::IntArray(p)) = c.get(&JavaString::from("block_pos")) else { return None };
    p.as_slice().try_into().ok()
}

/// The keys of an item frame (`ItemFrame.addAdditionalSaveData`, its base's `block_pos` and the entity `id` included).
pub fn save(w: &World, e: Entity) -> Option<Compound> {
    let a = w.get::<Attached>(e)?;
    let h = w.get::<Held>(e)?;
    let mut c = Compound::new();
    c.insert("id".into(), Tag::String(KINDS[usize::from(a.kind)].item.into()));
    c.insert("block_pos".into(), Tag::IntArray(a.pos.to_vec()));
    c.insert("Facing".into(), Tag::Byte(a.face as i8));
    c.insert("ItemRotation".into(), Tag::Byte(h.rotation as i8));
    c.insert("ItemDropChance".into(), Tag::Float(h.drop_chance.to_bits()));
    if let Some(s) = &h.stack {
        c.insert("Item".into(), item_to_nbt(s));
    }
    c.insert("Invisible".into(), Tag::Byte(i8::from(w.get::<Invisible>(e).is_some_and(|i| i.0))));
    c.insert("Fixed".into(), Tag::Byte(i8::from(w.get::<Fixed>(e).is_some_and(|f| f.0))));
    Some(c)
}

/// `ItemFrame.readAdditionalSaveData`: the frame is hung on its saved face without a wall check (it was saved on one). None when
/// its `block_pos` or its `Facing` is missing or invalid.
pub fn load(w: &mut World, c: &Compound) -> Option<Entity> {
    let pos = block_pos(c)?;
    let Some(Tag::Byte(f)) = c.get(&JavaString::from("Facing")) else { return None };
    let face = u8::try_from(*f).ok().filter(|&f| f < 6)?;
    let glow = matches!(c.get(&JavaString::from("id")), Some(Tag::String(s)) if s.to_string_lossy() == KINDS[usize::from(GLOW_ITEM_FRAME)].item);
    let flag = |key: &str| matches!(c.get(&JavaString::from(key)), Some(Tag::Byte(v)) if *v != 0);
    let stack = c.get(&JavaString::from("Item")).and_then(item_from_nbt);
    let rotation = match c.get(&JavaString::from("ItemRotation")) {
        Some(Tag::Byte(r)) => r.rem_euclid(8) as u8,
        _ => 0,
    };
    let drop_chance = match c.get(&JavaString::from("ItemDropChance")) {
        Some(Tag::Float(b)) => f32::from_bits(*b),
        _ => 1.0,
    };
    let e = hanging::hang(w, pos, face, if glow { GLOW_ITEM_FRAME } else { ITEM_FRAME }, None);
    w.entity_mut(e).insert(Held { stack, rotation, drop_chance });
    if flag("Fixed") {
        w.entity_mut(e).insert(Fixed(true));
    }
    if flag("Invisible") {
        w.entity_mut(e).insert(Invisible(true));
    }
    Some(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::{schedule, Clock};
    use horizon_core::geometry::Vec3;

    fn wall(_: &World, _: [i32; 3], _: u8, _: (u8, u8)) -> bool {
        true
    }
    fn wall_gone(_: &World, _: [i32; 3], _: u8, _: (u8, u8)) -> bool {
        false
    }
    fn world() -> World {
        let mut w = crate::cow::Herd::new(0, 1.0, true, false, 1).world;
        w.insert_resource(Walls(wall));
        w
    }

    #[test]
    fn a_frame_turns_one_step_at_a_time_modulo_eight() {
        let mut w = world();
        let e = place(&mut w, [0, 60, 0], 3, false, None).expect("the wall holds");
        assert!(!rotate(&mut w, e, None), "an empty frame does not turn");
        assert!(put(&mut w, e, ItemStack::new("minecraft:compass", 1), None));
        for turn in 1..=9u8 {
            assert!(rotate(&mut w, e, None));
            assert_eq!(w.get::<Held>(e).map(|h| h.rotation), Some(turn % 8));
        }
    }

    #[test]
    fn a_frame_saved_and_loaded_keeps_its_item_rotation_chance_and_kind() {
        let mut w = world();
        let e = place(&mut w, [4, 70, -2], 5, true, None).expect("the wall holds");
        put(&mut w, e, ItemStack::new("minecraft:diamond", 3), None);
        rotate(&mut w, e, None);
        rotate(&mut w, e, None);
        w.get_mut::<Held>(e).expect("held").drop_chance = 0.5;
        let c = save(&w, e).expect("a frame");
        let mut back = world();
        let l = load(&mut back, &c).expect("loaded");
        let held = back.get::<Held>(l).expect("held");
        assert_eq!(held.stack.as_ref().map(|s| (&*s.item, s.count)), Some(("minecraft:diamond", 3)));
        assert_eq!((held.rotation, held.drop_chance), (2, 0.5));
        let a = back.get::<Attached>(l).expect("attached");
        assert_eq!((a.pos, a.face, a.kind), ([4, 70, -2], 5, GLOW_ITEM_FRAME));
    }

    #[test]
    fn a_frame_whose_wall_is_gone_breaks_on_its_101st_tick() {
        let mut w = world();
        let e = place(&mut w, [0, 60, 0], 3, false, None).expect("the wall holds");
        let born = w.resource::<Clock>().0;
        w.insert_resource(Walls(wall_gone));
        hanging::arm(&mut w, e);
        let mut s = schedule();
        let mut broke = None;
        for t in 1..=200 {
            s.run(&mut w);
            if w.get_entity(e).is_err() {
                broke = Some(t);
                break;
            }
        }
        assert_eq!(broke, Some(101), "the check falls on the 101st tick after the birth at {born}");
    }

    #[test]
    fn a_blast_that_breaks_blocks_breaks_a_frame_unless_it_is_fixed_or_breaks_nothing() {
        let mut w = world();
        let loose = place(&mut w, [0, 60, 0], 3, false, None).expect("the wall holds");
        let fixed = place(&mut w, [1, 60, 0], 3, false, None).expect("the wall holds");
        w.entity_mut(fixed).insert(Fixed(true));
        let at = |x: f64| Vec3::new(x + 0.5, 60.5, 0.5);
        hanging::blast(&mut w, at(0.0), 1.0, None, false);
        assert!(w.get_entity(loose).is_ok(), "a blast that breaks no block spares the frame");
        hanging::blast(&mut w, at(0.0), 1.0, None, true);
        assert!(w.get_entity(loose).is_err(), "a blast that breaks blocks breaks the frame in reach");
        assert!(w.get_entity(fixed).is_ok(), "a fixed frame is not hurt by an explosion");
    }

    #[test]
    fn an_item_command_sets_one_of_the_stack_and_keeps_the_rotation() {
        let mut w = world();
        let e = place(&mut w, [0, 60, 0], 3, false, None).expect("the wall holds");
        w.get_mut::<Held>(e).expect("held").rotation = 3;
        assert!(set_item(&mut w, e, ItemStack::new("minecraft:map", 4)));
        let h = w.get::<Held>(e).expect("held");
        assert_eq!(h.stack.as_ref().map(|s| (&*s.item, s.count)), Some(("minecraft:map", 1)));
        assert_eq!(h.rotation, 3);
    }

    #[test]
    fn a_block_change_on_the_wall_or_in_the_cell_wakes_a_frame_for_its_deadline() {
        for changed in [[0, 60, -1], [0, 60, 0]] {
            let mut w = world();
            let e = place(&mut w, [0, 60, 0], 3, false, None).expect("the wall holds");
            w.insert_resource(Walls(wall_gone));
            w.insert_resource(crate::block_events::BlockEvents { changed: vec![changed] });
            hanging::attached_wake(&mut w);
            let mut s = schedule();
            let mut broke = None;
            for t in 1..=200 {
                s.run(&mut w);
                if w.get_entity(e).is_err() {
                    broke = Some(t);
                    break;
                }
            }
            assert_eq!(broke, Some(101), "a change at {changed:?} arms the check");
        }
    }
}
