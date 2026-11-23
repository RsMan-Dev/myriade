//! The blow, the strike and the pickup of a thrown trident: what the flight of `trident.rs` meets. The damage stays the flight's (the
//! `DamageQueue` and the player's hurt); the effects the host plays (sounds, statistics, the thunder bolt, the pickup) leave as
//! `TridentEvent`s. A strike needs a storm and a sky above the victim (`ThrownTrident.onHitEntity`, Channeling). One pickup path: the
//! flight's touch and the host's request both call `pickup`, which is idempotent (a taken trident is gone).
use crate::cow::{Clock, Level, Pos, TypeName};
use crate::goals::Online;
use crate::host_out_trident::{TridentEvent, TridentOut};
use crate::hostile::PlayerState;
use crate::targeting::Who;
use crate::trident::{forget, HitBonus, Pickup, Planted, Trident, BLOW};
use bevy_ecs::prelude::*;
use myriade_engine_lib::items::ItemStack;
use myriade_engine_lib::mobs::spawn_rules::SpawnLevel;
use horizon_core::geometry::Vec3;

/// What a blow does: the damage the flight deals (base plus the enchantment bonus), and whether Channeling strikes the victim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TridentHit {
    pub damage: f32,
    pub channel: bool,
}

fn emit(w: &mut World, ev: TridentEvent) {
    w.resource_mut::<TridentOut>().0.push(ev);
}

/// Channeling strikes where the victim stands: the trident is channeling, it is thundering and the sky is seen there.
fn channels(w: &World, trident: Entity, at: Vec3) -> bool {
    if !w.get::<Trident>(trident).is_some_and(|t| t.channeling) {
        return false;
    }
    let mw = w.resource::<Level>().mw.0;
    mw.thundering() && mw.can_see_sky(at.x.floor() as i32, at.y.floor() as i32, at.z.floor() as i32)
}

/// The blow: `BLOW` plus the bonus of the stack against the victim (`HitBonus`, computed at the impact), then the strike if Channeling.
fn blow(w: &mut World, trident: Entity, victim: Who, at: Vec3) -> TridentHit {
    let species = match victim {
        Who::Player(_) => "minecraft:player",
        Who::Entity(e) => w.get::<TypeName>(e).map_or("", |t| t.0),
    };
    let (by, bonus) = match w.get::<Trident>(trident) {
        Some(t) => (t.owner, (w.resource::<HitBonus>().0)(&t.stack, victim, species)),
        None => (None, 0.0),
    };
    let channel = channels(w, trident, at);
    emit(w, TridentEvent::Hit { entity: trident, by, victim, base: BLOW });
    if channel {
        emit(w, TridentEvent::Thunder { at: [at.x, at.y, at.z], cause: by });
    }
    TridentHit { damage: BLOW + bonus, channel }
}

/// The blow on a creature `victim` (its position is its `Pos`; a creature with none takes the origin).
pub fn hit_entity(w: &mut World, trident: Entity, victim: Entity) -> TridentHit {
    let at = w.get::<Pos>(victim).map_or(Vec3::new(0.0, 0.0, 0.0), |p| p.0);
    blow(w, trident, Who::Entity(victim), at)
}

/// The blow on a player `player`, standing at `at` (its feet).
pub fn hit_player(w: &mut World, trident: Entity, player: u32, at: Vec3) -> TridentHit {
    blow(w, trident, Who::Player(player), at)
}

/// The trident sticks in the block `_at` (`Landed`: the sound of the plant). The block's own effects stay with the host.
pub fn hit_block(w: &mut World, trident: Entity, _at: [i32; 3]) {
    emit(w, TridentEvent::Landed { entity: trident });
}

/// `ThrownTrident.playerTouch` for one player: the shake is over (when planted), the player is the owner (or any when there is none),
/// its box meets the trident and the pickup rule allows it. The trident is removed and `PickedUp` emitted with its stack, which is
/// returned; None when any check fails (a trident already taken is gone, so asking again is None).
pub fn pickup(w: &mut World, trident: Entity, player: u32) -> Option<ItemStack> {
    let t = w.get::<Trident>(trident)?;
    let (owner, rule, stack) = (t.owner, t.pickup, t.stack.clone());
    if rule == Pickup::Disallowed {
        return None;
    }
    if w.get::<Planted>(trident).is_some_and(|p| w.resource::<Clock>().0 < p.shake_end) {
        return None;
    }
    if owner.is_some_and(|o| o != Who::Player(player)) {
        return None;
    }
    let pl = w.resource::<Online>().get(player)?;
    let at = w.get::<Pos>(trident)?.0;
    if (pl.x - at.x).abs() >= 1.0 || (pl.z - at.z).abs() >= 1.0 || at.y <= pl.y - 1.0 || at.y >= pl.y + 2.5 {
        return None;
    }
    if rule == Pickup::CreativeOnly && !w.resource::<PlayerState>().creative(player) {
        return None;
    }
    forget(w, trident);
    emit(w, TridentEvent::PickedUp { entity: trident, by: Who::Player(player), stack: stack.clone() });
    w.despawn(trident);
    Some(stack)
}

/// The events of the trident world since the last call, in order (the host turns them into its own).
pub fn drain_events(w: &mut World) -> Vec<TridentEvent> {
    std::mem::take(&mut w.resource_mut::<TridentOut>().0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trident::TridentIndex;

    fn world() -> World {
        let mut w = World::new();
        w.insert_resource(Online::default());
        w.insert_resource(Clock::default());
        w.insert_resource(TridentOut::default());
        w.insert_resource(TridentIndex::default());
        w.insert_resource(PlayerState::default());
        w.resource_mut::<Online>().set(crate::goals::Player { id: 1, ..Default::default() });
        w
    }

    fn spawn(w: &mut World, pickup_rule: Pickup, planted_until: Option<u64>) -> Entity {
        let stack = ItemStack { item: "minecraft:trident".into(), count: 1, damage: 0, extra: None };
        let t = Trident { owner: None, loyalty: 0, pickup: pickup_rule, dealt: false, stack, channeling: false };
        let planted = planted_until.map(|shake_end| Planted { block: [0, 0, 0], since: 0, shake_end });
        let e = w.spawn((t, Pos(Vec3::new(0.0, 0.0, 0.0)))).id();
        if let Some(p) = planted {
            w.entity_mut(e).insert(p);
        }
        e
    }

    #[test]
    fn a_pickup_is_given_once() {
        let mut w = world();
        let e = spawn(&mut w, Pickup::Allowed, None);
        assert!(pickup(&mut w, e, 1).is_some());
        assert!(pickup(&mut w, e, 1).is_none());
    }

    #[test]
    fn creative_only_goes_to_a_creative_player() {
        let mut w = world();
        let e = spawn(&mut w, Pickup::CreativeOnly, None);
        assert!(pickup(&mut w, e, 1).is_none());
        w.resource_mut::<PlayerState>().set_creative(1, true);
        assert!(pickup(&mut w, e, 1).is_some());
    }

    #[test]
    fn a_planted_trident_waits_for_its_shake_to_end() {
        let mut w = world();
        let e = spawn(&mut w, Pickup::Allowed, Some(10));
        w.resource_mut::<Clock>().0 = 5;
        assert!(pickup(&mut w, e, 1).is_none());
        w.resource_mut::<Clock>().0 = 10;
        assert!(pickup(&mut w, e, 1).is_some());
    }

    #[test]
    fn a_disallowed_trident_is_never_given() {
        let mut w = world();
        let e = spawn(&mut w, Pickup::Disallowed, None);
        assert!(pickup(&mut w, e, 1).is_none());
    }
}
