//! The host's reads and writes of a creature's hurt state: the damage window, `invulnerableTime`, the last attackers, the combat log,
//! fire, the creeper's fuse, glowing, the enderman's teleport tries and one draw of the creature's own random.
use crate::bridge::host::EcsWorld;
use crate::bridge::view;
use crate::cow::{now_of, Burning, CombatLog, HurtAt, InvulnerableUntil, LastHurtBy};
use crate::explosion::{set_dir, Fuse};
use crate::flat::Rng;
use crate::goals::Events;
use crate::targeting::Who;
use bevy_ecs::entity::Entity;
use horizon_core::nbt::{Compound, JavaString, Tag};
use horizon_client_lib::mobs::MobWorld;

/// `LivingEntity.damageCooldownTime` after a hit; a hit inside the half above `HURT_WINDOW` counts only by its excess.
const COOLDOWN_AFTER_HIT: u64 = 20;
/// `lastHurtByPlayerMemoryTime` and the memory of `lastHurtByMob`.
const ATTACKER_MEMORY: u64 = 100;

impl EcsWorld {
    /// `damageCooldownTime`, derived from the last hurt (`HurtAt`): 20 at the hit, counting down; 0 when none or run out.
    pub fn damage_cooldown(&self, e: Entity) -> Option<i32> {
        let h = self.world().get::<HurtAt>(e)?;
        let now = now_of(self.world(), e);
        Some(if h.at == 0 { 0 } else { (h.at + COOLDOWN_AFTER_HIT).saturating_sub(now) as i32 })
    }

    /// Sets `damageCooldownTime` to `ticks` by moving the hurt's tick; at or under 10 the window is closed. A creature younger than the
    /// window cannot hold an earlier hurt tick: its remaining time then reads longer than `ticks` (the hurt tick stays at 1 or more).
    pub fn set_damage_cooldown(&mut self, e: Entity, ticks: i32) -> bool {
        let now = now_of(self.world(), e);
        let Some(mut h) = self.world_mut().get_mut::<HurtAt>(e) else { return false };
        h.at = if ticks <= 10 { 0 } else { (now + ticks as u64).saturating_sub(COOLDOWN_AFTER_HIT).max(1) };
        true
    }

    /// `Entity.invulnerableTime`: what is left of the window a mod set; 0 when none. None when `e` is not an entity.
    pub fn invulnerable_time(&self, e: Entity) -> Option<i32> {
        let w = self.world();
        w.get_entity(e).ok()?;
        Some(w.get::<InvulnerableUntil>(e).map_or(0, |u| u.0.saturating_sub(now_of(w, e)) as i32))
    }

    /// `Entity.setInvulnerableTime`; the window refuses what `isInvulnerableToBase` refuses while it runs.
    pub fn set_invulnerable_time(&mut self, e: Entity, ticks: i32) -> bool {
        let w = self.world_mut();
        if w.get_entity(e).is_err() {
            return false;
        }
        if ticks <= 0 {
            w.entity_mut(e).remove::<InvulnerableUntil>();
        } else {
            let until = now_of(w, e) + ticks as u64;
            w.entity_mut(e).insert(InvulnerableUntil(until));
        }
        true
    }

    /// `getLastHurtByMob` while it is remembered (100 ticks of the entity's own time).
    pub fn recent_attacker(&self, e: Entity) -> Option<Who> {
        let l = self.world().get::<LastHurtBy>(e)?;
        (now_of(self.world(), e).saturating_sub(l.at) <= ATTACKER_MEMORY).then_some(l.who)
    }

    /// `lastHurtByPlayerMemoryTime`: what is left of the 100 ticks after a player's hit; 0 when the last attacker is no player.
    pub fn last_hurt_by_player(&self, e: Entity) -> i32 {
        match self.world().get::<LastHurtBy>(e) {
            Some(l) if matches!(l.who, Who::Player(_)) => (l.at + ATTACKER_MEMORY).saturating_sub(now_of(self.world(), e)) as i32,
            _ => 0,
        }
    }

    /// The `CombatTracker` of `e`; None until its first hit.
    pub fn combat_log(&self, e: Entity) -> Option<&CombatLog> {
        self.world().get::<CombatLog>(e)
    }

    /// The player `player` hurt `e` with `weapon` in its main hand and `luck`: what its death reads of the killer (`loot::KillerKit`).
    /// Told with a player's hit, never with a fact of every player.
    pub fn note_killer(&mut self, e: Entity, player: u32, luck: f32, weapon: Option<horizon_client_lib::items::ItemStack>) {
        crate::loot::note_killer(self.world_mut(), e, player, luck, weapon);
    }

    /// `Creeper.ignite`: the fuse runs toward the blast whatever the creeper targets. False for a creature without a fuse.
    pub fn set_ignited(&mut self, e: Entity) -> bool {
        let w = self.world_mut();
        let Some(mut f) = w.get_mut::<Fuse>(e) else { return false };
        f.ignited = true;
        set_dir(w, e, 1);
        view::mark(w, e);
        true
    }

    /// `Entity.clearFire`: the fire goes out and the tracker is told. False when `e` was not burning.
    pub fn extinguish(&mut self, e: Entity) -> bool {
        let w = self.world_mut();
        if w.get::<Burning>(e).is_none() {
            return false;
        }
        w.entity_mut(e).remove::<Burning>();
        w.resource_mut::<Events>().extinguished.push(e);
        view::mark(w, e);
        true
    }

    /// `Entity.setGlowingTag`: the saved `Glowing` key, which the shared flags read (`sync_core::shared`).
    pub fn set_glowing(&mut self, e: Entity, on: bool) -> bool {
        let patch = Compound::from([(JavaString::from("Glowing"), Tag::Byte(i8::from(on)))]);
        let done = self.merge_nbt(e, &patch);
        if done {
            view::mark(self.world_mut(), e);
        }
        done
    }

    /// `Enderman.teleport` repeated: up to `tries` random teleports, until one succeeds (`ender_teleport::repeatedly`).
    pub fn repeated_teleport(&mut self, mw: &MobWorld<'_>, e: Entity, tries: u32) -> bool {
        let moved = self.lent(mw, |w| crate::behaviours::ender_teleport::repeatedly(w, e, tries));
        if moved {
            view::mark(self.world_mut(), e);
        }
        moved
    }

    /// Gives the creature the tag `name` as `Entity.addTag` does (`nbt_tag::add`); false when it refuses.
    pub fn add_spawn_tag(&mut self, e: Entity, name: &str) -> bool {
        crate::nbt_tag::add(self.world_mut(), e, name)
    }

    /// A seed for a host-side `LegacyRandom`, drawn from the creature's own generator (two draws, which advance it).
    pub fn draw_seed(&mut self, e: Entity) -> Option<i64> {
        let mut r = self.world_mut().get_mut::<Rng>(e)?;
        let hi = u64::from(r.next());
        let lo = u64::from(r.next());
        Some(((hi << 32) | lo) as i64)
    }
}
