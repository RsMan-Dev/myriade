//! The blast of a firework rocket (`FireworkRocketEntity.explode`) as data: `burst` computes what it reaches (the holder in full, the
//! creatures and the players within 5 blocks that see it) and `go_off` ends the rocket, queuing its `FireworkEvent`s for the host,
//! which applies the damage and shows the stars. The end of life is the rocket's wheel deadline (`rocket_end`): no tick visits it.
use crate::cow::*;
use crate::firework::{Rocket, EXPLODE_EVENT};
use crate::goals::{Events, Online};
use crate::host_out_firework::FireworkEvent;
use crate::nbt::{owner, Field};
use crate::projectile::Projectile;
use crate::rules::firework::{detonation_damage, exposure};
use crate::species::lists::FIREWORK_ROCKET;
use crate::targeting::player_box;
use bevy_ecs::prelude::*;
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::Vec3;
use horizon_core::nbt::{Compound, Tag};
use horizon_items::item_nbt::{item_from_nbt, item_to_nbt};
use std::borrow::Cow;
pub use horizon_items::firework::FireworkExplosion as FireworkStar;

/// The stars a rocket shows when it goes off (`FireworkRocketEntity.explosions`), kept beside it: none for a rocket without stars.
#[derive(Component, Clone, Debug)]
#[component(storage = "SparseSet")]
pub struct Stars(pub Vec<FireworkStar>);

/// The fireworks' outputs since the host last took them (`take_firework_events` on the bridge).
#[derive(Resource, Default)]
pub struct FireworkOut(pub Vec<FireworkEvent>);

/// The damage type of a rocket's blast (`DamageSources.fireworks`).
pub const DAMAGE_KIND: &str = "fireworks";

/// What a rocket's blast reaches: where it went off, its velocity, whether it makes its sound (not silent), its stars, the damage to
/// each creature in reach and to each player in reach (the holder first, with the full blast).
#[derive(Clone, Debug)]
pub struct Burst {
    pub at: Vec3,
    pub velocity: Vec3,
    pub sound: bool,
    pub stars: Vec<FireworkStar>,
    pub creatures: Vec<(Entity, f32)>,
    pub players: Vec<(u32, f32)>,
}

/// The blast of rocket `e` (`dealExplosionDamage`): `5 + 2 * explosions` to the holder, then to each creature and player within 5
/// blocks that sees the blast at its feet or its middle. A rocket without stars reaches nobody.
pub fn burst(w: &mut World, e: Entity) -> Burst {
    let velocity = w.get::<Vel>(e).map_or(Vec3::new(0.0, 0.0, 0.0), |v| v.0);
    let (Some(r), Some(at)) = (w.get::<Rocket>(e).copied(), w.get::<crate::cow::Pos>(e).map(|p| p.0)) else {
        return Burst { at: Vec3::new(0.0, 0.0, 0.0), velocity, sound: false, stars: Vec::new(), creatures: Vec::new(), players: Vec::new() };
    };
    let stars = w.get::<Stars>(e).map_or_else(Vec::new, |s| s.0.clone());
    let mut out = Burst { at, velocity, sound: !r.silent, stars, creatures: Vec::new(), players: Vec::new() };
    if r.explosions == 0 {
        return out;
    }
    let damage = detonation_damage(r.explosions);
    if let Some(id) = r.carrier {
        out.players.push((id, damage));
    }
    let mut near = Vec::new();
    w.resource::<Index>().for_radius(at.x, at.z, 6.0, |c| near.push(c));
    let mut around = Vec::new();
    w.resource::<Online>().for_near(at.x, at.z, 6.0, |pl| around.push(pl));
    if near.is_empty() && around.is_empty() {
        return out;
    }
    let level = *w.resource::<Level>();
    for c in near {
        if w.get::<Health>(c).is_none() {
            continue;
        }
        let Some(b) = w.get::<Bounds>(c).map(|b| b.0) else { continue };
        let center = b.center();
        let feet = Vec3::new(center.x, b.min_position().y, center.z);
        let height = b.max_position().y - b.min_position().y;
        if let Some(amount) = exposure(level.mw.0, at, feet, height, damage) {
            out.creatures.push((c, amount));
        }
    }
    for pl in around {
        if Some(pl.id) == r.carrier {
            continue;
        }
        let feet = Vec3::new(pl.x, pl.y, pl.z);
        let body = player_box(&pl);
        let height = body.max_position().y - body.min_position().y;
        if let Some(amount) = exposure(level.mw.0, at, feet, height, damage) {
            out.players.push((pl.id, amount));
        }
    }
    out
}

/// The rocket goes off where it is (`FireworkRocketEntity.explode`): the status 17 to the clients, its burst and its hurts queued, then
/// it is gone.
pub fn go_off(w: &mut World, e: Entity) {
    let b = burst(w, e);
    let cause = w.get::<Projectile>(e).and_then(|p| p.owner);
    w.resource_mut::<Events>().host.entity_event(e, EXPLODE_EVENT);
    let mut q = w.resource_mut::<FireworkOut>();
    let at = [b.at.x, b.at.y, b.at.z];
    let velocity = [b.velocity.x, b.velocity.y, b.velocity.z];
    q.0.push(FireworkEvent::Burst { at, velocity, sound: b.sound, stars: b.stars });
    for (victim, damage) in b.creatures {
        q.0.push(FireworkEvent::Hurt { victim, damage, at, kind: DAMAGE_KIND, cause });
    }
    for (player, damage) in b.players {
        q.0.push(FireworkEvent::PlayerBlast { player, damage, at, kind: DAMAGE_KIND, cause });
    }
    q.0.push(FireworkEvent::Expired { entity: e });
    drop(q);
    w.entity_mut(e).despawn();
}

/// The deadline of a rocket's life (`after`, set at launch): it goes off where it is, at the start of its tick.
pub fn rocket_end(w: &mut World, e: Entity) {
    if w.get::<Rocket>(e).is_some() {
        go_off(w, e);
    }
}

/// A hit on a flying rocket by another entity than itself: it goes off where it is.
pub fn impact(w: &mut World, e: Entity, hit: Hit) {
    if hit.target != e && w.get::<Rocket>(e).is_some() {
        go_off(w, e);
    }
}

/// The keys `FireworkRocketEntity.addAdditionalSaveData` writes beyond the projectile's and the rocket's in `nbt_fields`: the life
/// (`LifeTime`) and the rocket's item (`FireworksItem`, holding its flight and its stars).
pub const FIELDS: &[Field] = &[
    Field { key: Cow::Borrowed("LifeTime"), write: write_lifetime, read: read_lifetime, accepts: crate::nbt::accept::int, owner: Some(owner::<Rocket>) },
    Field { key: Cow::Borrowed("FireworksItem"), write: write_item, read: read_item, accepts: crate::nbt::accept::any, owner: Some(owner::<Rocket>) },
];

fn write_lifetime(w: &World, e: Entity, c: &mut Compound) {
    if let Some(r) = w.get::<Rocket>(e) {
        c.insert("LifeTime".into(), Tag::Int(r.lifetime));
    }
}

fn read_lifetime(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let Some(mut r) = w.get_mut::<Rocket>(e) else { return false };
    r.lifetime = *v;
    true
}

/// The rocket's item: a firework rocket whose `fireworks` hold its flight and its stars.
fn write_item(w: &World, e: Entity, c: &mut Compound) {
    let (Some(r), Some(stars)) = (w.get::<Rocket>(e), w.get::<Stars>(e)) else { return };
    let mut stack = ItemStack::new(FIREWORK_ROCKET, 1);
    stack.data_mut().fireworks = Some((r.flight, stars.0.clone()));
    c.insert("FireworksItem".into(), item_to_nbt(&stack));
}

fn read_item(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Some(stack) = item_from_nbt(t) else { return false };
    let Some((flight, stars)) = stack.data().and_then(|d| d.fireworks.clone()) else { return false };
    let Some(mut r) = w.get_mut::<Rocket>(e) else { return false };
    r.flight = flight;
    r.explosions = stars.len() as u8;
    drop(r);
    w.entity_mut(e).insert(Stars(stars));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cow::Pos;
    use horizon_items::firework::FireworkShape;

    fn world_with(explosions: u8, carrier: Option<u32>) -> (World, Entity) {
        let mut w = World::new();
        w.insert_resource(Index::default());
        w.insert_resource(Online::default());
        let e = w
            .spawn((Pos(Vec3::new(0.0, 64.0, 0.0)), Projectile { owner: None, age: 0, left_owner: false, expires: 0 }, Rocket { explosions, flight: 1, lifetime: 0, carrier, angled: false, horizontal: false, silent: false }, Stars(Vec::new())))
            .id();
        (w, e)
    }

    #[test]
    fn a_saved_rocket_loads_with_its_life_its_flight_and_its_stars() {
        let star = FireworkStar { shape: FireworkShape::Star, colors: vec![0xFF0000], fade: Vec::new(), trail: true, twinkle: false };
        let mut a = World::new();
        let e = a.spawn((Rocket { explosions: 1, flight: 2, lifetime: 37, carrier: None, angled: false, horizontal: false, silent: false }, Stars(vec![star]))).id();
        let mut c = Compound::new();
        for f in FIELDS {
            (f.write)(&a, e, &mut c);
        }
        let saved = Tag::Compound(c);
        let mut b = World::new();
        let d = b.spawn((Rocket { explosions: 0, flight: 0, lifetime: 0, carrier: None, angled: false, horizontal: false, silent: false }, Stars(Vec::new()))).id();
        for f in FIELDS {
            let t = saved.get(f.key.as_ref()).expect("the field was written");
            assert!((f.accepts)(t) && (f.read)(&mut b, d, t));
        }
        let r = *b.get::<Rocket>(d).unwrap();
        assert_eq!((r.flight, r.lifetime, r.explosions), (2, 37, 1));
        assert_eq!(b.get::<Stars>(d).unwrap().0[0].colors, vec![0xFF0000]);
    }

    #[test]
    fn a_rocket_without_stars_reaches_nobody() {
        let (mut w, e) = world_with(0, Some(3));
        let b = burst(&mut w, e);
        assert!(b.creatures.is_empty() && b.players.is_empty());
    }

    #[test]
    fn the_holder_takes_five_plus_two_per_star() {
        let (mut w, e) = world_with(3, Some(3));
        let b = burst(&mut w, e);
        assert_eq!(b.players, vec![(3, 11.0)]);
    }
}
