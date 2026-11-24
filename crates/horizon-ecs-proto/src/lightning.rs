//! The lightning bolt (`LightningBolt`): an entity of its own, alive for a few ticks. Its script runs on its own deadlines (`cow::after`,
//! one per tick it lives), never by a scan of the world: a bolt that is spent costs nothing. The strike: the sounds, the fire
//! (`spawnFire`), the lightning rod and the copper at the struck block; then, for every living entity in its box (3 blocks around,
//! 3 down, 9 up), `thunderHit`: a pig or a villager converts, a mooshroom flips, a turtle dies, a creeper is charged, the rest
//! burn and take 5. Random draws come from the bolt's own generator, in the order of the jar.
//! Not here: the choice of the strike point (`ServerLevel.tickThunder`, lightning rods within 128 blocks, weather), the sky flash, the
//! hits on players, items, boats, carts, orbs and cushions, and the advancement trigger (see the lot's notes).
use crate::cow::{self, Body, Cause, DamageQueue, Health, Pos, SpeciesId, SpeciesTable};
use crate::goals::Events;
use bevy_ecs::prelude::*;
use horizon_core::geometry::Vec3;
use crate::rules::lightning::{fire_spots, strike_hit, under, MAX_HALF_WIDTH};
use horizon_core::random::{LegacyRandom, RandomSource};
use horizon_world::registry::BlockStateId;

/// The bolt's script: `life` counts down from 2; the strike is at 2, the thunder is at `life >= 0`.
#[derive(Component, Clone, Debug)]
pub struct Bolt {
    pub life: i8,
    pub flashes: u8,
    /// A skeleton trap's bolt (`setVisualOnly`): it flashes and sounds, and strikes nothing.
    pub visual: bool,
    /// Blocks it set on fire (`blocksSetOnFire`), read by the advancement trigger.
    pub fires: u16,
    /// The seed the client draws the branches from (`LightningBolt.seed`).
    pub seed: i64,
    pub rng: LegacyRandom,
    /// The player whose trident summoned it (`LightningBolt.setCause`): the victims it hits go to their channeled-lightning trigger.
    pub cause: Option<u32>,
}

/// The cause the next `strike` takes (set by the host just before it casts a channeled bolt).
#[derive(Resource, Default)]
pub struct NextCause(pub Option<u32>);

/// Once per bolt: the mooshrooms and copper golems it already changed (a flip is not repeated on the next tick).
#[derive(Component, Default)]
pub struct Flipped(pub Vec<Entity>);

/// The block side of a strike, written by the host: the fire a block takes, the powered rod and the copper a strike weathers back.
/// `None` where the host has no such block: nothing is set.
pub trait BoltLevel: Send + Sync + 'static {
    /// `ServerLevel.canSpreadFireAround`: false stops every fire draw of the bolt.
    fn may_spread_fire(&self, at: [i32; 3]) -> bool;
    /// The fire a block at `p` takes, when it is air that fire survives.
    fn fire_state(&self, p: [i32; 3]) -> Option<BlockStateId>;
    /// The lightning rod at `p`, powered.
    fn powered_rod(&self, p: [i32; 3]) -> Option<BlockStateId>;
    /// The copper at `p` after a strike (one weathering step back; waxed copper stays).
    fn copper_strike(&self, p: [i32; 3]) -> Option<BlockStateId>;
}
#[derive(Resource, Default)]
pub struct Strikes(pub Option<Box<dyn BoltLevel>>);

/// `new LightningBolt(level, ...)`: `seed` then `flashes` (`nextInt(3) + 1`) from the bolt's own generator; the first tick is the next one.
pub fn strike(w: &mut World, at: Vec3, visual: bool, rng_seed: u64) -> Entity {
    let mut rng = LegacyRandom::new(rng_seed as i64);
    let seed = rng.next_long();
    let flashes = rng.next_int_bound(3).unwrap_or(0) as u8 + 1;
    let cause = w.get_resource_mut::<NextCause>().and_then(|mut c| c.0.take());
    let e = w.spawn((Pos(at), Bolt { life: 2, flashes, visual, fires: 0, seed, rng, cause })).id();
    cow::after(w, e, 1, step);
    e
}

/// `LightningBolt.tick` for one bolt.
fn step(w: &mut World, e: Entity) {
    let (Some(mut b), Some(pos)) = (w.get::<Bolt>(e).cloned(), w.get::<Pos>(e).map(|p| p.0)) else { return };
    let at = [pos.x, pos.y, pos.z];
    if b.life == 2 {
        let pitch = 0.8 + b.rng.next_float() * 0.2;
        w.resource_mut::<Events>().host.sound("entity.lightning_bolt.thunder", at, 10000.0, pitch);
        let pitch = 0.5 + b.rng.next_float() * 0.2;
        w.resource_mut::<Events>().host.sound("entity.lightning_bolt.impact", at, 2.0, pitch);
        if !b.visual {
            // Normal and hard difficulties start fires with the strike itself (`spawnFire(4)`).
            if crate::effects::difficulty(w) >= 2 {
                b.fires += spawn_fire(w, at, 4, &mut b.rng);
            }
            strike_rod(w, at);
        }
        // Every server bolt clears the copper, the skeleton trap's too.
        clear_copper(w, at);
    }
    b.life -= 1;
    if b.life < 0 {
        if b.flashes == 0 {
            w.entity_mut(e).despawn();
            return;
        }
        if i32::from(b.life) < -b.rng.next_int_bound(10).unwrap_or(0) {
            b.flashes -= 1;
            b.life = 1;
            b.seed = b.rng.next_long();
            if !b.visual {
                b.fires += spawn_fire(w, at, 0, &mut b.rng);
            }
        }
    }
    if b.life >= 0 && !b.visual {
        thunder_hit(w, e, at);
    }
    w.entity_mut(e).insert(b);
    cow::after(w, e, 1, step);
}

/// `spawnFire`: fire at the strike's block and `extra` neighbours drawn from the bolt's generator, where the host lets fire stand.
/// Returns how many blocks it set.
fn spawn_fire(w: &mut World, at: [f64; 3], extra: i32, rng: &mut LegacyRandom) -> u16 {
    let base = [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32];
    let Some(level) = w.resource::<Strikes>().0.as_deref() else { return 0 };
    if !level.may_spread_fire(base) {
        return 0;
    }
    let spots = fire_spots(base, extra, rng);
    let mut set = Vec::new();
    for p in spots {
        if let Some(s) = level.fire_state(p) {
            set.push((p, s));
        }
    }
    let n = set.len() as u16;
    let mut host = w.resource_mut::<Events>();
    for (p, s) in set {
        host.host.set_block(p, s);
    }
    n
}

/// `LightningRodBlock.onLightningStrike`: the rod under the strike point is powered (its 8-tick unpowering is the host's).
fn strike_rod(w: &mut World, at: [f64; 3]) {
    let p = under(at);
    let Some(s) = w.resource::<Strikes>().0.as_deref().and_then(|l| l.powered_rod(p)) else { return };
    w.resource_mut::<Events>().host.set_block(p, s);
}

/// `LightningBolt.clearCopperOnLightningStrike` at the block under the strike point.
fn clear_copper(w: &mut World, at: [f64; 3]) {
    let p = under(at);
    let Some(s) = w.resource::<Strikes>().0.as_deref().and_then(|l| l.copper_strike(p)) else { return };
    w.resource_mut::<Events>().host.set_block(p, s);
}

/// `Entity.thunderHit` for the living entities whose box meets the strike box (3 around, 3 down, 9 up).
fn thunder_hit(w: &mut World, bolt: Entity, at: [f64; 3]) {
    let peaceful = crate::effects::difficulty(w) == 0;
    let mut struck: Vec<(Entity, Vec3)> = Vec::new();
    // The index gives the creatures near the strike (3 blocks plus half the widest body): no sweep of the world.
    let mut near = Vec::new();
    w.resource::<crate::cow::Index>().for_radius(at[0], at[2], 3.0 + MAX_HALF_WIDTH, |e| near.push(e));
    for e in near {
        let (Some(p), Some(b), Some(h)) = (w.get::<Pos>(e), w.get::<Body>(e), w.get::<Health>(e)) else { continue };
        let hit = strike_hit(p.0, b.width, b.height, at);
        if hit && h.0 > 0.0 {
            struck.push((e, p.0));
        }
    }
    if let Some(cause) = w.get::<Bolt>(bolt).and_then(|b| b.cause) {
        for (victim, _) in &struck {
            w.get_resource_or_insert_with(crate::colony_register::MiscOut::default)
                .0
                .push(crate::host_out_misc::MiscEvent::LightningVictim { cause, victim: *victim });
        }
    }
    for (t, pos) in struck {
        let name = w.get::<SpeciesId>(t).and_then(|s| w.resource::<SpeciesTable>().0.get(s.0 as usize).map(|s| s.name));
        use crate::species::lists::{struck as kind_of, Struck};
        let what = name.map_or(Struck::Plain, kind_of);
        if let Struck::Converts(into) = what {
            if !peaceful && crate::conversion::convert_or_ask(w, t, into, crate::conversion::ConvertKeep::NONE) {
                continue;
            }
        }
        match what {
            Struck::Flips => {
                if flip_once(w, bolt, t) {
                    if let Some(mut v) = w.get_mut::<crate::behaviours::variant::Variant>(t) {
                        v.0 = 1 - v.0.min(1);
                        drop(v);
                        crate::bridge::view::mark(w, t);
                    }
                    w.resource_mut::<Events>().host.sound("entity.mooshroom.convert", [pos.x, pos.y, pos.z], 2.0, 1.0);
                }
                continue;
            }
            Struck::Dies => {
                w.resource_mut::<DamageQueue>().add(t, f32::MAX, Cause::Lightning);
                continue;
            }
            Struck::Charges => {
                w.entity_mut(t).insert(crate::explosion::Charged);
            }
            _ => {}
        }
        // Entity.thunderHit: the remaining fire grows by one; at -1 (not burning) it is set for 8 seconds; then 5 damage.
        if let Some(until) = w.get::<cow::Burning>(t).map(|b| b.until) {
            w.entity_mut(t).insert(cow::Burning { until: until + 1 });
        } else {
            cow::set_on_fire(w, t, 160);
        }
        w.resource_mut::<DamageQueue>().add_from(t, 5.0, Cause::Lightning, (at[0], at[2]));
    }
}

/// The first time this bolt changes `t` (the list of `Flipped`); true then.
fn flip_once(w: &mut World, bolt: Entity, t: Entity) -> bool {
    if w.get::<Flipped>(bolt).is_none() {
        w.entity_mut(bolt).insert(Flipped::default());
    }
    let mut f = w.get_mut::<Flipped>(bolt).expect("inserted above");
    if f.0.contains(&t) {
        return false;
    }
    f.0.push(t);
    true
}
