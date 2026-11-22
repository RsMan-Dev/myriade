//! What an arrow does after its flight: a player asks for a stuck arrow (`AbstractArrow.playerTouch`, `tryPickup`), its hit gives the
//! fire of Flame (`igniteForSeconds(5)`), the Punch push (`doKnockback`), the hit's sound and the potion effects of its contents
//! (`doPostHurtEffects`), a piercing arrow goes on through the creatures it has met (`PierceLevel`), a breeze turns it back
//! (`ProjectileDeflection.REVERSE`), and the mod hooks see its impact. The flight itself is `projectile.rs`; a stuck arrow sleeps there.
use crate::cow::{set_on_fire, Pos, Resist, TypeName, Vel, Yaw};
use crate::flat::Rng;
use crate::goals::Online;
use crate::host_out_arrows::{ArrowEvent, ArrowOut};
use crate::hostile::PlayerState;
use crate::projectile::{Flight, Flying, Order, Projectile, Stuck, StuckIndex};
use crate::rules::push_on_touch::knockback_factor;
use crate::targeting::{player_box, Who};
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::prelude::*;
use bevy_ecs::world::DeferredWorld;
use horizon_client_lib::arrows::Pickup;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::{Aabb, Vec3};

/// The end effects of a shot arrow, computed by the host at the launch (the enchantments, the potion of its item and the spectral glow):
/// the fire in ticks (0 for none), the Punch push, the Piercing level, and the potion effects by registry name, amplifier and ticks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Def {
    pub fire: u64,
    pub punch: f32,
    pub pierce: u8,
    pub effects: Vec<(&'static str, i32, i32)>,
}

/// A shot arrow: who may take it (`pickup`), the stack a pickup gives, what its hit does, and its hit sound (`minecraft:entity.arrow.hit`,
/// or the crossbow's, set by the host).
#[derive(Component, Clone, Debug)]
pub struct ArrowShot {
    pub pickup: Pickup,
    pub stack: ItemStack,
    pub fx: Def,
    pub sound: &'static str,
}

/// The item a thrown projectile carries (`ThrowableItemProjectile.Item`: a potion, an egg, a pearl), sparse, saved as `Item`.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct ThrownItem(pub ItemStack);

/// The kinds of the creatures this arrow killed (`piercedAndKilledEntities`, kept as kinds). On the arrow's removal a player shooter gets
/// an `Killed` event with them (`KILLED_BY_ARROW`); sparse, so an arrow with no kill pays nothing.
#[derive(Component, Clone, Debug, Default)]
#[component(storage = "SparseSet", on_remove = killed_end)]
pub struct Kills(pub Vec<&'static str>);

/// The weapon the arrow was fired from (`AbstractArrow.firedFromWeapon`, saved as `weapon`), when the shot knows it: a `Killed` event
/// carries it. Sparse: an arrow shot without a weapon has none.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct FiredFrom(pub ItemStack);

fn killed_end(mut world: DeferredWorld, ctx: HookContext) {
    let Some(owner @ Who::Player(_)) = world.get::<Projectile>(ctx.entity).and_then(|p| p.owner) else { return };
    let kinds = world.get::<Kills>(ctx.entity).map(|k| k.0.clone()).unwrap_or_default();
    let weapon = world.get::<FiredFrom>(ctx.entity).map(|f| f.0.clone());
    if !kinds.is_empty() {
        world.commands().queue(move |w: &mut World| emit(w, ArrowEvent::Killed { owner, kinds, weapon }));
    }
}

/// A piercing arrow's level and the creatures it has gone through (`piercingIgnoreEntityIds`). Only a piercing arrow has it.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct PierceLevel {
    pub level: u8,
    pub hit: Vec<Entity>,
}

/// The previous yaw of a turned arrow (`yRotO`), turned with its yaw by a deflection.
#[derive(Component, Clone, Copy, Debug)]
pub struct YawOld(pub f32);

/// The randoms of the arrows (the hit's pitch and the deflection's turn).
#[derive(Resource)]
pub struct ArrowRng(pub Rng);

impl Default for ArrowRng {
    fn default() -> Self {
        Self(Rng(0x9E37_79B9_7F4A_7C15))
    }
}

/// Puts the shot data on the arrow `e`, launched by `projectile::launch`; a piercing one gets its `PierceLevel`, a shot with a known
/// weapon its `FiredFrom`.
pub fn attach(w: &mut World, e: Entity, shot: ArrowShot, weapon: Option<ItemStack>) {
    if shot.fx.pierce > 0 {
        w.entity_mut(e).insert(PierceLevel { level: shot.fx.pierce, hit: Vec::new() });
    }
    if let Some(weapon) = weapon {
        w.entity_mut(e).insert(FiredFrom(weapon));
    }
    w.entity_mut(e).insert(shot);
}

/// The player `by` asks for the stuck arrow `e`: a `PickedUp` event carries the stack (count 0 for a creative take, which gives no
/// item). False, with no event, when the rule refuses: a flying arrow, a non-player `by`, a disallowed pickup, a survival player on a
/// creative-only arrow, or a player whose box does not touch the arrow's (the reach of `playerTouch`). The arrow stays until `taken`.
pub fn pickup(w: &mut World, arrow: Entity, by: Who) -> bool {
    let Who::Player(player) = by else { return false };
    let Some(shot) = w.get::<ArrowShot>(arrow) else { return false };
    let (rule, stack) = (shot.pickup, shot.stack.clone());
    if w.get::<Stuck>(arrow).is_none() {
        return false;
    }
    let stack = match rule {
        Pickup::Disallowed => return false,
        Pickup::Allowed => stack,
        Pickup::CreativeOnly if w.resource::<PlayerState>().creative(player) => ItemStack { count: 0, ..stack },
        Pickup::CreativeOnly => return false,
    };
    let Some(pl) = w.resource::<Online>().get(player) else { return false };
    let (Some(at), Some(size)) = (w.get::<Pos>(arrow).map(|p| p.0), w.get::<Flight>(arrow).map(|f| f.size)) else { return false };
    if !arrow_box(at, size).intersects(player_box(&pl)) {
        return false;
    }
    emit(w, ArrowEvent::PickedUp { entity: arrow, by, stack });
    true
}

/// The host's answer to a pickup: `took` removes the arrow from its block; otherwise (a full inventory) it stays stuck.
pub fn taken(w: &mut World, arrow: Entity, took: bool) {
    if !took {
        return;
    }
    if let Some(stuck) = w.get::<Stuck>(arrow).copied() {
        if let Some(v) = w.resource_mut::<StuckIndex>().at.get_mut(&stuck.block) {
            v.retain(|&o| o != arrow);
        }
    }
    w.despawn(arrow);
}

/// `AbstractArrow.onHitEntity` on a piercing arrow: the creature joins its list while fewer than level + 1 are in it. True when the
/// arrow goes on past it; false when it is past its level (the arrow ends there, before any blow). `projectile.rs` calls it.
pub fn take_pierce(p: &mut PierceLevel, victim: Entity) -> bool {
    if p.hit.len() > usize::from(p.level) {
        return false;
    }
    p.hit.push(victim);
    true
}

/// The end of the flight of the shot arrow `e` where it hits `victim` (None: a block at `block`, through `face`) for `damage`; `killed`
/// when the blow kills a creature (its health read before the blow). The blow on a creature is dealt by `projectile.rs` (the same `hurt`
/// as the other projectiles); here the fire, the Punch push and the kind a kill adds. A player's blow and knockback are left to the host.
pub fn on_hit(w: &mut World, e: Entity, victim: Option<Who>, damage: f32, killed: bool, block: Option<[i32; 3]>, face: Option<[i32; 3]>) {
    let at = w.get::<Pos>(e).map_or([0.0; 3], |p| [p.0.x, p.0.y, p.0.z]);
    let kind = w.get::<Flight>(e).map_or("", |f| f.id);
    let owner = w.get::<Projectile>(e).and_then(|p| p.owner);
    let shot = w.get::<ArrowShot>(e).map(|s| (s.fx.clone(), s.sound));
    let (fx, sound) = shot.unwrap_or_else(|| (Def::default(), "minecraft:entity.arrow.hit"));
    let pitch = 1.2 / (0.2 * unit(&mut w.get_resource_or_insert_with(ArrowRng::default).0) + 0.9);
    // A player's push is left to the host: its factor is 1 here (no attribute is kept for a player).
    let mut strength = if matches!(victim, Some(Who::Player(_))) { f64::from(fx.punch) * 0.6 } else { 0.0 };
    if let Some(Who::Entity(t)) = victim {
        if fx.fire > 0 {
            set_on_fire(w, t, fx.fire);
        }
        if crate::inside_effects::fire_left(w, e) > 0 {
            set_on_fire(w, t, crate::inside_effects::ARROW_IGNITES);
        }
        strength = knock(w, t, e, fx.punch);
        if killed {
            let name = w.get::<TypeName>(t).map_or("", |n| n.0);
            match w.get_mut::<Kills>(e) {
                Some(mut k) => k.0.push(name),
                None => {
                    w.entity_mut(e).insert(Kills(vec![name]));
                }
            }
        }
    }
    let (dx, dz) = flat_dir(w, e).unwrap_or((0.0, 0.0));
    emit(w, ArrowEvent::ModImpact { entity: e, kind, at, target: victim, owner, block, face });
    emit(w, ArrowEvent::Hit { entity: e, victim, owner, at, damage, damage_type: crate::species::lists::ARROW, knockback: strength as f32, dx, dz, fire: fx.fire, sound, pitch });
    let Some(target) = victim else { return };
    let mut effects = fx.effects;
    // `SpectralArrow.onHitEntity`: the creature glows for its duration (200 ticks by default).
    if kind == crate::species::lists::SPECTRAL_ARROW {
        effects.push(("minecraft:glowing", 0, 200));
    }
    for (effect, amplifier, ticks) in effects {
        emit(w, ArrowEvent::PotionEffect { target, effect, amplifier, ticks });
    }
}

/// `AbstractArrow.doKnockback`: the creature is pushed along the arrow's flat direction by `punch x 0.6` times its knockback factor
/// (`1 - resistance` from its `Resist` attribute, 0 when it has none), and 0.1 up. Nothing when that strength is 0.
fn knock(w: &mut World, target: Entity, arrow: Entity, punch: f32) -> f64 {
    let resist = w.get::<Resist>(target).map_or(0.0, |r| r.0);
    let strength = f64::from(punch) * 0.6 * knockback_factor(resist);
    let Some((dx, dz)) = flat_dir(w, arrow) else { return strength };
    if strength > 0.0 {
        if let Some(mut t) = w.get_mut::<Vel>(target) {
            t.0 = Vec3::new(t.0.x + dx * strength, t.0.y + 0.1, t.0.z + dz * strength);
        }
    }
    strength
}

/// The unit flat direction of the arrow `e`'s velocity; None without a flat velocity.
fn flat_dir(w: &World, e: Entity) -> Option<(f64, f64)> {
    let v = w.get::<Vel>(e)?.0;
    let len = (v.x * v.x + v.z * v.z).sqrt();
    (len > 0.0).then_some((v.x / len, v.z / len))
}

/// A breeze turns the flying arrow `e` back (`ProjectileDeflection.REVERSE`, then `Projectile.setOwner` to the deflector `by`): its velocity
/// times -0.5 on each axis, its yaw and its previous yaw (`yRotO`) turned by 170 + 20 x a random. The direction `_dir` is not read by
/// REVERSE. A stuck arrow is not turned; false then, and for an entity that is not a projectile. The server jar holds no damage for a
/// deflection (`Projectile.onDeflection` is empty).
pub fn deflect(w: &mut World, e: Entity, by: Option<Who>, dir: Vec3) -> bool {
    turn(w, e, Turn::Reverse, by, dir, false)
}

/// `ProjectileDeflection` of the jar: `None` changes nothing, `Reverse` turns the arrow back, `AimDeflect` and `MomentumDeflect` set
/// its velocity to `dir` (the host picks `None` when there is no deflector; it gives `dir` as the deflector's look angle, or its known speed, scaled).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Turn {
    None,
    Reverse,
    AimDeflect,
    MomentumDeflect,
}

/// The deflection `turn` of the projectile `e`, owned by `by` afterwards (`Projectile.deflect` sets the owner whatever the turn). `by_attack`
/// is the jar's `onDeflection` flag: a hurting projectile (fireball) then takes its push back to 0.1, else halves it (`HurtAccel`). False
/// when `e` is stuck or has no velocity.
pub fn turn(w: &mut World, e: Entity, turn: Turn, by: Option<Who>, dir: Vec3, by_attack: bool) -> bool {
    if w.get::<Stuck>(e).is_some() {
        return false;
    }
    let Some(v) = w.get::<Vel>(e).map(|v| v.0) else { return false };
    match turn {
        Turn::None => {}
        Turn::Reverse => {
            let k = unit(&mut w.get_resource_or_insert_with(ArrowRng::default).0);
            let angle = 170.0 + 20.0 * k;
            let yaw = w.get::<Yaw>(e).map_or(0.0, |y| y.0);
            let old = w.get::<YawOld>(e).map_or(yaw, |o| o.0);
            w.entity_mut(e).insert((Vel(v.scale(-0.5)), Yaw(yaw + angle), YawOld(old + angle)));
        }
        // The host picks `Turn::None` when there is no deflector; `by` is the NEW owner, not the deflector.
        Turn::AimDeflect | Turn::MomentumDeflect => {
            w.entity_mut(e).insert(Vel(dir));
        }
    }
    if let Some((order, accel)) = w.get::<Flight>(e).map(|f| (f.order, f.accel))
        && order == Order::Hurting
    {
        let before = w.get::<HurtAccel>(e).map_or(accel, |a| a.0);
        w.entity_mut(e).insert(HurtAccel(if by_attack { 0.1 } else { before * 0.5 }));
    }
    if let Some(mut p) = w.get_mut::<Projectile>(e) {
        p.owner = by;
    }
    true
}

/// `AbstractArrow.onHitEntity`: a critical arrow adds `nextInt(damage / 2 + 2)` to its blow.
pub fn critical_bonus(damage: f32, rng: &mut ArrowRng) -> f32 {
    let whole = damage as i32;
    let n = whole / 2 + 2;
    let draw = ((u64::from(rng.0.next()) * n as u64) >> 32) as i32;
    (whole + draw) as f32
}

/// The mod's gate on the impact of `e` (on `victim`, or the block at `block` through `face`, `at` where it is): the impact is announced
/// as `ModImpact`, and the arrow waits (`Suspended`, not flying) until `resolve`.
pub fn gate(w: &mut World, e: Entity, victim: Option<Who>, at: [f64; 3], block: Option<[i32; 3]>, face: Option<[i32; 3]>) {
    let kind = w.get::<Flight>(e).map_or("", |f| f.id);
    let owner = w.get::<Projectile>(e).and_then(|p| p.owner);
    emit(w, ArrowEvent::ModImpact { entity: e, kind, at, target: victim, owner, block, face });
    w.entity_mut(e).remove::<Flying>().insert(Suspended);
}

/// The mod's answer on the suspended impact of `e`: `cancel` despawns the arrow with no impact; else it flies again and its impact is
/// applied on the next tick (`GateOpen`). False when `e` is not suspended.
pub fn resolve(w: &mut World, e: Entity, cancel: bool) -> bool {
    if w.get::<Suspended>(e).is_none() {
        return false;
    }
    if cancel {
        w.despawn(e);
    } else {
        w.entity_mut(e).remove::<Suspended>().insert((Flying, GateOpen));
    }
    true
}

/// The mod's decision point on a projectile (`ModGate`): its impacts are announced and wait for `resolve`. Sparse, on the followed only.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct ModGate;

/// A gated projectile waiting for its mod's answer (`gate`, `resolve`).
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Suspended;

/// The gate let an impact through: the next tick applies it, the gate not asked again.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct GateOpen;

/// `AbstractHurtingProjectile.accelerationPower` after a deflection (`turn`); absent when the kind's own value applies.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct HurtAccel(pub f64);

/// `Projectile.noPhysics` of the jar (the shulker bullet's, true by default there). Stored; `fly` does not read it yet.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct NoPhysics(pub bool);

/// `AbstractArrow.baseDamage` (2.0 for an arrow), set by a mod; absent when the kind's own damage applies.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct BaseDamage(pub f64);

/// `AbstractArrow.critical`: a critical arrow hits harder (set by a mod); absent when false.
#[derive(Component, Clone, Copy, Debug)]
#[component(storage = "SparseSet")]
pub struct Critical(pub bool);

/// The arrow events since the last call, in order.
pub fn drain(w: &mut World) -> Vec<ArrowEvent> {
    w.get_resource_mut::<ArrowOut>().map_or_else(Vec::new, |mut o| std::mem::take(&mut o.0))
}

fn emit(w: &mut World, ev: ArrowEvent) {
    w.get_resource_or_insert_with(ArrowOut::default).0.push(ev);
}

/// A float in [0, 1) from 24 bits of a draw (the `RandomSource.nextFloat` shape).
fn unit(r: &mut Rng) -> f32 {
    (r.next() >> 8) as f32 / (1u32 << 24) as f32
}

/// The box of an arrow at `at` (the same as `projectile::bounds`: a square of side `size`, rising from its point).
/// The box of a projectile of `size` at `at`, the same as `projectile::bounds`.
pub fn arrow_box(at: Vec3, size: f64) -> Aabb {
    let w = size / 2.0;
    Aabb::new(Vec3::new(at.x - w, at.y, at.z - w), Vec3::new(at.x + w, at.y + size, at.z + w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goals::Player;

    fn world(creative: bool) -> World {
        let mut w = World::new();
        w.insert_resource(Online::default());
        w.insert_resource(PlayerState { creative: vec![false, creative], ..Default::default() });
        w.insert_resource(StuckIndex::default());
        w.resource_mut::<Online>().set(Player { id: 1, x: 0.0, y: 64.0, z: 0.0, ..Default::default() });
        w
    }

    fn shot(rule: Pickup) -> ArrowShot {
        ArrowShot { pickup: rule, stack: ItemStack { item: "minecraft:arrow".into(), count: 1, damage: 0, extra: None }, fx: Def::default(), sound: "minecraft:entity.arrow.hit" }
    }

    fn stuck(w: &mut World, rule: Pickup) -> Entity {
        let e = w.spawn((Pos(Vec3::new(0.0, 64.0, 0.0)), Flight::ARROW, Stuck { block: [0, 64, 0] }, shot(rule))).id();
        w.resource_mut::<StuckIndex>().at.entry([0, 64, 0]).or_default().push(e);
        e
    }

    fn picked(w: &mut World) -> Vec<ArrowEvent> {
        drain(w).into_iter().filter(|e| matches!(e, ArrowEvent::PickedUp { .. })).collect()
    }

    #[test]
    fn a_survival_player_gets_no_event_for_a_creative_only_arrow() {
        let mut w = world(false);
        let e = stuck(&mut w, Pickup::CreativeOnly);
        assert!(!pickup(&mut w, e, Who::Player(1)));
        assert!(picked(&mut w).is_empty());
    }

    #[test]
    fn a_creative_take_carries_a_stack_of_count_zero() {
        let mut w = world(true);
        let e = stuck(&mut w, Pickup::CreativeOnly);
        assert!(pickup(&mut w, e, Who::Player(1)));
        let events = picked(&mut w);
        assert!(matches!(&events[..], [ArrowEvent::PickedUp { by: Who::Player(1), stack, .. }] if stack.count == 0), "{events:?}");
    }

    #[test]
    fn a_full_inventory_leaves_the_arrow_stuck_and_a_taken_one_is_gone() {
        let mut w = world(false);
        let e = stuck(&mut w, Pickup::Allowed);
        assert!(pickup(&mut w, e, Who::Player(1)));
        taken(&mut w, e, false);
        assert!(w.get_entity(e).is_ok());
        taken(&mut w, e, true);
        assert!(w.get_entity(e).is_err());
        assert!(w.resource::<StuckIndex>().at.get(&[0, 64, 0]).is_some_and(|v| v.is_empty()));
    }

    #[test]
    fn a_flying_arrow_is_not_picked_up() {
        let mut w = world(false);
        let e = w.spawn((Pos(Vec3::new(0.0, 64.0, 0.0)), Flight::ARROW, shot(Pickup::Allowed))).id();
        assert!(!pickup(&mut w, e, Who::Player(1)));
    }

    #[test]
    fn a_piercing_arrow_goes_through_its_level_plus_one_creatures() {
        let mut w = world(false);
        let (a, b, c) = (w.spawn_empty().id(), w.spawn_empty().id(), w.spawn_empty().id());
        let mut p = PierceLevel { level: 1, hit: Vec::new() };
        assert!(take_pierce(&mut p, a));
        assert!(take_pierce(&mut p, b));
        assert!(!take_pierce(&mut p, c));
        assert_eq!(p.hit.len(), 2);
    }

    #[test]
    fn a_punch_pushes_the_creature_along_the_arrow_flat_velocity() {
        let mut w = world(false);
        let target = w.spawn(Vel(Vec3::ZERO)).id();
        let shot = ArrowShot { fx: Def { punch: 1.0, ..Def::default() }, ..shot(Pickup::Allowed) };
        let e = w.spawn((Pos(Vec3::new(0.0, 64.0, 0.0)), Flight::ARROW, Vel(Vec3::new(3.0, 1.0, 4.0)), shot)).id();
        on_hit(&mut w, e, Some(Who::Entity(target)), 2.0, false, None, None);
        let v = w.get::<Vel>(target).map(|v| v.0).unwrap_or(Vec3::ZERO);
        assert!((v.x - 0.36).abs() < 1e-9 && (v.z - 0.48).abs() < 1e-9 && (v.y - 0.1).abs() < 1e-9, "{v:?}");
    }

    #[test]
    fn a_hit_on_a_player_carries_its_victim_its_fire_and_its_potion() {
        let mut w = world(false);
        let fx = Def { fire: 100, punch: 1.0, pierce: 0, effects: vec![("minecraft:poison", 0, 100)] };
        let e = w.spawn((Pos(Vec3::new(0.0, 64.0, 0.0)), Flight::ARROW, Projectile { owner: Some(Who::Player(2)), age: 0, left_owner: false, expires: 0 }, ArrowShot { fx, ..shot(Pickup::Allowed) })).id();
        on_hit(&mut w, e, Some(Who::Player(1)), 4.0, false, None, None);
        let events = drain(&mut w);
        assert!(events.contains(&ArrowEvent::PotionEffect { target: Who::Player(1), effect: "minecraft:poison", amplifier: 0, ticks: 100 }), "{events:?}");
        assert!(events.iter().any(|ev| matches!(ev, ArrowEvent::Hit { victim: Some(Who::Player(1)), owner: Some(Who::Player(2)), fire: 100, damage_type: "minecraft:arrow", .. })), "{events:?}");
    }

    #[test]
    fn a_block_hit_gives_no_potion_and_no_victim() {
        let mut w = world(false);
        let fx = Def { effects: vec![("minecraft:poison", 0, 100)], ..Def::default() };
        let e = w.spawn((Pos(Vec3::new(0.0, 64.0, 0.0)), Flight::ARROW, ArrowShot { fx, ..shot(Pickup::Allowed) })).id();
        on_hit(&mut w, e, None, 0.0, false, Some([0, 64, 0]), Some([0, 1, 0]));
        let events = drain(&mut w);
        assert!(!events.iter().any(|ev| matches!(ev, ArrowEvent::PotionEffect { .. })));
        assert!(events.iter().any(|ev| matches!(ev, ArrowEvent::Hit { victim: None, .. })));
    }
}
