//! The cart variants (S58): a furnace's fuel, a hopper's pickup from its box, a TNT cart's fuse and blast, a generator's delay, a command
//! block's run, and the blows that wound and break a cart. The state is in `cart_state` and the physics in `minecart`; each effect leaves
//! as a `CartEvent` in `CartOut`, read by `EcsWorld::drain_cart`. A variant wakes on its wheel deadline or its marker (`Lit`, `CartHurt`).
use crate::cart_move::Rolling;
use crate::cart_state::{BlastPower, Cart, CartHurt, CommandBlock, Contents, Enabled, Spawner, DEFAULT_POWER};
use crate::cow::*;
use crate::explosion::ExplosionRng;
use crate::goals::sparse_any;
use crate::host_out_cart_variants::{CartEvent, CartOut};
use crate::item::{items_in, pickable, Item};
use crate::minecart::{Fuse, Furnace, Kind};
use crate::rules::item::max_stack;
use crate::rules::minecart::{blast_power, bounds_at, command_due, container_signal, fuse_step, hopper_box, hopper_enabled_on_activator, tnt_primed_by};
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use horizon_core::geometry::{Aabb, Axis, Vec3};
use horizon_core::nbt::{JavaString, Tag};

/// A furnace cart with fuel left (jar: `MinecartFurnace.tick` burns while `fuel` is positive): only a lit furnace is visited.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct Lit;

/// What a blow did to a cart: nothing, a wound, or a break (with its drop, as `AbstractMinecart.hurtServer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitResult {
    Ignored,
    Hurt,
    Broke { drop: bool },
}

/// What a use of a cart asks of the host: a fuel item to give (then `fuel_cart`), a container opened, a command screen, or nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseResult {
    Pass,
    Fuel,
    Open,
    Screen,
}

/// A coal adds 3600 ticks of fuel, and not past 32000 (jar: `MinecartFurnace.addFuel`).
const FUEL_TICKS: i32 = 3600;
const FUEL_MAX: i32 = 32000;
/// A hopper cart tries once a tick while enabled (jar: `MinecartHopper.tick` calls `tryConsumeItems` with no cooldown, one item a tick).
const HOPPER_PERIOD: i32 = 1;
/// A blow sets the hurt time to 10 and adds 10 times its amount to the damage; a damage past 40 breaks it (jar: `VehicleEntity.hurtServer`).
const HURT_TIME: i32 = 10;
const HURT_DAMAGE: f32 = 10.0;
const BREAK_DAMAGE: f32 = 40.0;
/// A generator cart's spawner delay when its `Delay` key is absent (jar: `BaseSpawner.load`, `getShortOr("Delay", 20)`).
const DEFAULT_SPAWN_DELAY: i32 = 20;

pub fn register(s: &mut bevy_ecs::schedule::Schedule) {
    s.add_systems(furnace_step.run_if(|w: &World| sparse_any::<Lit>(w)));
    s.add_systems(hurt_step.run_if(|w: &World| sparse_any::<CartHurt>(w)));
}

fn out(w: &mut World) -> &mut Vec<CartEvent> {
    &mut w.get_resource_or_insert_with(CartOut::default).into_inner().0
}

/// Arms `f` on the wheel for `e` unless an entry of it is already there (an activation may come every tick).
fn arm(w: &mut World, e: Entity, ticks: i32, f: fn(&mut World, Entity)) {
    if w.resource::<Wheel>().owned_by(e) == 0 {
        after(w, e, ticks, f);
    }
}

/// The lit furnaces burn one tick of fuel; at zero the push goes and the furnace is no longer lit.
fn furnace_step(w: &mut World) {
    let lit: Vec<Entity> = crate::queries::entities::<With<Lit>>(w);
    for e in lit {
        let Some(mut f) = w.get_mut::<Furnace>(e) else { continue };
        f.fuel = (f.fuel - 1).max(0);
        let spent = f.fuel == 0;
        if spent {
            f.push = Vec3::ZERO;
        }
        drop(f);
        if spent {
            w.entity_mut(e).remove::<Lit>();
        }
    }
}

/// The hurt time counts down and the damage eases by one a tick; a cart with neither left is no longer hurt.
fn hurt_step(w: &mut World) {
    let hurt: Vec<(Entity, CartHurt)> = crate::queries::with::<(Entity, &'static CartHurt), (), _>(w, |q, w| q.iter(w).map(|(e, h)| (e, *h)).collect());
    for (e, h) in hurt {
        let (time, damage) = ((h.time - 1).max(0), (h.damage - 1.0).max(0.0));
        if time == 0 && damage == 0.0 {
            w.entity_mut(e).remove::<CartHurt>();
        } else {
            w.entity_mut(e).insert(CartHurt { time, dir: h.dir, damage });
        }
    }
}

/// A blow on a cart (`hurtServer`). Creative breaks it with no drop; otherwise it is hurt, and breaks with its drop past the threshold.
/// `attacker` is the credit, carried by the `Broke` event. A blow wakes the cart (`Rolling`).
pub fn hit_cart(w: &mut World, e: Entity, amount: f32, creative: bool, attacker: Option<Who>) -> HitResult {
    if w.get::<Cart>(e).is_none() {
        return HitResult::Ignored;
    }
    if creative {
        return break_cart(w, e, false, attacker);
    }
    let h = accrue(w, e, amount);
    if h.damage > BREAK_DAMAGE {
        return break_cart(w, e, true, attacker);
    }
    w.entity_mut(e).insert((h, Rolling));
    HitResult::Hurt
}

/// The hurt state after a blow of `amount` (`VehicleEntity.hurtServer`: ten ticks, the direction flipped, ten times the amount).
fn accrue(w: &World, e: Entity, amount: f32) -> CartHurt {
    let mut h = w.get::<CartHurt>(e).copied().unwrap_or_default();
    h.time = HURT_TIME;
    h.dir = -h.dir;
    h.damage += amount * HURT_DAMAGE;
    h
}

/// A blast's blow on a cart (`MinecartTNT.destroy` with an explosion source): past the threshold a TNT cart that is not primed primes
/// with the short fuse `fuse(w)` draws instead of breaking, and one already primed stays; any other cart breaks with its drop.
pub fn hit_cart_blast(w: &mut World, e: Entity, amount: f32, attacker: Option<Who>, fuse: impl FnOnce(&mut World) -> i32) -> HitResult {
    if w.get::<Cart>(e).is_none() {
        return HitResult::Ignored;
    }
    let h = accrue(w, e, amount);
    if h.damage <= BREAK_DAMAGE {
        w.entity_mut(e).insert((h, Rolling));
        return HitResult::Hurt;
    }
    let Some(now) = w.get::<Fuse>(e).map(|f| f.0) else { return break_cart(w, e, true, attacker) };
    if tnt_primed_by(true, now).is_some() {
        let ticks = fuse(w);
        prime_cart(w, e, ticks);
    }
    w.entity_mut(e).insert((h, Rolling));
    HitResult::Hurt
}

/// A blow of fire or lava on a cart (`MinecartTNT.shouldSourceDestroy`: an `IS_FIRE` source destroys a TNT cart whatever the damage, which
/// primes the short fuse `fuse(w)` draws if it is unprimed). Any other cart is hurt as by `hit_cart`.
pub fn hit_cart_fire(w: &mut World, e: Entity, amount: f32, fuse: impl FnOnce(&mut World) -> i32) -> HitResult {
    let Some(now) = w.get::<Fuse>(e).map(|f| f.0) else { return hit_cart(w, e, amount, false, None) };
    if w.get::<Cart>(e).is_none() {
        return HitResult::Ignored;
    }
    let h = accrue(w, e, amount);
    if tnt_primed_by(true, now).is_some() {
        let ticks = fuse(w);
        prime_cart(w, e, ticks);
    }
    w.entity_mut(e).insert((h, Rolling));
    HitResult::Hurt
}

fn break_cart(w: &mut World, e: Entity, drop: bool, attacker: Option<Who>) -> HitResult {
    let at = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let kind = w.get::<Kind>(e).map_or(0, |k| crate::species::minecart::GENRES.iter().position(|(_, g)| g == k).unwrap_or(0) as u8);
    w.entity_mut(e).despawn();
    out(w).push(CartEvent::Broke { entity: e, at: [at.x, at.y, at.z], kind, drop, attacker });
    HitResult::Broke { drop }
}

/// A use of a cart: a furnace asks for fuel, a chest or a hopper opens (`OpenContents`), a command block shows its screen. The hand is the host's.
pub fn use_cart(w: &mut World, e: Entity, player: u32, _hand: u8) -> UseResult {
    match w.get::<Kind>(e).copied() {
        Some(Kind::Furnace) => UseResult::Fuel,
        Some(Kind::Chest | Kind::Hopper) => {
            out(w).push(CartEvent::OpenContents { entity: e, player });
            UseResult::Open
        }
        Some(Kind::CommandBlock) => UseResult::Screen,
        _ => UseResult::Pass,
    }
}

/// A fuel item in a furnace cart: the fuel is added and the furnace lit. False when the cart is no furnace or the fuel would pass the cap.
pub fn fuel_cart(w: &mut World, e: Entity) -> bool {
    let Some(mut f) = w.get_mut::<Furnace>(e) else { return false };
    if f.fuel + FUEL_TICKS > FUEL_MAX {
        return false;
    }
    f.fuel += FUEL_TICKS;
    drop(f);
    w.entity_mut(e).insert(Lit);
    true
}

/// A TNT cart lit by a fire, a blow or a powered rail: it primes once (`tnt_primed_by`), its blast power is `power` when given. Its fuse
/// counts down on the wheel from the next tick.
pub fn ignite_cart(w: &mut World, e: Entity, power: Option<f32>) {
    let Some(fuse) = w.get::<Fuse>(e).map(|f| f.0) else { return };
    let Some(lit) = tnt_primed_by(true, fuse) else { return };
    prime_cart(w, e, lit);
    if let Some(p) = power {
        let mut bp = w.get::<BlastPower>(e).copied().unwrap_or(DEFAULT_POWER);
        bp.base = p;
        w.entity_mut(e).insert(bp);
    }
}

/// `MinecartTNT.primeFuse`: the cart counts down from `ticks` on the wheel from the next tick, and the host is told (sound, event).
fn prime_cart(w: &mut World, e: Entity, ticks: i32) {
    w.entity_mut(e).insert(Fuse(ticks));
    out(w).push(CartEvent::Fuse { entity: e });
    after(w, e, 1, fuse_fire);
}

/// One tick of a primed cart's fuse; at its end the cart explodes (`MinecartTNT.tick`).
fn fuse_fire(w: &mut World, e: Entity) {
    let Some(fuse) = w.get::<Fuse>(e).map(|f| f.0) else { return };
    if fuse < 0 {
        return;
    }
    let (next, explodes, _) = fuse_step(fuse);
    if explodes {
        explode_cart(w, e);
    } else {
        w.entity_mut(e).insert(Fuse(next));
        after(w, e, 1, fuse_fire);
    }
}

/// The blast of a cart: the base power plus its speed factor times the gain of `blast_power` at the cart's speed and a draw of the explosion's rng.
fn explode_cart(w: &mut World, e: Entity) {
    let at = w.get::<Pos>(e).map_or(Vec3::ZERO, |p| p.0);
    let speed = w.get::<Vel>(e).map_or(0.0, |v| v.0.length_squared());
    let r = w.resource_mut::<ExplosionRng>().0.next_f64();
    let bp = w.get::<BlastPower>(e).copied().unwrap_or(DEFAULT_POWER);
    let power = bp.base + bp.speed_factor * (blast_power(speed, r) - DEFAULT_POWER.base);
    w.entity_mut(e).despawn();
    out(w).push(CartEvent::Explode { at: [at.x, at.y, at.z], power, entity: e });
}

/// The area a hopper cart sucks from: its own box widened 0.25 sideways (`hopper_box`).
fn hopper_area(p: Vec3) -> Aabb {
    let b = bounds_at(p);
    let lo = Vec3::new(b.min(Axis::X), b.min(Axis::Y), b.min(Axis::Z));
    let hi = Vec3::new(b.max(Axis::X), b.max(Axis::Y), b.max(Axis::Z));
    let (lo, hi) = hopper_box(lo, hi);
    Aabb::new(Vec3::new(lo[0], lo[1], lo[2]), Vec3::new(hi[0], hi[1], hi[2]))
}

/// `Container.addItem` of a stack into the slots (jar: `HopperBlockEntity.addItem`): the compatible stacks first, then the empty slots, each
/// up to `max`. What does not fit is returned.
fn put_in_slots(slots: &mut [Option<ItemStack>], mut stack: ItemStack, max: u32) -> ItemStack {
    for s in slots.iter_mut().flatten() {
        if stack.count == 0 {
            break;
        }
        if s.count < max && s.same_item_same_components(&stack) {
            let n = (max - s.count).min(stack.count);
            s.count += n;
            stack.count -= n;
        }
    }
    for slot in slots.iter_mut().filter(|s| s.is_none()) {
        if stack.count == 0 {
            break;
        }
        let n = stack.count.min(max);
        let mut part = stack.clone();
        part.count = n;
        *slot = Some(part);
        stack.count -= n;
    }
    stack
}

/// A hopper cart's tick (`MinecartHopper.tryConsumeItems`): the ready items of its box go into its slots in order; the first that goes in
/// whole ends the tick (`suckInItems` returns true). It tries again every tick while enabled.
fn hopper_fire(w: &mut World, e: Entity) {
    if !w.get::<Enabled>(e).is_some_and(|en| en.0) {
        return;
    }
    let Some(p) = w.get::<Pos>(e).map(|p| p.0) else { return };
    let Some(mw) = w.get_resource::<Level>().map(|l| l.mw.0) else { return };
    let ready: Vec<Entity> = items_in(w, hopper_area(p)).into_iter().filter(|&i| pickable(w, i)).collect();
    let mut moved = false;
    for item in ready {
        let Some(stack) = w.get::<Item>(item).map(|i| i.stack.clone()) else { continue };
        let max = max_stack(mw, &stack);
        let left = {
            let Some(mut k) = w.get_mut::<Contents>(e) else { break };
            put_in_slots(&mut k.slots, stack.clone(), max)
        };
        moved |= left.count < stack.count;
        if left.count == 0 {
            w.entity_mut(item).despawn();
            out(w).push(CartEvent::HopperPulled { entity: e, item });
            break;
        }
        if left.count < stack.count {
            if let Some(mut it) = w.get_mut::<Item>(item) {
                it.stack = left;
            }
        }
    }
    if moved {
        signal_cart(w, e);
    }
    after(w, e, HOPPER_PERIOD, hopper_fire);
}

/// The redstone level of a container's slots (`getRedstoneSignalFromContainer`): each stack's fraction of its largest size (`max`), summed
/// over the slots by `container_signal`.
fn cart_signal_level(slots: &[Option<ItemStack>], max: impl Fn(&ItemStack) -> u32) -> i32 {
    let fractions: Vec<f32> = slots.iter().flatten().map(|s| s.count as f32 / max(s) as f32).collect();
    container_signal(&fractions, slots.len())
}

/// A chest or a hopper cart's signal after its contents changed: sent to the host as `Signal`. Nothing without the level's items.
pub fn signal_cart(w: &mut World, e: Entity) {
    let Some(mw) = w.get_resource::<Level>().map(|l| l.mw.0) else { return };
    let Some(k) = w.get::<Contents>(e) else { return };
    let level = cart_signal_level(&k.slots, |s| max_stack(mw, s));
    out(w).push(CartEvent::Signal { entity: e, level });
}

/// The delay of a generator cart's spawner, from its `Delay` key (a short; the jar's default 20 when it is absent).
fn spawner_delay(w: &World, e: Entity) -> i32 {
    match w.get::<Spawner>(e).and_then(|s| s.data.get(&JavaString::from("Delay"))) {
        Some(Tag::Short(d)) => i32::from(*d).max(1),
        _ => DEFAULT_SPAWN_DELAY,
    }
}

/// A generator cart's spawner delay ends: the host is told (`SpawnerTick`) and the delay runs again.
fn spawner_fire(w: &mut World, e: Entity) {
    out(w).push(CartEvent::SpawnerTick { entity: e });
    let d = spawner_delay(w, e);
    after(w, e, d, spawner_fire);
}

/// A command block's run: its command goes to the host (`CommandRun`), an empty command does nothing.
fn command_fire(w: &mut World, e: Entity) {
    let Some(command) = w.get::<CommandBlock>(e).map(|c| c.command.clone()) else { return };
    if !command.is_empty() {
        out(w).push(CartEvent::CommandRun { entity: e, command });
    }
}

/// Starts the variant's own deadline after its spawn: a hopper that is enabled, a generator with its delay.
pub fn start_cart(w: &mut World, e: Entity) {
    match w.get::<Kind>(e).copied() {
        Some(Kind::Hopper) if w.get::<Enabled>(e).is_some_and(|en| en.0) => arm(w, e, HOPPER_PERIOD, hopper_fire),
        Some(Kind::Spawner) => {
            let d = spawner_delay(w, e);
            arm(w, e, d, spawner_fire);
        }
        _ => {}
    }
}

/// A powered rail under a cart (`activateMinecart`, by variant): a hopper is enabled by an unpowered one, a TNT cart is lit by a powered
/// one, a command block runs by a powered one.
pub fn activate_cart(w: &mut World, e: Entity, powered: bool) {
    match w.get::<Kind>(e).copied() {
        Some(Kind::Hopper) => {
            let on = hopper_enabled_on_activator(powered);
            if let Some(mut en) = w.get_mut::<Enabled>(e) {
                en.0 = on;
            }
            if on {
                arm(w, e, HOPPER_PERIOD, hopper_fire);
            }
        }
        Some(Kind::Tnt) if powered => ignite_cart(w, e, None),
        Some(Kind::CommandBlock) if powered => {
            let now = now_of(w, e) as i32;
            let Some(mut c) = w.get_mut::<CommandBlock>(e) else { return };
            if !c.last_activated.is_none_or(|last| command_due(powered, now, last)) {
                return;
            }
            c.last_activated = Some(now);
            drop(c);
            arm(w, e, 0, command_fire);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cart_state;
    use crate::cow::Herd;
    use crate::minecart::FUSE;
    use myriade_engine_lib::items::ItemStack;

    fn world() -> Herd {
        let mut h = Herd::new(0, 1.0, true, false, 1);
        crate::explosion::insert_resources(&mut h.world);
        h
    }

    #[test]
    fn a_primed_cart_explodes_at_rest_with_the_base_power_after_its_fuse() {
        let mut h = world();
        let e = cart_state::spawn(&mut h.world, Kind::Tnt, [0.5, 1.0, 0.5], 0.0);
        ignite_cart(&mut h.world, e, None);
        for _ in 0..=FUSE {
            fuse_fire(&mut h.world, e);
        }
        let events = &h.world.resource::<CartOut>().0;
        assert!(events.contains(&CartEvent::Explode { at: [0.5, 1.0, 0.5], power: 4.0, entity: e }));
        assert!(h.world.get::<Cart>(e).is_none());
    }

    #[test]
    fn a_fire_or_a_powered_rail_primes_a_cart_only_once() {
        let mut h = world();
        let e = cart_state::spawn(&mut h.world, Kind::Tnt, [0.5, 1.0, 0.5], 0.0);
        ignite_cart(&mut h.world, e, None);
        activate_cart(&mut h.world, e, true);
        assert_eq!(h.world.get::<Fuse>(e).map(|f| f.0), Some(FUSE));
        let fuses = h.world.resource::<CartOut>().0.iter().filter(|ev| matches!(ev, CartEvent::Fuse { .. })).count();
        assert_eq!(fuses, 1);
    }

    #[test]
    fn a_furnace_without_fuel_is_not_lit_and_stops_when_its_fuel_is_spent() {
        let mut h = world();
        let e = cart_state::spawn(&mut h.world, Kind::Furnace, [0.5, 1.0, 0.5], 0.0);
        assert!(h.world.get::<Lit>(e).is_none());
        assert!(fuel_cart(&mut h.world, e));
        assert_eq!(h.world.get::<Furnace>(e).map(|f| f.fuel), Some(3600));
        for _ in 0..3600 {
            furnace_step(&mut h.world);
        }
        assert_eq!(h.world.get::<Furnace>(e).map(|f| f.fuel), Some(0));
        assert!(h.world.get::<Lit>(e).is_none());
    }

    #[test]
    fn a_blow_wounds_a_cart_and_a_creative_one_breaks_it_with_no_drop() {
        let mut h = world();
        let e = cart_state::spawn(&mut h.world, Kind::Chest, [0.5, 1.0, 0.5], 0.0);
        assert_eq!(hit_cart(&mut h.world, e, 1.0, false, None), HitResult::Hurt);
        let hurt = h.world.get::<CartHurt>(e).copied().expect("hurt");
        assert_eq!((hurt.time, hurt.damage), (10, 10.0));
        assert_eq!(hit_cart(&mut h.world, e, 1.0, true, None), HitResult::Broke { drop: false });
        assert!(h.world.get::<Cart>(e).is_none());
    }

    #[test]
    fn a_hopper_fills_the_stacks_it_has_before_the_empty_slots() {
        let mut slots = vec![Some(ItemStack::new("minecraft:stone", 60)), None, None];
        let left = put_in_slots(&mut slots, ItemStack::new("minecraft:stone", 10), 64);
        assert_eq!(left.count, 0);
        assert_eq!(slots.iter().map(|s| s.as_ref().map(|s| s.count)).collect::<Vec<_>>(), vec![Some(64), Some(6), None]);
    }

    #[test]
    fn a_full_chest_signals_15_and_a_half_full_last_slot_signals_14() {
        let full = vec![Some(ItemStack::new("minecraft:stone", 64)); 27];
        assert_eq!(cart_signal_level(&full, |_| 64), 15);
        let mut last_half = full.clone();
        last_half[26] = Some(ItemStack::new("minecraft:stone", 32));
        assert_eq!(cart_signal_level(&last_half, |_| 64), 14);
    }
}
