//! The armor stand's own state (S6). Life, equipment, air and the box are the shared components (`Health`, `Equipment`, `Air`, `Body`,
//! `Bounds`). What is left is here: one byte of flags, the pose when it is not the default one, the disabled slots when one is.
//! A stand at rest has no `Moving`: nothing visits it, so a placed stand costs nothing per tick. The first four bits are the client
//! flags of the jar (`CLIENT_FLAG_*`); invisible and no gravity are entity flags of the jar, kept beside them.
use bevy_ecs::prelude::*;

pub const SMALL: u8 = 1;
pub const SHOW_ARMS: u8 = 4;
pub const NO_BASE_PLATE: u8 = 8;
pub const MARKER: u8 = 16;
pub const INVISIBLE: u8 = 32;
pub const NO_GRAVITY: u8 = 64;

/// The flags of a stand; absent on a plain stand.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stand {
    pub flags: u8,
}

/// `ArmorStand.hasPhysics`: a stand with a marker or no gravity never moves (`travel` does nothing).
pub fn has_physics(s: Stand) -> bool {
    s.flags & (MARKER | NO_GRAVITY) == 0
}

/// The air a stand has now, read rather than ticked: `min(300, air + 4 * (now - last))`, the NBT +4 a tick of `ArmorStand`.
/// `last` is the tick of the last write of the counter, so an idle stand costs nothing until it is read.
pub fn air_at(air: i32, last: u64, now: u64) -> i32 {
    let gained = i64::from(air) + 4 * (now.saturating_sub(last)) as i64;
    gained.min(300) as i32
}

/// The box of a stand (width, height): `ArmorStand.BABY_DIMENSIONS` (0.5 x 0.9875) when small, the type's 0.5 x 1.975 otherwise.
pub fn stand_size(s: Stand) -> (f32, f32) {
    if s.flags & SMALL != 0 { (0.5, 0.9875) } else { (0.5, 1.975) }
}

/// `ArmorStand.DEFAULT_*_POSE`: head, body, left arm, right arm, left leg, right leg (degrees).
pub const DEFAULT_POSE: [[f32; 3]; 6] = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [-10.0, 0.0, -10.0], [-15.0, 0.0, 10.0], [-1.0, 0.0, -1.0], [1.0, 0.0, 1.0]];

/// The pose of a stand that is not the default one (absent otherwise).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Pose(pub Box<[[f32; 3]; 6]>);

/// `ArmorStand.DisabledSlots`: bit `slot` (taking) and bit `slot + 8` (putting); absent while no slot is disabled.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisabledSlots(pub i32);

/// The pose a stand has now.
pub fn pose_of(w: &World, e: Entity) -> [[f32; 3]; 6] {
    w.get::<Pose>(e).map_or(DEFAULT_POSE, |p| *p.0)
}

/// Sets the pose: the component is kept only while the pose differs from the default one.
pub fn set_pose(w: &mut World, e: Entity, pose: [[f32; 3]; 6]) {
    if pose == DEFAULT_POSE {
        w.entity_mut(e).remove::<Pose>();
    } else {
        w.entity_mut(e).insert(Pose(Box::new(pose)));
    }
}

/// `EquipmentSlot` ordinal of the jar, as Horizon indexes a stand's slots: 0 main, 1 off, 2 feet, 3 legs, 4 chest, 5 head, 6 body, 7 saddle.
pub fn slot_of_index(i: usize) -> Option<crate::goals::Slot> {
    use crate::goals::Slot::*;
    [Main, Off, Feet, Legs, Chest, Head, Body, Saddle].get(i).copied()
}
/// `ArmorStand.filterFlag`: the bit of a slot in `DisabledSlots` (taking: bit `flag`; putting: bit `flag + 8` for a filled slot,
/// `flag + 16` for an empty one, as `swapItem` reads them).
fn filter_flag(slot: usize) -> u32 {
    [0, 5, 1, 2, 3, 4][slot]
}
/// `ArmorStand.isDisabled`: the slot is locked, or it is a hand and the stand has no arms.
pub fn is_disabled(w: &World, e: Entity, slot: usize) -> bool {
    let disabled = w.get::<DisabledSlots>(e).map_or(0, |d| d.0);
    let arms = w.get::<Stand>(e).is_some_and(|s| s.flags & SHOW_ARMS != 0);
    disabled & (1 << filter_flag(slot)) != 0 || (slot < 2 && !arms)
}
/// `ArmorStand.swapItem`'s locks: taking from a filled slot, or putting into an empty one, can each be disabled.
pub fn swap_locked(w: &World, e: Entity, slot: usize, filled: bool) -> bool {
    let offset = if filled { 8 } else { 16 };
    let disabled = w.get::<DisabledSlots>(e).map_or(0, |d| d.0);
    disabled & (1 << (filter_flag(slot) + offset)) != 0
}
/// `ArmorStand.getClickedSlot` on a click at `rel_y` above the stand's feet (divided by 0.5 when small): the slot whose height it is,
/// the main hand when none, and only where the slot has something to take (`has`).
pub fn clicked_slot(small: bool, rel_y: f32, has: impl Fn(usize) -> bool) -> usize {
    let y = if small { rel_y / 0.5 } else { rel_y };
    if y >= 0.1 && y < 0.1 + if small { 0.8 } else { 0.45 } && has(2) {
        2
    } else if y >= 0.9 + if small { 0.3 } else { 0.0 } && y < 0.9 + if small { 1.0 } else { 0.7 } && has(4) {
        4
    } else if y >= 0.4 && y < 0.4 + if small { 1.0 } else { 0.8 } && has(3) {
        3
    } else if y >= 1.6 && has(5) {
        5
    } else if !has(0) && has(1) {
        1
    } else {
        0
    }
}
/// `ArmorStand.interact`'s choice of the slot a click acts on: with an item in hand, its own slot (`held_slot`) unless it is disabled;
/// with an empty hand, the clicked height, or the main hand when the clicked slot is disabled. `None`: the click changes nothing.
pub fn click_target(w: &World, e: Entity, held: bool, held_slot: Option<usize>, rel_y: f32) -> Option<usize> {
    let small = w.get::<Stand>(e).is_some_and(|s| s.flags & SMALL != 0);
    let filled = |s: usize| slot_of_index(s).is_some_and(|sl| crate::goals::equipped(w, e, sl) != 0);
    let slot = if held {
        let s = held_slot?;
        if is_disabled(w, e, s) {
            return None;
        }
        s
    } else {
        let s = clicked_slot(small, rel_y, filled);
        if is_disabled(w, e, s) { 0 } else { s }
    };
    let has = filled(slot);
    if swap_locked(w, e, slot, has) || (!held && !has) {
        return None;
    }
    Some(slot)
}

/// The loot table of a stand's death (`entities/armor_stand`): rolled only for a stand that is not small, when the mob drops are on.
pub const LOOT: &str = "minecraft:entities/armor_stand";
pub fn drops_loot(s: Stand, mob_drops: bool) -> bool {
    s.flags & SMALL == 0 && mob_drops
}

/// The tick of the last blow that only wobbled the stand (absent until one did: no byte for a stand that was never hit).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastHit(pub u64);

/// What a blow did: the stand breaks, it wobbles (`Wobble`: the hit sound), or nothing (the blow was refused before this).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blow {
    Break,
    Wobble,
}

/// `ArmorStand.hurtServer` for a blow: a creative player breaks it at once; otherwise the first blow wobbles it (and sets `LastHit`), and a
/// blow within five ticks of that one breaks it. `always_kills` breaks it at once, as `#always_kills_armor_stands` does.
pub fn blow(w: &mut World, e: Entity, now: u64, creative: bool, always_kills: bool) -> Blow {
    if creative || always_kills {
        return Blow::Break;
    }
    match w.get::<LastHit>(e).copied() {
        Some(LastHit(t)) if now - t <= 5 => Blow::Break,
        _ => {
            w.entity_mut(e).insert(LastHit(now));
            Blow::Wobble
        }
    }
}
