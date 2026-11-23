//! Mob handles for native server mods.
use super::{Game, commands::Target};
use horizon_client::mobs::{self, Mob, Species, Who};
use horizon_ecs_proto::targeting::Who as PWho;
use mod_api::{EntityRef, LivingRef, MobRef};

/// What decides whether a player can be a mob's target.
pub(super) struct PlayerFacts {
    pub mode: i32,
    pub alive: bool,
    pub invulnerable: bool,
    pub uuid: [i32; 4],
}

impl Game<'_> {
    /// Restore a saved anger target after insertion, so the target-change hook
    /// receives a live actor and a currently resolvable living target. A
    /// target that is absent or invalid resolves to none, as EntityReference
    /// does while vanilla loads a neutral mob. Returns false only when the
    /// actor cannot yet be represented by a native handle.
    pub(in crate::game) fn native_restore_loaded_target(
        &mut self,
        id: u32,
        uuid: Option<[i32; 4]>,
    ) -> bool {
        let Some(mob) = self.entity(id).and_then(|entity| self.mob(entity)) else {
            return false;
        };
        let target = uuid.and_then(|words| {
            let uuid = horizon_client::lan::playerdata::ints_uuid(words);
            let entity = self
                .player(uuid)
                .map(|player| player.entity())
                .or_else(|| {
                    let target = self
                        .persist
                        .uuids
                        .iter()
                        .find_map(|(&id, found)| (*found == words).then_some(id))?;
                    self.entity(target)
                })?;
            self.living(entity)
        });
        let target = target.filter(|target| {
            let Some(kind) = self.native_entity_target(target.entity()) else {
                return false;
            };
            self.native_ecs_valid_target(id, kind)
        });
        // Cancellation leaves the newly loaded mob's target unchanged.
        let _ = self.mob_set_target(mob, target);
        true
    }

    /// A player target's game mode, liveness, invulnerability (the ability,
    /// the `Invulnerable` flag or its window) and uuid.
    pub(super) fn native_player_facts(&self, target: Target) -> Option<PlayerFacts> {
        match target {
            Target::Guest(conn) => {
                let g = self.command_guest(conn)?;
                if g.is_fake() {
                    return None;
                }
                Some(PlayerFacts {
                    mode: g.game_mode,
                    alive: !g.dead && g.health > 0.0,
                    invulnerable: g.motion.as_ref().is_some_and(|p| {
                        p.invulnerable
                            || p.health.permanently_invulnerable
                            || p.health.invulnerable_time > 0
                    }),
                    uuid: horizon_client::lan::playerdata::uuid_ints(g.uuid),
                })
            }
            Target::Player => {
                if self.lan_acting_fake {
                    return None;
                }
                let player = self.player.as_ref()?;
                Some(PlayerFacts {
                    mode: self.mode_id(),
                    alive: !player.health.dead_or_dying(),
                    invulnerable: player.invulnerable
                        || player.health.permanently_invulnerable
                        || player.health.invulnerable_time > 0,
                    uuid: self
                        .lan_acting_uuid
                        .map(horizon_client::lan::playerdata::uuid_ints)
                        .or(self.level.player_uuid)
                        .unwrap_or_default(),
                })
            }
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum MobFlag {
    Aggressive,
    Persistent,
}

impl Game<'_> {
    /// `Mob.asValidTarget` and the modeled `canAttack` overrides: the game's
    /// own state as the target's facts, through the one rule set
    /// [`Mob::valid_target`] (arismoko/horizon#1175) a goal tick's view shares.
    pub(super) fn native_mob_valid_target(&self, mob: &Mob, target: Target) -> bool {
        if mob.species() == Species::Warden {
            return self.native_warden_valid_target(mob, target);
        }
        match target {
            Target::Mob(id) => {
                let Some(species) = self.creature_species(id) else {
                    return false;
                };
                mob.valid_target(
                    &self.mob_world_base(&[]),
                    &mobs::goals::TargetFacts::Mob {
                        id,
                        species,
                        alive: self.ecs.alive(id),
                        invulnerable: self.ecs_flag(id, "Invulnerable").unwrap_or(false),
                        baby: self.ecs.age(id).is_some_and(|age| age < 0),
                    },
                )
            }
            Target::Player | Target::Guest(_) => {
                let Some(PlayerFacts {
                    mode,
                    alive,
                    invulnerable,
                    uuid,
                }) = self.native_player_facts(target)
                else {
                    return false;
                };
                mob.valid_target(
                    &self.mob_world_base(&[]),
                    &mobs::goals::TargetFacts::Player {
                        alive,
                        invulnerable,
                        // The game modes a target cannot be attacked in:
                        // creative (1) and spectator (3).
                        creative: mode == 1,
                        spectator: mode == 3,
                        uuid,
                    },
                )
            }
            _ => false,
        }
    }
    /// `native_mob_valid_target` for the ECS mob `mob`: a target that is alive and not allied with it, or a player
    /// or guest outside the game modes that cannot be attacked (creative, spectator).
    pub(super) fn native_ecs_valid_target(&self, mob: u32, target: Target) -> bool {
        match target {
            Target::Mob(id) => {
                id != mob
                    && self.creature_species(id).is_some()
                    && self.ecs.alive(id)
                    && !horizon_client::scoreboard::teams_allied(self.mob_team(mob), self.mob_team(id))
            }
            Target::Player | Target::Guest(_) => {
                self.native_player_facts(target).is_some_and(|f| f.alive && !f.invulnerable && !matches!(f.mode, 1 | 3))
            }
            _ => false,
        }
    }
    /// `Warden.canTargetEntity`, which overrides the usual `Mob.canAttack`.
    fn native_warden_valid_target(&self, mob: &Mob, target: Target) -> bool {
        use horizon_core::geometry::Axis;
        let (bounds, team) = match target {
            Target::Mob(id) => {
                if !self.ecs.alive(id)
                    || self.ecs_flag(id, "Invulnerable") == Some(true)
                    || self.ecs.kind(id) == Some("minecraft:warden")
                {
                    return false;
                }
                let Some(bounds) = self.ecs.bounds(id) else {
                    return false;
                };
                (bounds, self.mob_team(id))
            }
            Target::Guest(conn) => {
                let Some(g) = self.command_guest(conn) else {
                    return false;
                };
                if g.dimension != self.dim.id()
                    || g.dead
                    || g.health <= 0.0
                    || matches!(g.game_mode, 1 | 3)
                    || g.motion.as_ref().is_some_and(|p| p.invulnerable)
                {
                    return false;
                }
                (g.bounds(), self.score.board.players_team(&g.name))
            }
            Target::Player => {
                let Some(player) = &self.player else {
                    return false;
                };
                if matches!(self.mode_id(), 1 | 3)
                    || player.health.dead_or_dying()
                    || player.invulnerable
                {
                    return false;
                }
                (player.bounds(), self.player_team())
            }
            _ => return false,
        };
        let border = self.border.geometry();
        !horizon_client::scoreboard::teams_allied(self.mob_team(mob.id), team)
            && border.is_within_bounds(bounds.min(Axis::X), bounds.min(Axis::Z), 0.0)
            && border.is_within_bounds(
                bounds.max(Axis::X) - f64::from(1.0e-5_f32),
                bounds.max(Axis::Z) - f64::from(1.0e-5_f32),
                0.0,
            )
    }

    /// The entity as a vanilla Mob: a mob or the ender dragon. Players, armor
    /// stands, stale handles and clients are rejected. Mobs taken out for
    /// ticking are unavailable (a arismoko/horizon#29 follow-up).
    pub(super) fn mob(&self, entity: EntityRef) -> Option<MobRef> {
        match self.native_entity_target(entity)? {
            Target::Mob(_) | Target::Dragon(_) => Some(MobRef::new(entity)),
            _ => None,
        }
    }

    /// The existing portal decision for an available mob in the current level.
    pub(super) fn mob_can_use_portal(&self, mob: MobRef) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        Some(self.ecs.can_use_portal(id))
    }

    /// `Mob.getTarget`. Brain-backed mobs read their attack memory; piglins
    /// and hoglins use Horizon's `ai.target` stand-in. Invalid targets, an
    /// unavailable mob or player binding, or a target without a living handle
    /// read as None. The dragon reads what a mod set, while it stays a valid
    /// target (`EnderDragon.canAttack`); its flight never sets one.
    pub(super) fn mob_target(&self, mob: MobRef) -> Option<LivingRef> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return self.native_dragon_target(mob.entity());
        };
        let target = self.ecs.ai_target(id)?;
        let target_id = match target {
            Who::Mob(id) => id,
            Who::Player => {
                let host = self.lan.as_ref()?;
                let conn = host.mob_players.get(&id)?;
                u32::try_from(host.hub.guest(*conn)?.entity_id).ok()?
            }
        };
        let entity = self.entity(target_id)?;
        self.native_ecs_valid_target(id, self.native_entity_target(entity)?)
            .then(|| self.living(entity))?
    }

    /// `Mob.setTarget`; None clears it. Only mobs and players whose binding
    /// can be stored in the current level are supported; armor stands are
    /// refused. The dragon stores a mob or a player, which its flight ignores
    /// and its save does not keep (vanilla keeps no target either). Brain-backed mobs refuse writes: vanilla's setter
    /// does not update their brain memory (arismoko/horizon#934). A fox, an enderman or a
    /// zombified piglin stores the target with its own `setTarget` side
    /// effects (`Mob::set_target`).
    /// Invalid targets are refused (false, mob unchanged); vanilla instead
    /// stores `asValidTarget(target)`, clearing the target when it is invalid.
    /// False also means a stale handle, client, or an unavailable mob/player.
    pub(super) fn mob_set_target(&mut self, mob: MobRef, target: Option<LivingRef>) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return self.native_dragon_set_target(mob.entity(), target);
        };
        if !self.ecs.exists(id) || self.ecs.has_brain(id) {
            return false;
        }
        if let Some(target) = target {
            let Some(target_kind) = self.native_entity_target(target.entity()) else {
                return false;
            };
            if !self.native_ecs_valid_target(id, target_kind) {
                return false;
            }
        }
        let Some(target) = self.native_mob_target_event(mob, target, false) else {
            return false;
        };
        if let Some(target) = target {
            let Some(kind) = self.native_entity_target(target.entity()) else {
                return false;
            };
            if !self.native_ecs_valid_target(id, kind) {
                return false;
            }
        }
        let Some(who) = self.native_mob_bind_target(id, target) else {
            return false;
        };
        self.native_store_target(id, who);
        true
    }

    /// Stores the target a mod set on the ECS mob `id`, its hook already run: an entity by its ECS entity, a player by its
    /// entity id. Unlike `ecs_mob_target`, it fires no event again.
    fn native_store_target(&mut self, id: u32, who: Option<Who>) -> Option<PWho> {
        let who = match who {
            None => None,
            Some(Who::Mob(t)) => self.ecs.entity_of(t).map(PWho::Entity),
            Some(Who::Player) => self
                .lan
                .as_ref()
                .and_then(|h| h.hub.guest(*h.mob_players.get(&id)?))
                .and_then(|g| u32::try_from(g.entity_id).ok())
                .map(PWho::Player),
        };
        self.ecs_with(|ecs, mw| ecs.set_target(mw, id, who));
        who
    }

    /// Dispatch a mod-initiated change while the mob remains in the game.
    pub(in crate::game) fn native_mob_target_event(
        &self,
        mob: MobRef,
        original_target: Option<LivingRef>,
        is_brain: bool,
    ) -> Option<Option<LivingRef>> {
        if !mod_api::__private::listened(mod_api::HookId::LivingChangeTarget) {
            return Some(original_target);
        }
        let mut new_target = original_target;
        let mut cancel = false;
        super::native_hooks::NativeHookHost { host: self }.native_living_change_target(
            mod_api::__private::LivingChangeTargetCall {
                mob,
                original_target,
                new_target: &mut new_target,
                is_brain,
                cancel: &mut cancel,
            },
        );
        (!cancel).then_some(new_target)
    }

    /// The `Who` mob `id` stores for `target`, for either target setter: a
    /// mob, or a player in the level in place. A player target binds to the
    /// mob (`mob_players`), so the mob's tick sees that player. None for a
    /// fake player (the root rule for every mob target setter), a player in
    /// another level, or anything else (an armor stand, the dragon, a stale
    /// handle).
    pub(super) fn native_mob_bind_target(
        &mut self,
        id: u32,
        target: Option<LivingRef>,
    ) -> Option<Option<Who>> {
        let (who, player) = match target {
            None => (None, None),
            Some(target) => match self.native_entity_target(target.entity())? {
                Target::Mob(id) => (Some(Who::Mob(id)), None),
                t @ Target::Guest(conn) if self.lan_acting_uuid.is_none() => {
                    self.native_player_facts(t)?;
                    let guest = self.lan.as_ref()?.hub.guest(conn)?;
                    if guest.dimension != self.dim.id() {
                        return None;
                    }
                    (Some(Who::Player), Some(conn))
                }
                t @ Target::Player => {
                    self.native_player_facts(t)?;
                    match self.lan_acting {
                        Some(conn) if self.lan.is_some() => (Some(Who::Player), Some(conn)),
                        _ => return None,
                    }
                }
                _ => return None,
            },
        };
        if let Some(conn) = player {
            self.lan.as_mut()?.mob_players.insert(id, conn);
        }
        Some(who)
    }

    /// `Brain.setMemory(ATTACK_TARGET, target)`, or `eraseMemory` for None:
    /// the target a brain-driven mob — an axolotl, a breeze, a creaking, a
    /// hoglin, a piglin or piglin brute, a warden or a zoglin — reads instead
    /// of `Mob.setTarget`'s (arismoko/horizon#934). A plain, non-expiring memory write: the
    /// brain's own behaviours act on it from the next tick (a target the
    /// species may not attack is dropped there, as vanilla's
    /// `StopAttackingIfTargetInvalid` drops it), and none of a species' own
    /// setter effects run — a warden's `setAttackTarget` roar, reach and
    /// sonic-boom bookkeeping stays with its own sensing. A frog refuses: its
    /// target belongs to the running tongue attack, which starts from the
    /// nearest attackable, so a written one would not be pursued.
    /// False for a fake player, a player in another level, a mob without an
    /// attack-target brain (`MobRef::set_target` writes those), a stale or
    /// unavailable mob, the dragon, or on a client.
    pub(super) fn mob_set_brain_target(&mut self, mob: MobRef, target: Option<LivingRef>) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if !self.ecs.exists(id) || !self.ecs.has_brain(id) {
            return false;
        }
        let Some(target) = self.native_mob_target_event(mob, target, true) else {
            return false;
        };
        let Some(who) = self.native_mob_bind_target(id, target) else {
            return false;
        };
        let who = self.native_store_target(id, who);
        self.ecs.set_brain_attack_target(id, who)
    }

    /// The mob flag a handle names: a mob's, or the dragon's own.
    fn native_mob_flag(&self, mob: MobRef, flag: MobFlag) -> Option<bool> {
        match self.native_entity_target(mob.entity())? {
            Target::Mob(id) if self.ecs.is_active() => {
                self.ecs_mod_live(id)?;
                match flag {
                    MobFlag::Aggressive => self.ecs.aggressive(id),
                    MobFlag::Persistent => self.ecs.requires_custom_persistence(id),
                }
            }
            _ => None,
        }
    }
    fn native_mob_set_flag(&mut self, mob: MobRef, flag: MobFlag, value: bool) -> bool {
        if let (true, Some(Target::Mob(id))) = (self.ecs.is_active(), self.native_entity_target(mob.entity())) {
            return self.ecs_mod_live(id).is_some()
                && match flag {
                    MobFlag::Aggressive => self.ecs.set_aggressive(id, value),
                    MobFlag::Persistent => self.ecs.set_persistence_required(id, value),
                };
        }
        false
    }

    /// `Mob.isAggressive`, the synced mob flag (the dragon's reaches its
    /// clients as `DATA_MOB_FLAGS_ID`).
    pub(super) fn mob_is_aggressive(&self, mob: MobRef) -> Option<bool> {
        self.native_mob_flag(mob, MobFlag::Aggressive)
    }

    /// `Mob.setAggressive`. False for an unavailable mob.
    pub(super) fn mob_set_aggressive(&mut self, mob: MobRef, value: bool) -> bool {
        self.native_mob_set_flag(mob, MobFlag::Aggressive, value)
    }

    /// `Mob.isPersistenceRequired`. The dragon never despawns
    /// (`EnderDragon.checkDespawn` is empty), but keeps and saves the flag
    /// (`PersistenceRequired`).
    pub(super) fn mob_persistence_required(&self, mob: MobRef) -> Option<bool> {
        self.native_mob_flag(mob, MobFlag::Persistent)
    }

    /// `Mob.setPersistenceRequired`, which sets the flag and cannot clear it.
    /// False for an unavailable mob.
    pub(super) fn mob_set_persistence_required(&mut self, mob: MobRef) -> bool {
        self.native_mob_set_flag(mob, MobFlag::Persistent, true)
    }

    /// `Mob.requiresCustomPersistence` and its species overrides: passengers,
    /// leashed mobs and species-specific reasons to stay, such as an enderman
    /// carrying a block. Independent of `PersistenceRequired` and `NoAI`.
    /// None for an unavailable mob or the separately modeled dragon.
    pub(super) fn mob_requires_custom_persistence(&self, mob: MobRef) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.requires_custom_persistence(id));
        }
        None
    }

    /// `Mob.isNoAi`: the mob stands where it is and its goals do not run,
    /// while what NoAI keeps running in vanilla (the daylight burn of the
    /// undead, a horse's tail) still does. Saved with it as `NoAI`. None
    /// for anything but a mob.
    pub(super) fn mob_no_ai(&self, mob: MobRef) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.no_ai(id));
        }
        None
    }

    /// `Mob.setNoAi`: whether the mob's own AI runs. With no AI the mob
    /// stands (what NoAI keeps in vanilla still runs), and the flag is
    /// saved with it as `NoAI`. False for anything but a mob.
    pub(super) fn mob_set_no_ai(&mut self, mob: MobRef, no_ai: bool) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.set_no_ai(id, no_ai);
        }
        false
    }

    /// `LivingEntity.getNoActionTime`: the ticks since the mob last had a
    /// reason to stay (a player near, a hit). Despawning and a raid's
    /// recruiting read it. None for a stale or unavailable mob, the dragon,
    /// or on a client.
    pub(super) fn mob_no_action_time(&self, mob: MobRef) -> Option<i32> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        self.ecs_mod_live(id).and(self.ecs.no_action_time(id))
    }

    /// `LivingEntity.setNoActionTime`: the count the mob's own tick carries
    /// on from (a far-away mob's despawn chance starts past 600). Not saved,
    /// as in vanilla. False for a stale or unavailable mob, the dragon, or
    /// on a client.
    pub(super) fn mob_set_no_action_time(&mut self, mob: MobRef, ticks: i32) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.set_no_action_time(id, ticks);
        }
        false
    }

    /// `Mob.setBaby` through the species' setter: ageable mobs use their
    /// baby start age (`-48000` for sniffers, otherwise `-24000`) or `0`,
    /// including age-boundary attribute and AI setup changes. Non-ageable
    /// babies, such as zombies, keep their non-growing baby state instead.
    /// Leaves `NoAI` and `PersistenceRequired` unchanged. False, with no
    /// changes, for an unavailable mob or a species with no baby form.
    pub(super) fn mob_set_baby(&mut self, mob: MobRef, baby: bool) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            // `Mob.setBaby` has a baby form for the ageable, the zombies, the piglin and the zoglin only.
            let has_baby_form = self.creature_species(id).is_some_and(|s| {
                use horizon_client::mobs::finalize::ageable;
                ageable(s) || matches!(s, Species::Zombie | Species::Husk | Species::Drowned | Species::ZombieVillager | Species::ZombifiedPiglin | Species::Piglin | Species::Zoglin)
            });
            return has_baby_form && self.ecs_mod_live(id).is_some() && self.ecs.set_baby(id, baby);
        }
        false
    }

    /// `LookControl.setLookAt`, by the mob's `lookAt` path: the wanted
    /// position its look control rotates the head toward over the next
    /// ticks. As `move_to`'s path, the mob's own AI — a goal's look, or a
    /// brain species' `LookAtTargetSink`, which writes through this same
    /// control — may issue its own and replace this one; no species refuses.
    /// A creaking being looked at, an angry bee and an axolotl playing dead
    /// hold their look control, as in vanilla: the wanted position waits
    /// until it runs again. False for non-finite coordinates, a stale or
    /// unavailable mob, a client, and the dragon.
    pub(super) fn mob_look_at(&mut self, mob: MobRef, at: [f64; 3]) -> bool {
        if at.iter().any(|v| !v.is_finite()) {
            return false;
        }
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.look_at(id, at);
        }
        false
    }

    /// `JumpControl.jump`: the mob jumps on its next own tick by its own
    /// jump path — no velocity is written, and the control consumes the
    /// wish on that tick (a rabbit's jump control reads it too). The mob's
    /// own wishes (a slime's hop, a swimming brain's) are the same flag; no
    /// species refuses. A creaking being looked at holds its jump control,
    /// as in vanilla: the wish waits until it runs again. False for a stale
    /// or unavailable mob, a client, and the dragon.
    pub(super) fn mob_jump(&mut self, mob: MobRef) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.jump(id);
        }
        false
    }

    /// `Mob.restrictTo(pos, radius)`: the home area the mob keeps to — its
    /// own stroll and wander goals pick their positions inside it, as a
    /// lead's and a guardian's home do. Radius `-1` clears the restriction
    /// while keeping `pos` as the center, `clear_restriction`'s shape. The
    /// area is saved with the mob (`home_pos`, `home_radius`). False for a
    /// stale or unavailable mob, a client, and the dragon.
    pub(super) fn mob_restrict_to(&mut self, mob: MobRef, at: [i32; 3], radius: i32) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.restrict_to(id, at, radius);
        }
        false
    }

    /// `Mob.getRestrictCenter`: the restriction's block position, kept
    /// after `clear_restriction` and after a load (`[0, 0, 0]` for a mob
    /// that never had one, vanilla's `ZERO`). None for a stale or
    /// unavailable mob, a client, and the dragon.
    pub(super) fn mob_restrict_center(&self, mob: MobRef) -> Option<[i32; 3]> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).map(|_| self.ecs.restrict_center(id).unwrap_or([0; 3]));
        }
        None
    }

    /// `Mob.getRestrictRadius`: `-1` while the mob has no restriction.
    /// None for a stale or unavailable mob, a client, and the dragon.
    pub(super) fn mob_restrict_radius(&self, mob: MobRef) -> Option<i32> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).map(|_| self.ecs.restrict_radius(id).unwrap_or(-1));
        }
        None
    }

    /// `Mob.hasRestriction`. None for a stale or unavailable mob, a client,
    /// and the dragon.
    pub(super) fn mob_has_restriction(&self, mob: MobRef) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.has_restriction(id));
        }
        None
    }

    /// `Mob.clearRestriction`: the restriction goes, the center stays (its
    /// own goals read no restriction, as vanilla's `-1` radius). False for
    /// a stale or unavailable mob, a client, and the dragon.
    pub(super) fn mob_clear_restriction(&mut self, mob: MobRef) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.clear_restriction(id);
        }
        false
    }

    /// `Mob.isWithinRestriction(pos)`: whether the block position is inside
    /// the home area; a mob without a restriction is within everywhere, as
    /// in vanilla. None for a stale or unavailable mob, a client, and the
    /// dragon.
    pub(super) fn mob_is_within_restriction(&self, mob: MobRef, at: [i32; 3]) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.is_within_restriction(id, at.map(|c| f64::from(c) + 0.5)));
        }
        None
    }

    /// `Mob.isLeftHanded`: whether the mob holds things in its left hand
    /// (the mob flags in its entity data). None for anything but a mob.
    pub(super) fn mob_left_handed(&self, mob: MobRef) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and_then(|_| self.ecs_flag(id, "LeftHanded"));
        }
        None
    }

    /// `Mob.setLeftHanded`: sets the mob flag the players around get with
    /// the mob's next entity data and see in its swing and held items.
    /// Saved with the mob as `LeftHanded`. False for anything but a mob.
    pub(super) fn mob_set_left_handed(&mut self, mob: MobRef, left_handed: bool) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs_put_flag(id, "LeftHanded", left_handed);
        }
        false
    }

    /// `Mob.canPickUpLoot`, with the overrides of an allay (holding
    /// something) and a sulfur cube (with no block). None for anything but
    /// a mob.
    pub(super) fn mob_can_pick_up_loot(&self, mob: MobRef) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(mob.entity())? else {
            return None;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).and(self.ecs.can_pick_up_loot(id));
        }
        None
    }

    /// `Mob.setCanPickUpLoot`: whether the mob picks up the items it sees
    /// (saved with it as `CanPickUpLoot`; clients are not told, as in
    /// vanilla). An allay and a sulfur cube decide for themselves. False
    /// for anything but a mob.
    pub(super) fn mob_set_can_pick_up_loot(&mut self, mob: MobRef, can_pick_up: bool) -> bool {
        let Some(Target::Mob(id)) = self.native_entity_target(mob.entity()) else {
            return false;
        };
        if self.ecs.is_active() {
            return self.ecs_mod_live(id).is_some() && self.ecs.set_can_pick_up_loot(id, can_pick_up);
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_entity_refs::tests::with_game;
    use super::*;
    use horizon_client::entities::ArmorStand;
    use horizon_core::geometry::Vec3;
    use mod_api::WorldEpoch;

    fn add_mob(game: &mut Game<'_>, id: u32, name: &str) -> MobRef {
        if !game.ecs.is_active() {
            game.ecs = game.test_ecs_level();
        }
        crate::game::ecs::test_support::place_test_mob(game, id, name, [3.5, 65.0, 3.5]).unwrap();
        game.mob(game.entity(id).unwrap()).unwrap()
    }

    #[test]
    #[ignore = "daily"]
    fn redirected_fake_target_is_refused_with_the_original_target_intact() {
        use std::sync::{Arc, Mutex};
        struct Redirect(Arc<Mutex<Option<LivingRef>>>);
        #[mod_api::__subscriptions]
        impl mod_api::Hooks for Redirect {
            fn living_change_target(
                &self,
                host: &dyn mod_api::Host,
                event: &mut mod_api::LivingChangeTargetEvent<'_>,
            ) -> mod_api::Flow {
                assert!(event.mob.living().health(host).is_some());
                *event.new_target = *self.0.lock().unwrap();
                mod_api::Flow::Continue
            }
        }
        let redirect = Arc::new(Mutex::new(None));
        let mods = vec![mod_api::ModEntry::new(
            "target-test",
            Box::new(Redirect(redirect.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            with_game(|game| {
                super::super::native_item_places::tests::open_to_lan(game);
                let level = game.level(game.dim.id()).unwrap();
                let fake = game.fake_player(level, 1234, "fake").unwrap();
                let mob = add_mob(game, 12, "minecraft:zombie");
                add_mob(game, 13, "minecraft:sheep");
                let existing = game.living(game.entity(13).unwrap()).unwrap();
                // The hook's redirect is the new target: the sheep is set through it first.
                *redirect.lock().unwrap() = Some(existing);
                assert!(mob.set_target(game, Some(existing)));
                *redirect.lock().unwrap() = Some(LivingRef::new(fake.entity()));
                assert!(!mob.set_target(game, None));
                assert_eq!(mob.target(game), Some(existing));
            })
        });
    }

    #[test]
    #[ignore = "daily"]
    fn mob_portal_eligibility_matches_gameplay() {
        with_game(|game| {
            let cow = add_mob(game, 12, "minecraft:cow");
            let wither = add_mob(game, 13, "minecraft:wither");
            let check = |game: &Game<'_>, handle: MobRef, expected| {
                let host: &dyn mod_api::Host = game;
                assert_eq!(handle.can_use_portal(host), Some(expected));
            };
            check(game, cow, true);
            check(game, wither, false);
            let ride = |game: &mut Game<'_>, on: bool| {
                let (cow_e, rider_e) = (game.ecs.entity_of(12).unwrap(), game.ecs.entity_of(13).unwrap());
                let w = game.ecs.ecs_mut().unwrap().world_mut();
                if on {
                    w.entity_mut(cow_e).insert(horizon_ecs_proto::mount::Riding(rider_e));
                } else {
                    w.entity_mut(cow_e).remove::<horizon_ecs_proto::mount::Riding>();
                }
            };
            ride(game, true);
            check(game, cow, false);
            ride(game, false);
            check(game, cow, true);
            let stale = MobRef::new(mod_api::EntityRef::new(WorldEpoch::new(999), 12));
            let host: &dyn mod_api::Host = game;
            assert_eq!(stale.can_use_portal(host), None);
            game.enter_command_level(super::super::Dim::Nether).unwrap();
            let host: &dyn mod_api::Host = game;
            assert_eq!(cow.can_use_portal(host), None);
            game.enter_command_level(super::super::Dim::Overworld)
                .unwrap();
            check(game, cow, true);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn mob_flags_baby_setter_refuses_no_baby_species_and_unavailable_mobs() {
        with_game(|game| {
            for (id, name) in [(12, "minecraft:skeleton"), (13, "minecraft:parrot")] {
                let mob = add_mob(game, id, name);
                assert!(mob.set_no_ai(game, true));
                assert!(mob.set_persistence_required(game));
                for baby in [true, false] {
                    assert!(!mob.set_baby(game, baby), "{name}");
                    assert_eq!(mob.living().baby(game), Some(false), "{name}");
                    assert_eq!(mob.no_ai(game), Some(true), "{name}");
                    assert_eq!(mob.persistence_required(game), Some(true), "{name}");
                }
            }
            let mob = add_mob(game, 14, "minecraft:sniffer");
            let stale = MobRef::new(EntityRef::new(
                WorldEpoch::new(mob.entity().epoch().get() + 1),
                14,
            ));
            assert_eq!(stale.requires_custom_persistence(game), None);
            assert!(!stale.set_baby(game, true));
            game.held_out.push(14);
            assert_eq!(mob.requires_custom_persistence(game), None);
            assert!(!mob.set_baby(game, true));
            game.held_out.pop();
            assert!(mob.set_baby(game, true));

            game.ecs_with(|ecs, mw| ecs.summon_dragon(mw, [0.0, 80.0, 0.0], 0.0, 0, horizon_ecs_proto::species::ender_dragon::START_PHASE, &horizon_ecs_proto::spawn_sheet::SpawnSheet::default(), &mut || 15));
            let dragon = game.mob(game.entity(15).unwrap()).unwrap();
            assert_eq!(dragon.requires_custom_persistence(game), None);
            assert!(!dragon.set_baby(game, true));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn player_target_round_trips_through_the_mob_binding() {
        use horizon_client::lan::{Joined, NetConfig, PlayerSender, guest::Guest, hub::HubConfig};
        use horizon_protocol::packets::configuration::ClientInformation;
        use std::sync::{Arc, atomic::AtomicI32, mpsc};

        with_game(|game| {
            let settings = super::super::lan::LanSettings {
                port: 0,
                guest_commands: false,
                force_game_mode: false,
            };
            let net = NetConfig {
                announce: false,
                ..NetConfig::lan(0, "MobRefTest".into(), 2)
            };
            let config = HubConfig {
                default_game_mode: 0,
                guest_commands: false,
                force_game_mode: false,
                hardcore: false,
                view_distance: 10,
                simulation_distance: 10,
                dimension: "minecraft:overworld".into(),
                allow_flight: false,
                seed: 0,
                max_players: 2,
                levels: Default::default(),
                level_seeds: Default::default(),
            };
            game.lan_host(settings, net, config).unwrap();
            let (tx, _rx) = mpsc::channel();
            let mut guest = Guest::new(
                Joined {
                    conn: 1,
                    uuid: 123,
                    name: "guest".into(),
                    properties: Vec::new(),
                    info: ClientInformation::new(8),
                    brand: String::new(),
                    sender: PlayerSender::new(tx, Arc::default()),
                    latency: Arc::new(AtomicI32::new(0)),
                    address: "127.0.0.1:1".parse().unwrap(),
                    expected: false,
                },
                20,
                [0.0; 3],
                8,
            );
            guest.placed = true;
            game.lan.as_mut().unwrap().hub.guests.push(guest);
            let mob = add_mob(game, 12, "minecraft:zombie");
            let player = game.living(game.entity(20).unwrap()).unwrap();
            assert!(mob.set_target(game, Some(player)));
            assert_eq!(mob.target(game), Some(player));
            assert_eq!(game.lan.as_ref().unwrap().mob_players.get(&12), Some(&1));
            // `ZombifiedPiglin.setTarget` does not impersonate a player hit:
            // the kill credit is unchanged.
            let piglin = add_mob(game, 13, "minecraft:zombified_piglin");
            assert!(piglin.set_target(game, Some(player)));
            assert!(game.ecs.credited_uuid(13).is_none());
            assert!(piglin.set_target(game, None));
            let level = game.level(game.dim.id()).unwrap();
            let fake = game.fake_player(level, 9999, "fake").unwrap();
            let fake_living = LivingRef::new(fake.entity());
            assert!(!piglin.set_target(game, Some(fake_living)));
            assert_eq!(piglin.target(game), None);
            // The alert recipient is allied with the host, not with the
            // unteamed guest targeted by the source piglin.
            let recipient = add_mob(game, 14, "minecraft:zombified_piglin");
            game.persist.uuids.insert(14, [0, 0, 0, 14]);
            let host_name = game.acting_player_name().to_owned();
            game.score.board.add_team("red");
            game.score.board.add_to_team(&host_name, "red");
            game.score
                .board
                .add_to_team("00000000-0000-0000-0000-00000000000e", "red");
            assert!(piglin.set_target(game, Some(player)));
            let alert = || {
                mobs::MobEvent::PiglinAlertTarget(mobs::undead::PiglinAlert {
                    mob: 14,
                    source: 13,
                    target: Who::Player,
                })
            };
            game.handle_mob_events(vec![alert()]).unwrap();
            assert_eq!(recipient.target(game), Some(player));
            assert!(recipient.set_target(game, None));
            game.score.board.add_to_team("guest", "red");
            game.handle_mob_events(vec![alert()]).unwrap();
            assert_eq!(recipient.target(game), None);
            {
                let value = 1;
                game.lan
                    .as_mut()
                    .unwrap()
                    .hub
                    .guests
                    .update(0, |guest| guest.game_mode = value);
            }
            assert!(!mob.set_target(game, Some(player)));
            assert_eq!(mob.target(game), None);
            {
                let value = 0;
                game.lan
                    .as_mut()
                    .unwrap()
                    .hub
                    .guests
                    .update(0, |guest| guest.game_mode = value);
            }
            {
                let value = "minecraft:the_nether".into();
                game.lan
                    .as_mut()
                    .unwrap()
                    .hub
                    .guests
                    .update(0, |guest| guest.dimension = value);
            }
            assert!(!mob.set_target(game, Some(player)));
            game.lan.as_mut().unwrap().mob_players.clear();
            assert_eq!(mob.target(game), None);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn brain_species_take_a_written_attack_target_memory() {
        with_game(|game| {
            for (id, name) in [
                (12, "minecraft:warden"),
                (13, "minecraft:breeze"),
                (14, "minecraft:axolotl"),
                (15, "minecraft:piglin"),
                (16, "minecraft:hoglin"),
                (17, "minecraft:creaking"),
                (18, "minecraft:zoglin"),
                (19, "minecraft:frog"),
                (20, "minecraft:zombie"),
                (21, "minecraft:iron_golem"),
            ] {
                add_mob(game, id, name);
            }
            let handle = |game: &Game<'_>, id| game.mob(game.entity(id).unwrap()).unwrap();
            let golem = game.living(game.entity(21).unwrap()).unwrap();
            // Each brain keeps the memory where its behaviours read it, and the
            // read handle answers it back; clearing erases it.
            for id in [12, 13, 14, 15, 16, 17, 18] {
                let mob = handle(game, id);
                assert!(mob.set_brain_target(game, Some(golem)), "mob {id}");
                assert_eq!(mob.target(game), Some(golem), "mob {id}");
                assert!(mob.set_brain_target(game, None), "mob {id}");
                assert_eq!(mob.target(game), None, "mob {id}");
            }
            // The brains act on it: a tick keeps a hoglin's and a breeze's
            // written target (their target-validity behaviours accept an iron
            // golem), as a memory their own sensing set would be kept.
            for (x, z) in (-1..=1).flat_map(|x| (-1..=1).map(move |z| (x, z))) {
                let mut chunk = horizon_client::chunk::Chunk::empty(x, z);
                chunk.lit = true;
                game.world.chunks.insert((x, z), std::sync::Arc::new(chunk));
            }
            for id in [13, 16] {
                assert!(handle(game, id).set_brain_target(game, Some(golem)));
            }
            game.tick_mobs().unwrap();
            for id in [13, 16] {
                assert_eq!(
                    handle(game, id).target(game),
                    Some(golem),
                    "mob {id} after a tick"
                );
            }
            // The frog's target belongs to its tongue attack; a mob without
            // an attack-target brain takes `set_target` instead.
            assert!(!handle(game, 19).set_brain_target(game, Some(golem)));
            assert!(!handle(game, 20).set_brain_target(game, Some(golem)));
            assert!(handle(game, 20).set_target(game, Some(golem)));
            // The ordinary setter still refuses the brain species.
            assert!(!handle(game, 13).set_target(game, Some(golem)));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn mob_handles_read_write_and_refuse_unavailable_entities() {
        with_game(|game| {
            add_mob(game, 12, "minecraft:zombie");
            add_mob(game, 13, "minecraft:zombie");
            crate::game::ecs::test_support::place_test_stand(game, 14, &ArmorStand::new(14, Vec3::ZERO, 0.0));
            // Reserve the dragon's eight consecutive part IDs away from the mobs.
            game.ecs_with(|ecs, mw| ecs.summon_dragon(mw, [0.0, 80.0, 0.0], 0.0, 0, horizon_ecs_proto::species::ender_dragon::START_PHASE, &horizon_ecs_proto::spawn_sheet::SpawnSheet::default(), &mut || 40));
            assert!(
                game.mob(game.entity(40).unwrap()).is_some(),
                "the dragon is a Mob"
            );
            let entity = game.entity(12).unwrap();
            let mob = game.mob(entity).unwrap();
            let other = game.living(game.entity(13).unwrap()).unwrap();
            let stand = game.living(game.entity(14).unwrap()).unwrap();
            for (id, name) in [
                (16, "minecraft:creaking"),
                (17, "minecraft:creeper"),
                (18, "minecraft:goat"),
                (19, "minecraft:warden"),
                (21, "minecraft:ghast"),
            ] {
                add_mob(game, id, name);
            }
            let brain = game.mob(game.entity(16).unwrap()).unwrap();
            assert!(brain.set_brain_target(game, Some(other)));
            assert_eq!(brain.target(game), Some(other));
            assert!(!brain.set_target(game, None));
            assert_eq!(brain.target(game), Some(other));
            let warden = game.mob(game.entity(19).unwrap()).unwrap();
            let ghast = game.living(game.entity(21).unwrap()).unwrap();
            assert!(warden.set_brain_target(game, Some(ghast)));
            assert_eq!(
                warden.target(game),
                Some(ghast),
                "warden can target a ghast, unlike Mob.canAttack"
            );
            assert!(!warden.set_target(game, None));
            let creeper = game.mob(game.entity(17).unwrap()).unwrap();
            let goat = game.living(game.entity(18).unwrap()).unwrap();
            assert!(creeper.set_target(game, Some(other)));
            assert!(!creeper.set_target(game, Some(goat)));
            assert_eq!(creeper.target(game), Some(other));
            assert!(game.mob(stand.entity()).is_none());
            assert_eq!(mob.living().entity(), entity);
            assert_eq!(game.mob_is_aggressive(mob), Some(false));
            assert!(game.mob_set_aggressive(mob, true));
            assert_eq!(game.mob_is_aggressive(mob), Some(true));
            assert_eq!(game.mob_persistence_required(mob), Some(false));
            assert!(game.mob_set_persistence_required(mob));
            assert_eq!(game.mob_persistence_required(mob), Some(true));
            assert!(mob.set_target(game, Some(other)));
            assert_eq!(mob.target(game), Some(other));
            assert!(!game.mob_set_target(mob, Some(stand)));
            assert_eq!(game.mob_target(mob), Some(other));
            let stale = MobRef::new(EntityRef::new(
                WorldEpoch::new(entity.epoch().get() + 1),
                12,
            ));
            assert!(game.mob(stale.entity()).is_none());
            assert!(!game.mob_set_target(stale, None));
            assert_eq!(game.mob_is_aggressive(stale), None);
            game.held_out.push(12);
            assert_eq!(game.mob_target(mob), None);
            assert!(!game.mob_set_target(mob, None));
            game.held_out.pop();
            assert_eq!(game.mob_target(mob), Some(other));
            assert!(game.mob_set_target(mob, None));
            assert_eq!(game.mob_target(mob), None);
        });
    }

    #[test]
    #[ignore = "daily"]
    fn a_restriction_round_trips_through_the_handle_and_saves() {
        use horizon_core::nbt::Tag;
        with_game(|game| {
            let mob = add_mob(game, 12, "minecraft:zombie");
            assert_eq!(game.mob_restrict_center(mob), Some([0, 0, 0]));
            assert_eq!(game.mob_restrict_radius(mob), Some(-1));
            assert_eq!(game.mob_has_restriction(mob), Some(false));
            assert_eq!(
                game.mob_is_within_restriction(mob, [900, 90, 900]),
                Some(true),
                "an unrestricted mob is within everywhere"
            );
            assert!(game.mob_restrict_to(mob, [10, 64, -5], 6));
            assert_eq!(game.mob_has_restriction(mob), Some(true));
            assert_eq!(game.mob_restrict_center(mob), Some([10, 64, -5]));
            assert_eq!(game.mob_restrict_radius(mob), Some(6));
            assert_eq!(
                game.mob_is_within_restriction(mob, [12, 64, -2]),
                Some(true)
            );
            assert_eq!(
                game.mob_is_within_restriction(mob, [20, 64, 0]),
                Some(false)
            );
            // The home area is saved with the mob.
            let saved = game.entity_nbt(mob.entity()).unwrap();
            assert_eq!(
                saved.get(&"home_pos".into()),
                Some(&Tag::IntArray(vec![10, 64, -5]))
            );
            assert_eq!(saved.get(&"home_radius".into()), Some(&Tag::Int(6)));
            // clearRestriction keeps the center; every position is within
            // again.
            assert!(game.mob_clear_restriction(mob));
            assert_eq!(game.mob_has_restriction(mob), Some(false));
            assert_eq!(game.mob_restrict_center(mob), Some([10, 64, -5]));
            assert_eq!(game.mob_restrict_radius(mob), Some(-1));
            assert_eq!(game.mob_is_within_restriction(mob, [20, 64, 0]), Some(true));
            let saved = game.entity_nbt(mob.entity()).unwrap();
            assert!(saved.get(&"home_pos".into()).is_none());
            assert!(saved.get(&"home_radius".into()).is_none());
            // restrictTo with radius -1 clears while keeping the new center.
            assert!(game.mob_restrict_to(mob, [1, 2, 3], -1));
            assert_eq!(game.mob_has_restriction(mob), Some(false));
            assert_eq!(game.mob_restrict_center(mob), Some([1, 2, 3]));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn an_extreme_restriction_answers_as_vanilla() {
        with_game(|game| {
            let mob = add_mob(game, 12, "minecraft:zombie");
            // `Vec3i.distSqr` in doubles: the far corner of the int range is
            // outside, with no overflow on the way.
            assert!(game.mob_restrict_to(mob, [i32::MIN, 0, 0], 6));
            assert_eq!(
                game.mob_is_within_restriction(mob, [i32::MAX, 0, 0]),
                Some(false)
            );
            assert!(game.mob_restrict_to(mob, [i32::MIN, i32::MIN, i32::MIN], 6));
            assert_eq!(
                game.mob_is_within_restriction(mob, [i32::MAX, i32::MAX, i32::MAX]),
                Some(false)
            );
            // The radius squares as a Java `int`: 65 536² wraps to 0, so not
            // even the center is within.
            assert!(game.mob_restrict_to(mob, [0, 0, 0], 65_536));
            assert_eq!(game.mob_is_within_restriction(mob, [0, 0, 0]), Some(false));
            // The largest radius that squares without wrapping still holds.
            assert!(game.mob_restrict_to(mob, [0, 0, 0], 46_340));
            assert_eq!(
                game.mob_is_within_restriction(mob, [46_339, 0, 0]),
                Some(true)
            );
            assert_eq!(
                game.mob_is_within_restriction(mob, [46_340, 0, 0]),
                Some(false)
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn hand_and_loot_flags_live_on_the_mob_handle() {
        use horizon_core::nbt::Tag;
        with_game(|game| {
            let mob = add_mob(game, 12, "minecraft:zombie");
            game.ecs_with(|ecs, mw| ecs.summon_dragon(mw, [0.0, 80.0, 0.0], 0.0, 0, horizon_ecs_proto::species::ender_dragon::START_PHASE, &horizon_ecs_proto::spawn_sheet::SpawnSheet::default(), &mut || 15));
            let entity = game.entity(12).unwrap();
            assert_eq!(game.mob_left_handed(mob), Some(false));
            assert!(game.mob_set_left_handed(mob, true));
            assert_eq!(game.mob_left_handed(mob), Some(true));
            assert!(game.mob_set_can_pick_up_loot(mob, true));
            assert_eq!(game.mob_can_pick_up_loot(mob), Some(true));
            // Saved as LeftHanded / CanPickUpLoot.
            let saved = game.entity_nbt(entity).unwrap();
            assert_eq!(saved.get(&"LeftHanded".into()), Some(&Tag::Byte(1)));
            assert_eq!(saved.get(&"CanPickUpLoot".into()), Some(&Tag::Byte(1)));
            assert!(game.mob_set_left_handed(mob, false));
            assert!(game.mob_set_can_pick_up_loot(mob, false));
            assert_eq!(game.mob_left_handed(mob), Some(false));
            assert_eq!(game.mob_can_pick_up_loot(mob), Some(false));
            // The dragon is not a Mob for these.
            let dragon = game.mob(game.entity(15).unwrap()).unwrap();
            assert_eq!(game.mob_left_handed(dragon), None);
            assert!(!game.mob_set_left_handed(dragon, true));
            assert_eq!(game.mob_can_pick_up_loot(dragon), None);
            assert!(!game.mob_set_can_pick_up_loot(dragon, true));
            assert!(!game.mob_look_at(dragon, [0.0; 3]));
            assert!(!game.mob_jump(dragon));
            assert!(!game.mob_restrict_to(dragon, [0, 0, 0], 5));
            assert_eq!(game.mob_has_restriction(dragon), None);
            // A stale handle changes no other entity.
            let stale = MobRef::new(EntityRef::new(
                WorldEpoch::new(entity.epoch().get() + 1),
                12,
            ));
            assert_eq!(game.mob_left_handed(stale), None);
            assert!(!game.mob_set_left_handed(stale, true));
            assert_eq!(game.mob_can_pick_up_loot(stale), None);
            assert!(!game.mob_set_can_pick_up_loot(stale, true));
            assert!(!game.mob_look_at(stale, [0.0; 3]));
            assert!(!game.mob_jump(stale));
            assert!(!game.mob_restrict_to(stale, [0, 0, 0], 5));
            assert!(!game.mob_clear_restriction(stale));
            assert_eq!(game.mob_restrict_center(stale), None);
            assert_eq!(game.mob_restrict_radius(stale), None);
            assert_eq!(game.mob_has_restriction(stale), None);
            assert_eq!(game.mob_is_within_restriction(stale, [0, 0, 0]), None);
        });
    }
}
