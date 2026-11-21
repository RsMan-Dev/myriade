//! The armor stand's own state (S6). Life, equipment, air and the box are the shared components (`Health`, `Equipment`, `Air`, `Body`,
//! `Bounds`). What is left is here: one byte of flags, the pose when it is not the default one, the disabled slots when one is.
//! A stand at rest has no `Moving`: nothing visits it, so a placed stand costs nothing per tick. The first four bits are the client
//! flags of the jar (`CLIENT_FLAG_*`); invisible and no gravity are entity flags of the jar, kept beside them.
use crate::cow::*;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;

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

/// `ArmorStand.ignoreExplosion`: an invisible stand is no target of a blast.
pub fn ignores_explosion(s: Stand) -> bool {
    s.flags & INVISIBLE != 0
}

/// `ArmorStand.brokenByPlayer`, the stand's own item: the stand is gone and one armor stand item pops where it stood (its equipment and its
/// loot are not dropped here). The pop's velocity is zero: the original's random pop is not ported.
pub fn break_drop(w: &mut World, e: Entity) -> Entity {
    let at = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    w.entity_mut(e).despawn();
    crate::item::drop_item(w, ItemStack::new(crate::species::armor_stand::ITEM, 1), at, Vec3::ZERO, 10)
}

/// A stand whose physics is live: `travel` moves it while it has `Moving`, `fall` takes this back once it rests (a SparseSet: no cost
/// while no stand falls).
#[derive(Component, Clone, Copy, Debug, Default)]
#[component(storage = "SparseSet")]
pub struct Unsettled;

/// Starts the physics of a stand that can move (`has_physics`): it gets `Moving` and `Unsettled`, and `travel` moves it from then on.
pub fn unsettle(w: &mut World, e: Entity) {
    if w.get::<Stand>(e).is_some_and(|s| has_physics(*s)) {
        w.entity_mut(e).insert((Moving, Unsettled));
        // The ground is read again by the move: `travel` takes a stand that still "stands" with no speed for one at rest, which a stand put in
        // the air or left over a block that went would be (`Entity.move` recomputes `onGround` from the collision of each tick).
        if let Some(mut c) = w.get_mut::<Contact>(e) {
            c.on_ground = false;
        }
    }
}

/// The end of a stand's motion: a stand that rests on the ground with no speed is settled, and waits on the block it stands on (a
/// change of that cell wakes it through `block_watch`, nothing scans the stands).
fn fall(w: &mut World) {
    if !crate::goals::sparse_any::<Unsettled>(w) {
        return;
    }
    let rest: Vec<(Entity, [i32; 3])> = w
        .query_filtered::<(Entity, &Pos, &Vel, &Contact, &Body), With<Unsettled>>()
        .iter(w)
        // At rest on the ground a living entity keeps the speed its gravity gives in the tick (`(0 - g) * 0.98`), which `travel` leaves as it is.
        .filter(|(_, _, v, c, b)| c.on_ground && v.0.x == 0.0 && v.0.y == (0.0 - b.gravity) * f64::from(0.98_f32) && v.0.z == 0.0)
        .map(|x| (x.0, [x.1 .0.x.floor() as i32, x.1 .0.y.floor() as i32 - 1, x.1 .0.z.floor() as i32]))
        .collect();
    for (e, support) in rest {
        w.entity_mut(e).remove::<Unsettled>();
        if let Some(mut bw) = w.get_resource_mut::<crate::block_watch::BlockWatch>() {
            bw.watch_cell(support, e);
        }
    }
}

/// The stands whose support changed (`block_watch` lists them) start their physics again.
fn wake(w: &mut World) {
    let woken = match w.get_resource_mut::<crate::block_watch::BlockWatch>() {
        Some(mut bw) if !bw.woken.is_empty() => std::mem::take(&mut bw.woken),
        _ => return,
    };
    for e in woken {
        unsettle(w, e);
    }
}

/// `ArmorStand.hurtServer` for the fire (`ignites`, `burns`): a fire cell lights a stand that is not burning for 100 ticks, and hurts a
/// burning one 0.15; the burn takes 4 above half a heart. An invisible or a marker stand takes none of it.
fn fire_intake(w: &mut World, e: Entity, cause: Cause) {
    let Some(flags) = w.get::<Stand>(e).map(|s| s.flags) else { return };
    if flags & (INVISIBLE | MARKER) != 0 {
        return;
    }
    if cause == Cause::InFire {
        if w.get::<Burning>(e).is_some() {
            cause_damage(w, e, 0.15);
        } else {
            set_on_fire(w, e, 100);
        }
    } else if w.get::<Health>(e).is_some_and(|h| h.0 > 0.5) {
        cause_damage(w, e, 4.0);
    }
}

/// `ArmorStand.causeDamage`: the health less `amount`; at 0.5 or less the stand breaks by anything.
fn cause_damage(w: &mut World, e: Entity, amount: f32) {
    let health = w.get::<Health>(e).map_or(0.0, |h| h.0) - amount;
    if health <= 0.5 {
        break_by_anything(w, e);
    } else if let Some(mut h) = w.get_mut::<Health>(e) {
        h.0 = health;
    }
}

/// `ArmorStand.brokenByAnything` then `kill`: the equipment pops above the stand's cell and the stand is gone; its own item does not drop.
/// The loot table is not rolled here.
fn break_by_anything(w: &mut World, e: Entity) {
    let at = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    for slot in (0..8).filter_map(slot_of_index) {
        let id = crate::equipment::take_slot(w, e, slot);
        if let Some(name) = crate::items::item_name(id).filter(|_| id != 0) {
            crate::item::drop_item(w, ItemStack::new(name, 1), at, Vec3::ZERO, 10);
        }
    }
    w.entity_mut(e).despawn();
}

/// The creation of an armor stand: the species' entity, its flags, the fire intake, and its physics when it can move.
pub fn spawn(w: &mut World, sp: &Species, at: Vec3, seed: u64, flags: u8) -> Entity {
    let e = spawn_species(w, sp, at.x, at.z, seed);
    // Not a `Mob`: its row of the table is a stand-in's. Its life is the default of `LivingEntity` (the attribute MAX_HEALTH, 20).
    w.entity_mut(e).insert((Stand { flags }, Intake(fire_intake), Health(20.0)));
    if let Some(mut p) = w.get_mut::<Pos>(e) {
        p.0 = at;
    }
    // The box follows the position the stand was put at: the move reads it, not the position.
    if let Some((width, height)) = w.get::<Body>(e).map(|b| (b.width, b.height)) {
        if let Some(mut b) = w.get_mut::<Bounds>(e) {
            b.0 = box_at(at, width, height);
        }
    }
    unsettle(w, e);
    e
}

/// The stands' physics ends after `travel` and before the index, as the falling blocks': `wake` starts them again, `fall` settles them.
pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems((wake.after(crate::block_watch::block_watch).before(travel), fall.after(travel).before(reindex)));
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
