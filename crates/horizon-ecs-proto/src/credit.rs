//! The last blow a player dealt (`LivingEntity.resolvePlayerResponsibleForDamage`, `lastHurtByPlayer` and `lastHurtByPlayerMemoryTime`): a hit
//! whose cause is a player (a projectile's owner too) remembers it for 100 ticks of the entity's own time. A death reads it: the experience and
//! the loot that vanilla gives for "killed by a player" (`dropAllDeathLoot`, `dropExperience`, the equipment's drop chances). The state is a
//! sparse component written by the hit; the end is a deadline compared with the entity's own time when read, nothing counts down per tick.
//! A creature carrying `CreditsOwner` (the rule `credits_owner`, set by the species table) credits its owner instead (a tamed wolf).
use crate::cow::now_of;
use crate::nbt::{accept, owner, Field};
use crate::player_uuids::{id_of, uuid_of, wait, Slot};
use crate::rules::Rule;
use crate::tame::Owner;
use crate::targeting::Who;
use bevy_ecs::prelude::*;
use horizon_core::nbt::{Compound, Tag};
use std::borrow::Cow;

/// `setLastHurtByPlayer(player, 100)`.
pub const MEMORY: u64 = 100;
const KEY_PLAYER: &str = "last_hurt_by_player";
const KEY_TIME: &str = "last_hurt_by_player_memory_time";

/// Who last hurt the entity as a player and until when (the entity's own time). `player` is None while the save's uuid names a player that
/// has not joined (`uuid` keeps it), or when the owner credited is no player.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(storage = "SparseSet")]
pub struct PlayerCredit {
    pub player: Option<u32>,
    pub uuid: Option<[i32; 4]>,
    pub until: u64,
}

/// A tamed creature whose blows credit its owner (`Wolf`'s clause of `resolvePlayerResponsibleForDamage`).
#[derive(Component, Clone, Copy, Debug)]
pub struct CreditsOwner;
/// The rule `credits_owner` (no params).
pub static CREDITS_OWNER: Rule = Rule {
    name: "credits_owner",
    equip: |w, e, _| {
        w.entity_mut(e).insert(CreditsOwner);
    },
    attrs: crate::rules::no_attrs,
    interact: crate::rules::no_interact,
};

fn words(w: &World, player: Option<u32>) -> Option<[i32; 4]> {
    uuid_of(w, player?).map(crate::bridge::uuids::to_words)
}

/// A hit on `e` by `by`: a player credits itself, a creature with `CreditsOwner` its owner (cleared when it has none); anything else leaves
/// the memory as it was. Called by the damage step for every hit that lands, a fatal one included.
pub fn record(w: &mut World, e: Entity, by: Option<Who>) {
    let player = match by {
        Some(Who::Player(id)) => Some(id),
        Some(Who::Entity(a)) if w.get::<CreditsOwner>(a).is_some() => match w.get::<Owner>(a) {
            Some(Owner(Some(Who::Player(id)))) => Some(*id),
            Some(Owner(Some(Who::Entity(_)))) => None,
            Some(Owner(None)) => {
                w.entity_mut(e).remove::<PlayerCredit>();
                return;
            }
            None => return,
        },
        _ => return,
    };
    let until = now_of(w, e) + MEMORY;
    let uuid = words(w, player);
    w.entity_mut(e).insert(PlayerCredit { player, uuid, until });
}

/// The credit of `e` while it is remembered (`lastHurtByPlayerMemoryTime > 0`).
pub fn active(w: &World, e: Entity) -> Option<PlayerCredit> {
    w.get::<PlayerCredit>(e).copied().filter(|c| c.until > now_of(w, e))
}
/// Whether a player's blow is still remembered ("killed by a player").
pub fn killed_by_player(w: &World, e: Entity) -> bool {
    active(w, e).is_some()
}
/// The player credited, when it is known.
pub fn player_of(w: &World, e: Entity) -> Option<u32> {
    active(w, e)?.player
}

/// The keys: both are written while the credit is remembered and both read pose it.
pub const CREATORS: &[Field] = &[
    Field { key: Cow::Borrowed(KEY_PLAYER), write: write_player, read: read_player, accepts: uuid_tag, owner: Some(owner::<PlayerCredit>) },
    Field { key: Cow::Borrowed(KEY_TIME), write: write_time, read: read_time, accepts: accept::int, owner: Some(owner::<PlayerCredit>) },
];
fn uuid_tag(t: &Tag) -> bool {
    matches!(t, Tag::IntArray(v) if v.len() == 4)
}
fn write_player(w: &World, e: Entity, c: &mut Compound) {
    let Some(k) = active(w, e) else { return };
    if let Some(u) = words(w, k.player).or(k.uuid) {
        c.insert(KEY_PLAYER.into(), Tag::IntArray(u.to_vec()));
    }
}
fn write_time(w: &World, e: Entity, c: &mut Compound) {
    if let Some(k) = active(w, e) {
        c.insert(KEY_TIME.into(), Tag::Int(i32::try_from(k.until - now_of(w, e)).unwrap_or(i32::MAX)));
    }
}
fn read_player(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::IntArray(v) = t else { return false };
    let Ok(u) = <[i32; 4]>::try_from(v.as_slice()) else { return false };
    let known = id_of(w, crate::bridge::uuids::from_words(u));
    if known.is_none() {
        wait(w, crate::bridge::uuids::from_words(u), e, Slot::Credit);
    }
    let mut k = w.get::<PlayerCredit>(e).copied().unwrap_or(PlayerCredit { player: None, uuid: None, until: 0 });
    (k.player, k.uuid) = (known, Some(u));
    w.entity_mut(e).insert(k);
    true
}
fn read_time(w: &mut World, e: Entity, t: &Tag) -> bool {
    let Tag::Int(v) = t else { return false };
    let until = now_of(w, e) + u64::try_from(*v).unwrap_or(0);
    let mut k = w.get::<PlayerCredit>(e).copied().unwrap_or(PlayerCredit { player: None, uuid: None, until: 0 });
    k.until = until;
    w.entity_mut(e).insert(k);
    true
}
