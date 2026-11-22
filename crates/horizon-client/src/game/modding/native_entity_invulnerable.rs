//! An entity's invulnerability for native server mods, through an
//! `EntityRef` (docs/modding/src/how-to/native-mods.md, "Entity and level handles"): the
//! `Invulnerable` flag (`Entity.permanentlyInvulnerable`) and
//! `LivingEntity.damageCooldownTime`, the ticks after a hit in which a
//! weaker one does only the difference. Horizon has no counterpart yet of
//! 26.3's own `Entity.invulnerableTime` timer, so neither is offered. Reads
//! return None and writes false for a stale handle, on a client, and for a
//! kind Horizon keeps no such state for.
use super::{Game, commands::Target};
use horizon_client::mobs::Damage;
use mod_api::EntityRef;

impl Game<'_> {
    /// Whether a named causing entity is a creative player.
    pub(in crate::game) fn creative_damage_source_id(&self, id: i32) -> bool {
        id != 0
            && ((id == self.own_entity_id() && self.creative)
                || self.lan.as_ref().is_some_and(|host| {
                    host.hub
                        .guests
                        .iter()
                        .any(|g| g.entity_id == id && g.creative())
                }))
    }
    /// Ask listeners about a nonliving entity damage source.
    pub(in crate::game) fn native_nonliving_invulnerable(
        &mut self,
        id: u32,
        kind: &str,
        source: horizon_client::health::HurtSource<'_>,
        vanilla: bool,
    ) -> bool {
        crate::game::entity::damage_level_of!(self, scoped).invulnerability_check(
            horizon_client::health::Hurting::Entity(id as i32, kind, source),
            vanilla,
        )
    }
    /// A player's body, where it is kept now.
    pub(super) fn native_player_body(
        &self,
        t: Target,
    ) -> Option<&horizon_client::motion::PlayerMotion> {
        match t {
            Target::Player => self.player.as_ref(),
            Target::Guest(conn) => self.lan.as_ref()?.hub.guest(conn)?.motion.as_deref(),
            _ => None,
        }
    }
    fn native_player_body_mut(
        &mut self,
        t: Target,
    ) -> Option<&mut horizon_client::motion::PlayerMotion> {
        match t {
            Target::Player => self.player.as_mut(),
            Target::Guest(conn) => self.lan.as_mut()?.hub.guests.motion_mut(conn),
            _ => None,
        }
    }

    /// `Entity.isPermanentlyInvulnerable`: the `Invulnerable` flag, which
    /// only damage that bypasses invulnerability (the void, `/kill`) or a
    /// creative player gets past. Mobs keep it on themselves (saved with
    /// them), players on their body (arismoko/horizon#916, not saved: Horizon's player save
    /// keeps no such field yet), the other kinds in the saved `Invulnerable`
    /// tag of their entity data (arismoko/horizon#916) — the tag the item damage path
    /// already refuses by.
    pub(super) fn entity_permanently_invulnerable(&self, entity: EntityRef) -> Option<bool> {
        match self.native_entity_target(entity)? {
            Target::Mob(id) if self.ecs_has(id) => self.ecs_flag(id, "Invulnerable"),
            t @ (Target::Player | Target::Guest(_)) => {
                Some(self.native_player_body(t)?.health.permanently_invulnerable)
            }
            t => Some(self.saved_invulnerable_flag(t)?),
        }
    }
    /// `Entity.setInvulnerable`: the write side of
    /// [`entity_permanently_invulnerable`](Self::entity_permanently_invulnerable):
    /// a mob's flag (saved with it), a player's body, the saved tag for the
    /// other kinds. False where the entity is gone.
    pub(super) fn entity_set_invulnerable(
        &mut self,
        entity: EntityRef,
        invulnerable: bool,
    ) -> bool {
        match self.native_entity_target(entity) {
            Some(Target::Mob(id)) if self.ecs_has(id) => self.ecs_put_flag(id, "Invulnerable", invulnerable),
            Some(t @ (Target::Player | Target::Guest(_))) => {
                self.native_player_body_mut(t).is_some_and(|p| {
                    p.health.permanently_invulnerable = invulnerable;
                    true
                })
            }
            Some(t) => self.set_saved_invulnerable_flag(t, invulnerable),
            None => false,
        }
    }

    /// The saved `Invulnerable` tag of a non-mob entity's data (arismoko/horizon#916):
    /// `false` where its entity data has none. None for a kind that keeps
    /// none (a frame, a painting, a fang).
    fn saved_invulnerable_flag(&self, t: Target) -> Option<bool> {
        let id = match t {
            Target::Stand(id) | Target::Dragon(id) | Target::Item(id) => id,
            _ => return None,
        };
        Some(self.saved_invulnerability(id).0)
    }
    /// Writes the `Invulnerable` tag into a non-mob entity's saved data
    /// (arismoko/horizon#916), the compound its own save builds on.
    fn set_saved_invulnerable_flag(&mut self, t: Target, invulnerable: bool) -> bool {
        use horizon_core::nbt::Tag;
        let id = match t {
            Target::Stand(id) | Target::Dragon(id) | Target::Item(id) => id,
            _ => return false,
        };
        let mut compound = match self.persist.raw.get(&id) {
            Some(Tag::Compound(map)) => map.clone(),
            _ => std::collections::BTreeMap::new(),
        };
        compound.insert("Invulnerable".into(), Tag::Byte(i8::from(invulnerable)));
        self.persist.raw.insert(id, Tag::Compound(compound));
        true
    }

    /// `Entity.invulnerableTime` (arismoko/horizon#916): the entity-level invulnerability
    /// window, its own field on a mob and a player — never the living damage
    /// cooldown — and the saved `invulnerable_time` tag for the other kinds.
    /// Counted down by the base tick; `isTemporarilyInvulnerable` is it above
    /// zero. None for a stale handle or a kind that keeps none.
    pub(super) fn entity_invulnerable_time(&self, entity: EntityRef) -> Option<i32> {
        match self.native_entity_target(entity)? {
            // The ECS mob's window is read by the bridge (`invulnerable_time`, listed in the report).
            Target::Mob(id) if self.ecs_has(id) => self.ecs.invulnerable_time(id),
            t @ (Target::Player | Target::Guest(_)) => {
                Some(self.native_player_body(t)?.health.invulnerable_time)
            }
            t => self.saved_invulnerable_time(t),
        }
    }
    /// `Entity.setInvulnerableTime` (arismoko/horizon#916): the write side of
    /// [`native_entity_invulnerable_time`](Self::native_entity_invulnerable_time).
    /// False where the entity is gone or keeps no such field.
    pub(super) fn entity_set_invulnerable_time(&mut self, entity: EntityRef, ticks: i32) -> bool {
        let ticks = ticks.max(0);
        match self.native_entity_target(entity) {
            Some(Target::Mob(id)) if self.ecs_has(id) => self.ecs.set_invulnerable_time(id, ticks),
            Some(t @ (Target::Player | Target::Guest(_))) => {
                self.native_player_body_mut(t).is_some_and(|p| {
                    p.health.invulnerable_time = ticks;
                    true
                })
            }
            Some(t) => self.set_saved_invulnerable_time(t, ticks),
            None => false,
        }
    }

    /// A non-mob kind's saved invulnerability (arismoko/horizon#916), the one parse of its
    /// entity data: the `Invulnerable` flag and the `invulnerable_time`
    /// window. `(false, 0)` where its data has neither.
    fn saved_invulnerability(&self, id: u32) -> (bool, i32) {
        use horizon_core::nbt::{JavaString, Tag};
        use std::sync::LazyLock;
        // The two keys, built once: a `JavaString` is UTF-16 in a `Vec`, and building one per
        // lookup allocated twice for every armor stand, item and dragon, every tick.
        static FLAG: LazyLock<JavaString> = LazyLock::new(|| "Invulnerable".into());
        static WINDOW: LazyLock<JavaString> = LazyLock::new(|| "invulnerable_time".into());
        match self.persist.raw.get(&id) {
            Some(Tag::Compound(map)) => (
                matches!(map.get(&*FLAG), Some(Tag::Byte(1))),
                match map.get(&*WINDOW) {
                    Some(Tag::Int(ticks)) => *ticks,
                    _ => 0,
                },
            ),
            _ => (false, 0),
        }
    }
    /// The saved `invulnerable_time` tag of a non-mob entity's data (arismoko/horizon#916):
    /// 0 where its entity data has none. None for a kind that keeps none.
    fn saved_invulnerable_time(&self, t: Target) -> Option<i32> {
        let id = match t {
            Target::Stand(id) | Target::Dragon(id) | Target::Item(id) => id,
            _ => return None,
        };
        Some(self.saved_invulnerability(id).1)
    }
    /// `Entity.isInvulnerableToBase` over a non-mob kind's saved flag and
    /// window (arismoko/horizon#916) — the one place that refusal is decided: past only for
    /// `#bypasses_invulnerability` damage and a creative player's hit
    /// (`isCreativePlayer`). False where the saved state has neither.
    pub(in crate::game) fn saved_refuses_damage(
        &self,
        id: u32,
        damage_type: &str,
        creative_source: bool,
    ) -> bool {
        let (flag, window) = self.saved_invulnerability(id);
        if !flag && window <= 0 {
            return false;
        }
        let registry = &self.health_resources.damage;
        let bypass = registry
            .id(damage_type)
            .is_some_and(|kind| registry.in_tag(kind, "minecraft:bypasses_invulnerability"));
        !(bypass || creative_source)
    }
    /// `DamageSource.isCreativePlayer` (arismoko/horizon#916): the causing player's creative
    /// mode, the host's own or a guest's; false for anything else. The one
    /// decision every invulnerability gate shares with `/damage`'s and the
    /// dragon's.
    pub(in crate::game) fn creative_source_of(&self, cause: Option<Target>) -> bool {
        match cause {
            Some(Target::Player) => self.creative,
            Some(Target::Guest(conn)) => self.command_guest(conn).is_some_and(|g| g.creative()),
            _ => false,
        }
    }
    /// Writes the `invulnerable_time` tag into a non-mob entity's saved data
    /// (arismoko/horizon#916).
    fn set_saved_invulnerable_time(&mut self, t: Target, ticks: i32) -> bool {
        use horizon_core::nbt::Tag;
        let id = match t {
            Target::Stand(id) | Target::Dragon(id) | Target::Item(id) => id,
            _ => return false,
        };
        let mut compound = match self.persist.raw.get(&id) {
            Some(Tag::Compound(map)) => map.clone(),
            _ => std::collections::BTreeMap::new(),
        };
        compound.insert("invulnerable_time".into(), Tag::Int(ticks.max(0)));
        self.persist.raw.insert(id, Tag::Compound(compound));
        true
    }
    /// `Entity.baseTick`'s countdown of a non-mob kind's saved
    /// `invulnerable_time` (arismoko/horizon#916): one less each tick while it runs, so the
    /// window an entity carries in its saved data closes like a mob's and a
    /// player's do.
    pub(in crate::game) fn tick_saved_invulnerable_time(&mut self, id: u32) {
        use horizon_core::nbt::Tag;
        let ticks = self.saved_invulnerability(id).1;
        if ticks <= 0 {
            return;
        }
        let ticks = ticks - 1;
        let mut compound = match self.persist.raw.get(&id) {
            Some(Tag::Compound(map)) => map.clone(),
            _ => std::collections::BTreeMap::new(),
        };
        compound.insert("invulnerable_time".into(), Tag::Int(ticks));
        self.persist.raw.insert(id, Tag::Compound(compound));
    }

    /// `Entity.hurtServer` of `amount` from a source of `damage_type`
    /// (`minecraft:generic`'s shape), `direct` the entity that dealt it (a
    /// shot arrow) and `causing` the one responsible for it (its shooter):
    /// the engine's own hurt paths take it — the damage type, the attacker,
    /// armor, the cooldown and every hurt hook kept, the damage event the
    /// hit already broadcasts — by kind: the player's and mobs' for the
    /// living, the item, dragon and crystal branches for the damageable
    /// rest. True when the damage was taken. None for a stale handle, false
    /// for a kind that cannot be hurt or an unregistered damage type. Stale
    /// direct/causing handles are treated as absent sources.
    pub(super) fn entity_hurt(
        &mut self,
        entity: EntityRef,
        damage_type: &str,
        amount: f32,
        direct: Option<EntityRef>,
        causing: Option<EntityRef>,
    ) -> Option<bool> {
        let target = self.native_entity_target(entity)?;
        let direct = direct.and_then(|e| self.native_entity_target(e));
        let causing = causing.and_then(|e| self.native_entity_target(e));
        // `DamageSource(type, cause, at)`: the source position the knockback
        // pushes away from is the direct entity's.
        let from = direct.and_then(|t| self.target_position(t));
        Some(self.hurt_target(target, amount, damage_type, direct, causing, from))
    }

    /// `Entity.isInvulnerableTo` for a source of `damage_type`: arismoko/horizon#842's own
    /// decision, the guard every mob damage hook runs after — dead or
    /// removed, fire immunity, or the `Invulnerable` flag nothing but
    /// `#bypasses_invulnerability` gets past. None for a stale handle or a
    /// kind with no such decision (the player's paths answer their refusals
    /// at hurt time).
    pub(super) fn entity_invulnerable_to_damage(
        &self,
        entity: EntityRef,
        damage_type: &str,
    ) -> Option<bool> {
        let Target::Mob(id) = self.native_entity_target(entity)? else {
            return None;
        };
        if self.ecs_has(id) {
            // `isInvulnerableToBase`: the permanent flag or the window refuses what is not `#bypasses_invulnerability`.
            use horizon_client::mobs::{DamageKind as K, NamedDamage};
            let bypass = match Self::damage_kind(&self.health_resources.damage, damage_type) {
                K::OutOfWorld | K::GenericKill => true,
                K::Named(n) => n.tags & NamedDamage::BYPASSES_INVULNERABILITY != 0,
                _ => false,
            };
            let held = self.ecs_flag(id, "Invulnerable")? || self.ecs.invulnerable_time(id)? > 0;
            return Some(held && !bypass);
        }
        None
    }

    /// `Entity.isInvulnerableTo` asked of `mob` outside a hit, for a source of `damage_type`:
    /// the vanilla answer, which `entity_invulnerability_check` listeners may change.
    pub(in crate::game) fn mob_query_invulnerable(
        &self,
        mob: &horizon_client::mobs::Mob,
        damage_type: &str,
    ) -> bool {
        let source = Damage::of(Self::damage_kind(
            &self.health_resources.damage,
            damage_type,
        ));
        let vanilla = mob.refuses_damage(&source);
        horizon_client::health::query_invulnerability(
            &self.world,
            self.pack.assets.registry,
            Some(world_items!(self)),
            Some(&self.pack.block_registries),
            &self.damage_hooks,
            horizon_client::health::Hurting::Mob(mob, &source),
            vanilla,
        )
    }

    /// Whether `source`'s damage may hit `entity` through their teams
    /// (`Player.canHarmPlayer`, `Team.allowFriendlyFire`,
    /// `DamageSource.doTeamsAllowDamage`): allies without friendly fire
    /// cannot. Entities without a team always can. None for a stale handle.
    pub(super) fn entity_entities_team_damage_allowed(
        &self,
        entity: EntityRef,
        source: EntityRef,
    ) -> Option<bool> {
        let own = self.native_target_team(self.native_entity_target(entity)?)?;
        let their = self.native_target_team(self.native_entity_target(source)?)?;
        Some(horizon_client::scoreboard::teams_allow_damage(own, their))
    }

    /// The team an entity is on: a mob's own or its tamed owner's, the
    /// acting player's, a guest's by name.
    pub(super) fn native_target_team(
        &self,
        t: Target,
    ) -> Option<Option<&horizon_client_core::scoreboard::Team>> {
        Some(match t {
            Target::Player => self.player_team(),
            Target::Guest(conn) => self
                .score
                .board
                .players_team(&self.command_guest(conn)?.name),
            Target::Mob(id) => self.mob_team(id),
            _ => None,
        })
    }

    /// `Entity.isInvulnerableToPiercingWeapon` (`isInvulnerable`): the
    /// `Entity.isInvulnerable()`: the `Invulnerable` flag or the window
    /// `Entity.invulnerableTime` keeps open — a mob's own fields, a player's
    /// health, the saved tags for the other kinds (arismoko/horizon#916). None for a stale
    /// handle.
    pub(super) fn entity_invulnerable_to_piercing_weapon(&self, entity: EntityRef) -> Option<bool> {
        match self.native_entity_target(entity)? {
            Target::Mob(id) if self.ecs_has(id) => {
                let window = self.ecs.invulnerable_time(id).unwrap_or(0);
                Some(self.ecs_flag(id, "Invulnerable")? || window > 0)
            }
            t @ (Target::Player | Target::Guest(_)) => {
                let p = self.native_player_body(t)?;
                Some(p.health.permanently_invulnerable || p.health.invulnerable_time > 0)
            }
            t => {
                let id = match t {
                    Target::Stand(id) | Target::Dragon(id) | Target::Item(id) => id,
                    _ => return None,
                };
                let (flag, window) = self.saved_invulnerability(id);
                Some(flag || window > 0)
            }
        }
    }

    /// `LivingEntity.damageCooldownTime`: 20 after a hit, counting down;
    /// while it is above 10 a hit does only what it deals beyond the last
    /// one. Mobs and players.
    pub(super) fn entity_damage_cooldown_time(&self, entity: EntityRef) -> Option<i32> {
        match self.native_entity_target(entity)? {
            Target::Mob(id) if self.ecs_has(id) => self.ecs.damage_cooldown(id),
            t => Some(self.native_player_body(t)?.health.hurt.cooldown),
        }
    }
    /// Sets `LivingEntity.damageCooldownTime`, as mods do to let rapid hits
    /// through (0). Mobs and players.
    pub(super) fn entity_set_damage_cooldown_time(
        &mut self,
        entity: EntityRef,
        ticks: i32,
    ) -> bool {
        let Some(t) = self.native_entity_target(entity) else {
            return false;
        };
        match t {
            Target::Mob(id) if self.ecs_has(id) => self.ecs.set_damage_cooldown(id, ticks),
            t => match self.native_player_body_mut(t) {
                Some(p) => {
                    p.health.hurt.cooldown = ticks;
                    true
                }
                None => false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::native_entity_refs::tests::with_game;
    use horizon_core::geometry::Vec3;

    /// arismoko/horizon#916: the permanent flag reaches players and non-mob kinds, and a
    /// player's damage is really refused by it.
    #[test]
    #[ignore = "daily"]
    fn the_flag_reaches_players_and_non_mobs_and_refuses_player_damage() {
        use horizon_client::motion::PlayerMotion;
        super::super::native_entity_refs::tests::with_game(|game| {
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            crate::game::native_entity_refs::tests::bind_local_player(game, 20, 123);
            let player = game.entity(20).unwrap();
            crate::game::ecs::test_support::place_test_stand(game, 14, &horizon_client::entities::ArmorStand::new(
                    14,
                    Vec3::ZERO,
                    0.0,
                ));
            let stand = game.entity(14).unwrap();

            // A player's flag, on their body, refusing their damage — and
            // `#bypasses_invulnerability` still getting past.
            assert_eq!(game.entity_permanently_invulnerable(player), Some(false));
            assert!(game.entity_set_invulnerable(player, true));
            assert_eq!(game.entity_permanently_invulnerable(player), Some(true));
            game.hurt_player_by("minecraft:generic", 5.0);
            assert_eq!(
                game.player.as_ref().unwrap().health.health(),
                20.0,
                "the flag refused it"
            );
            game.hurt_player_by("minecraft:out_of_world", 100.0);
            assert!(
                game.player.as_ref().unwrap().health.health() <= 0.0,
                "out of world gets past the flag"
            );

            // A stand without saved data reads false, and the write lands in
            // the saved `Invulnerable` tag the item damage path refuses by.
            assert_eq!(game.entity_permanently_invulnerable(stand), Some(false));
            assert!(game.entity_set_invulnerable(stand, true));
            assert_eq!(game.entity_permanently_invulnerable(stand), Some(true));
            let raw = game.persist.raw.get(&14).cloned().unwrap();
            assert!(horizon_client::entity_nbt::flag(&raw, "Invulnerable"));
            // The timer tag rides along the same compound.
            assert!(game.entity_set_invulnerable_time(stand, 12));
            assert_eq!(game.entity_invulnerable_time(stand), Some(12));
            let raw = game.persist.raw.get(&14).cloned().unwrap();
            assert!(matches!(
                raw.get("invulnerable_time"),
                Some(horizon_core::nbt::Tag::Int(12))
            ));
        });
    }

    /// arismoko/horizon#916, review round 2: `Entity.setInvulnerable` on a player is the
    /// permanent flag, apart from `Abilities.invulnerable` — the abilities
    /// stay untouched, a survival hit is refused and a creative player's
    /// passes the flag (`isInvulnerableToBase`'s one exception), while the
    /// ability gate (a creative player's own mode) holds even a creative
    /// hit back, as vanilla `Player.hurtServer` does.
    #[test]
    #[ignore = "daily"]
    fn the_players_permanent_flag_is_apart_from_their_abilities() {
        use horizon_client::motion::PlayerMotion;
        super::super::native_entity_refs::tests::with_game(|game| {
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            crate::game::native_entity_refs::tests::bind_local_player(game, 20, 123);
            let player = game.entity(20).unwrap();

            // The window refuses an ordinary hit; a creative player's
            // `player_attack` passes it (`DamageSource.isCreativePlayer`).
            assert!(game.entity_set_invulnerable_time(player, 60));
            game.mob_hits_player(5.0, [0.0; 3], "minecraft:player_attack");
            assert_eq!(
                game.player.as_ref().unwrap().health.health(),
                20.0,
                "the window refused it"
            );
            game.mob_hits_player_from(5.0, [0.0; 3], "minecraft:player_attack", true);
            assert_eq!(
                game.player.as_ref().unwrap().health.health(),
                15.0,
                "a creative player's hit passes the window"
            );
            game.player.as_mut().unwrap().health.invulnerable_time = 0;
            // The landed hit started the hurt cooldown: a later hit must
            // out-damage it to land at all (`LivingEntity.hurtServer`).
            game.player.as_mut().unwrap().health.hurt.cooldown = 0;
            game.player.as_mut().unwrap().health.hurt.last_hurt = 0.0;

            // The permanent flag is not the abilities: set_invulnerable
            // leaves `Abilities.invulnerable` alone.
            assert!(game.entity_set_invulnerable(player, true));
            assert!(
                !game.player.as_ref().unwrap().invulnerable,
                "the abilities are untouched"
            );
            game.mob_hits_player(5.0, [0.0; 3], "minecraft:player_attack");
            assert_eq!(
                game.player.as_ref().unwrap().health.health(),
                15.0,
                "the flag refused the survival hit"
            );
            game.mob_hits_player_from(5.0, [0.0; 3], "minecraft:player_attack", true);
            assert_eq!(
                game.player.as_ref().unwrap().health.health(),
                10.0,
                "and a creative player's hit passes the flag"
            );
            game.player.as_mut().unwrap().health.hurt.cooldown = 0;
            game.player.as_mut().unwrap().health.hurt.last_hurt = 0.0;

            // The ability flag is another gate (`Player.hurtServer`): even a
            // creative player's hit does not pass it, as in vanilla.
            game.player.as_mut().unwrap().invulnerable = true;
            game.mob_hits_player_from(5.0, [0.0; 3], "minecraft:player_attack", true);
            assert_eq!(
                game.player.as_ref().unwrap().health.health(),
                10.0,
                "the ability flag holds even a creative hit back"
            );
        });
    }

    /// The saved flag and window read with the keys built once are the ones read with the keys
    /// built per lookup, for every shape of saved data a non-mob can hold.
    #[test]
    #[ignore = "daily"]
    fn saved_invulnerability_matches_a_key_built_per_lookup() {
        use horizon_core::nbt::Tag;
        super::super::native_entity_refs::tests::with_game(|game| {
            let shapes: Vec<Option<Tag>> = {
                let mut v = vec![None, Some(Tag::Int(3)), Some(Tag::Compound(Default::default()))];
                for flag in [None, Some(Tag::Byte(1)), Some(Tag::Byte(0)), Some(Tag::Int(1))] {
                    for window in [None, Some(Tag::Int(7)), Some(Tag::Int(-2)), Some(Tag::Byte(5))] {
                        let mut map = std::collections::BTreeMap::new();
                        if let Some(f) = &flag {
                            map.insert("Invulnerable".into(), f.clone());
                        }
                        if let Some(w) = &window {
                            map.insert("invulnerable_time".into(), w.clone());
                        }
                        map.insert("Other".into(), Tag::Int(1));
                        v.push(Some(Tag::Compound(map)));
                    }
                }
                v
            };
            for (i, shape) in shapes.into_iter().enumerate() {
                let id = 100 + i as u32;
                match shape.clone() {
                    Some(tag) => {
                        game.persist.raw.insert(id, tag);
                    }
                    None => {
                        game.persist.raw.remove(&id);
                    }
                }
                let new = game.saved_invulnerability(id);
                let old = match &shape {
                    Some(Tag::Compound(map)) => (
                        matches!(map.get(&"Invulnerable".into()), Some(Tag::Byte(1))),
                        match map.get(&"invulnerable_time".into()) {
                            Some(Tag::Int(ticks)) => *ticks,
                            _ => 0,
                        },
                    ),
                    _ => (false, 0),
                };
                assert_eq!(new, old, "shape {i}");
            }
        });
    }

    /// arismoko/horizon#916, review: a non-mob's saved window counts down each tick, and the
    /// item damage path refuses by it.
    #[test]
    #[ignore = "daily"]
    fn a_non_mobs_saved_window_counts_down_and_refuses() {
        use horizon_client::entities::ArmorStand;
        use horizon_core::geometry::Vec3;
        super::super::native_entity_refs::tests::with_game(|game| {
            crate::game::ecs::test_support::place_test_stand(game, 14, &ArmorStand::new(14, Vec3::new(0.5, 64.0, 0.5), 0.0));
            let stand = game.entity(14).unwrap();
            assert!(game.entity_set_invulnerable_time(stand, 20));
            // Entity.baseTick, over the saved tag.
            game.tick_saved_invulnerable_time(14);
            game.tick_saved_invulnerable_time(14);
            assert_eq!(game.entity_invulnerable_time(stand), Some(18));
            // Down to zero it stays, and reads as no window.
            for _ in 0..18 {
                game.tick_saved_invulnerable_time(14);
            }
            assert_eq!(game.entity_invulnerable_time(stand), Some(0));
        });
    }

    /// arismoko/horizon#916, coderabbit: an item outside the entity-ticking chunks does not
    /// tick, so its saved window does not count down in that pass — while
    /// one the pass reaches does.
    #[test]
    #[ignore = "daily"]
    fn an_item_outside_the_ticking_chunks_keeps_its_window() {
        use horizon_client::items::ItemStack;
        use horizon_client::motion::PlayerMotion;
        with_game(|game| {
            // A player's tickets: chunks out of the simulation distance
            // around them do not tick entities.
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            game.update_chunk_tickets();
            let stack = || ItemStack {
                item: "minecraft:stone".into(),
                count: 1,
                damage: 0,
                extra: None,
            };
            game.spawn_item(stack(), [0.5, 64.0, 0.5], [0.0; 3], 10);
            game.spawn_item(stack(), [100000.5, 64.0, 0.5], [0.0; 3], 10);
            let near = game.entity(game.ecs.item_ids()[0]).unwrap();
            let far = game.entity(game.ecs.item_ids()[1]).unwrap();
            assert!(game.entity_set_invulnerable_time(near, 5));
            assert!(game.entity_set_invulnerable_time(far, 5));
            game.tick_items();
            assert_eq!(game.entity_invulnerable_time(near), Some(4));
            assert_eq!(
                game.entity_invulnerable_time(far),
                Some(5),
                "outside the ticking chunks, the window holds"
            );
        });
    }

    /// arismoko/horizon#916, review round 2: a stand the flag or the window covers refuses a
    /// survival punch — no wobble, no break — while a creative player's hit
    /// passes both, as `DamageSource.isCreativePlayer` does.
    #[test]
    #[ignore = "daily"]
    fn a_saved_stand_refuses_the_punch_that_would_break_it() {
        use horizon_client::entities::ArmorStand;
        with_game(|game| {
            crate::game::ecs::test_support::place_test_stand(game, 14, &ArmorStand::new(14, Vec3::new(0.5, 64.0, 0.5), 0.0));
            let stand = game.entity(14).unwrap();
            // The window refuses even the first hit's wobble.
            assert!(game.entity_set_invulnerable_time(stand, 5));
            assert!(!game.punch_armor_stand(0));
            assert_eq!(game.armor_stand_list().len(), 1);
            // The flag holds the breaking second hit back in survival.
            assert!(game.entity_set_invulnerable(stand, true));
            assert!(!game.punch_armor_stand(0));
            assert!(!game.punch_armor_stand(0));
            assert_eq!(game.armor_stand_list().len(), 1, "still standing");
            // A creative player's hit passes the flag.
            game.creative = true;
            assert!(game.punch_armor_stand(0));
            assert_eq!(game.armor_stand_list().len(), 0);
        });
    }

    /// arismoko/horizon#916, review round 2: the player's flag and window ride the save —
    /// `sync_level` writes them, `load_saved_state_once` reads them back.
    #[test]
    #[ignore = "daily"]
    fn the_players_flag_and_window_ride_the_save() {
        use horizon_client::motion::PlayerMotion;
        use horizon_core::nbt::Tag;
        with_game(|game| {
            game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
            crate::game::native_entity_refs::tests::bind_local_player(game, 20, 123);
            let player = game.entity(20).unwrap();
            assert!(game.entity_set_invulnerable(player, true));
            assert!(game.entity_set_invulnerable_time(player, 7));
            // Entity.saveWithoutId: the flag always, the window while open.
            game.sync_level();
            let saved = game.level.player_extra.clone().unwrap();
            assert!(matches!(saved.get("Invulnerable"), Some(Tag::Byte(1))));
            assert!(matches!(saved.get("invulnerable_time"), Some(Tag::Int(7))));
            // A later load restores both onto the body.
            game.player
                .as_mut()
                .unwrap()
                .health
                .permanently_invulnerable = false;
            game.player.as_mut().unwrap().health.invulnerable_time = 0;
            let mut pending = true;
            game.player
                .as_mut()
                .unwrap()
                .load_saved_state_once(&mut pending, &game.level)
                .unwrap();
            assert!(
                game.player
                    .as_ref()
                    .unwrap()
                    .health
                    .permanently_invulnerable
            );
            assert_eq!(game.entity_invulnerable_time(player), Some(7));
        });
    }

    /// arismoko/horizon#916, review round 2: the dragon's saved flag refuses `/damage`
    /// except from a creative player — the one refusal every saved kind
    /// shares (`saved_refuses_damage`).
    #[test]
    #[ignore = "daily"]
    fn the_dragons_saved_flag_refuses_all_but_a_creative_player() {
        use horizon_client::mobs::DamageKind;
        super::super::native_entity_refs::tests::with_game(|game| {
            game.ecs_with(|ecs, mw| ecs.summon_dragon(mw, [0.0, 80.0, 0.0], 0.0, 0, horizon_ecs_proto::species::ender_dragon::START_PHASE, &mut || 15));
            let entity = game.entity(15).unwrap();
            assert!(game.entity_set_invulnerable(entity, true));

            // Match cmd_damage's normalized direct and causing player source.
            let source = (
                Some(super::Target::Player),
                Some(super::Target::Player),
                None,
            );
            let before = game.ecs_living_health(15).unwrap();
            // A survival player's blow: refused.
            game.creative = false;
            assert!(!game.command_damage_dragon(
                15,
                10.0,
                "minecraft:generic",
                DamageKind::Generic,
                source,
            ));
            assert_eq!(game.ecs_living_health(15), Some(before));
            // A creative player's blow passes the flag (`isCreativePlayer`).
            game.creative = true;
            assert!(game.command_damage_dragon(
                15,
                10.0,
                "minecraft:generic",
                DamageKind::Generic,
                source,
            ));
            assert!(game.ecs_living_health(15).unwrap() < before);
            // And the void needs no creative help (`#bypasses_invulnerability`).
            game.creative = false;
            assert!(game.command_damage_dragon(
                15,
                1000.0,
                "minecraft:out_of_world",
                DamageKind::OutOfWorld,
                source,
            ));
        });
    }
}
