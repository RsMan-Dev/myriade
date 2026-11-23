use super::*;

impl Game<'_> {
    pub(in crate::game) fn guest_respawn(&mut self, conn: u32) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let Some(i) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        // The guest's own bed, else the world spawn: never the host's bed.
        let world = self.world_spawn_position();
        let spawn = host.hub.guests[i].spawn.unwrap_or(world);
        if !host.hub.guests[i].dead {
            return;
        }
        // PlayerList.respawn removes the dead player (Player.remove).
        self.guest_drop_carried(i);
        // AbstractContainerScreen.tick / ServerPlayer.closeContainer: the dead
        // player's open container goes with it, so a chest's lid (its opener
        // count) falls instead of staying up for a player who is gone.
        self.guest_close_menus_and_tell(conn);
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        // PlayerList.respawn builds a new `ServerPlayer`, whose `lastEquipmentItems` starts
        // empty: the mods hear each worn item again from empty (`native_players_equipment`).
        if let Some(play) = host.play.get_mut(&conn) {
            play.equipment_heard = None;
            play.inventory_heard = None;
            play.level_up_sound = Default::default();
        }
        // PlayerList.respawn: a server's player comes back in its respawn
        // point's level: the Overworld, where the world spawn and beds are,
        // or another one's anchor or forced point, which its block resolves
        // once the level has loaded around it.
        let elsewhere = host.hub.guests[i]
            .respawn_elsewhere
            .as_ref()
            .map(RespawnConfig::from_saved);
        let level = elsewhere.map_or(crate::game::Dim::Overworld, |c| c.dim);
        let spawn = elsewhere.map_or(spawn, |c| {
            [
                f64::from(c.pos[0]) + 0.5,
                f64::from(c.pos[1]),
                f64::from(c.pos[2]) + 0.5,
            ]
        });
        // PlayerRespawnPositionEvent: an Overworld forced point needs no
        // block, so it is final now, before the Respawn packet goes out, and
        // a mod's other level or point costs no extra one. A bed or anchor,
        // or the world spawn's column, is final only once its trip arrives
        // (`arrive_server_trips`).
        let forced_spawn = host.hub.guests[i].spawn;
        // The fresh player faces where `hub.respawn` turns it by default,
        // not the way the dead one faced.
        let found = crate::game::respawn_position::Respawn {
            dim: level,
            at: horizon_core::geometry::Vec3::new(spawn[0], spawn[1], spawn[2]),
            yaw: 0.0,
            pitch: 0.0,
        };
        let server = true;
        let decision = (server && elsewhere.is_none() && forced_spawn.is_some())
            .then(|| self.respawn_position(conn, found, None))
            .flatten();
        let keep_point = decision.is_none_or(|d| d.copy);
        let (level, spawn) = match decision {
            Some(d) => (d.to.dim, [d.to.at.x, d.to.at.y, d.to.at.z]),
            None => (level, spawn),
        };
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let rotation = decision.map(|d| [d.to.yaw, d.to.pitch]);
        let back = self.dim;
        let left = crate::game::Dim::from_id(&host.hub.guests[i].dimension);
        let away = server && host.hub.guests[i].dimension != level.id();
        if server {
            self.server_trips.retain(|t| !t.is(conn));
        }
        if away && let Err(e) = self.enter_level(level) {
            log::error!("could not open {}: {e}", level.id());
            return;
        }
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        host.hub.guests.update(i, |g| {
            // restartClientLoadTimerAfterRespawn.
            g.dead = false;
            g.load_timeout = myriade_engine::lan::guest::CLIENT_LOADED_TIMEOUT;
            g.loading = true;
            g.death_time = 0;
            g.food = 20;
            g.saturation = 5.0;
            g.sent_health = None;
            // ServerPlayer.restoreFrom(old, false): the base values come back,
            // and the new player has the max health they give.
            if let Some(m) = g.new_body(false) {
                let m = *m;
                g.logs_kept = Some((m.hurt_log, m.attack_log));
            }
            g.health = g.respawn_health(self.motion);
            let mut xp = Experience {
                level: g.xp_level,
                progress: g.xp_progress,
                total: g.xp_total,
                score: g.score,
            };
            let keep_inventory = self
                .session
                .rules
                .bool_at(myriade_engine::gamerules::vanilla::KEEP_INVENTORY)
                || g.game_mode == SPECTATOR;
            xp.respawn(keep_inventory);
            g.effects = Default::default();
            (g.xp_level, g.xp_progress, g.xp_total, g.score) =
                (xp.level, xp.progress, xp.total, xp.score);
        });
        // sendLevelInfo reads the level's respawn data when it sends, after the
        // Clone hook, which may have moved it.
        let world_spawn = self.world_spawn_position();
        let clock_rate = self.network_clock_rate();
        let ctx = TickCtx {
            world: &mut self.world,
            entities: &[],
            next_entity: &mut self.next_entity,
            // PlayerList.respawn / sendLevelInfo send level respawn data,
            // independently of the personal destination passed to Hub::respawn.
            spawn: world_spawn,
            spawn_angle: self.level.world_spawn_angle,
            spawn_dimension: self.level.spawn_dimension(),
            sea_level: self.end.locator().map_or(63, |terrain| terrain.sea_level()),
            difficulty: i32::from(self.session.difficulty),
            difficulty_locked: self.level.difficulty_locked,
            clock_rate,
            tick_rate: self.clock.rate(),
            tick_frozen: self.clock.frozen(),
            tick_steps: self.clock.step_state(),
            collision: Some(self.collision),
            border: &self.border,
            streaming: Some(&self.streaming),
        };
        if let Some(host) = self.lan.as_mut() {
            host.hub.respawn(i, spawn, rotation, &ctx);
        }
        // ServerGamePacketListenerImpl.handleClientCommand: a hardcore world's
        // respawned player only watches.
        if self.level.hardcore
            && let Some(host) = self.lan.as_mut()
        {
            host.hub
                .set_game_mode(conn, GameMode::Spectator.id(), &self.world, Some(self.collision));
        }
        // PlayerList.respawn: `copyRespawnPosition` hands the new player the
        // old point through `setRespawnPosition`: none, or an Overworld forced
        // point (`spawn`). A point with a block is copied once the block is
        // found. A cancel leaves the new player with none; it still comes
        // back here this time, as vanilla placed it before the copy.
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        if !keep_point {
            // copyOriginalSpawnPosition false: the fresh player has none.
            {
                let value = None;
                host.hub.guests.update(i, |guest| guest.spawn = value);
            }
        }
        let in_frame_bed: Option<RespawnConfig> = None;
        if let Some(config) = elsewhere {
            self.respawn_trip(conn, config);
        } else if in_frame_bed.is_none() && forced_spawn.is_none() {
            // TeleportTransition.createDefault: no point of its own, so the
            // world spawn's free column once its chunks have loaded.
            self.world_spawn_trip(conn, None);
        }
        if let Some(c) = in_frame_bed
            && self.dim == crate::game::Dim::Overworld
            && self.world.loaded(c.pos[0], c.pos[2])
        {
            let found = self
                .as_guest(conn, |g| g.resolve_server_respawn(c, true))
                .flatten();
            let world = self
                .level
                .world_spawn
                .map_or([0.5, 80.0, 0.5], |[x, y, z]| {
                    [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5]
                });
            let Some(host) = self.lan.as_mut() else {
                return;
            };
            let at = found.map_or(world, |(at, yaw, pitch)| {
                {
                    let value = yaw;
                    host.hub.guests.update(i, |guest| {
                        guest.yaw = value;
                        guest.pitch = pitch;
                    });
                }
                [at.x, at.y, at.z]
            });
            if found.is_none() {
                // TeleportTransition.missingRespawnBlock.
                host.hub.missing_respawn_block(conn);
                self.world_spawn_trip(conn, None);
            }
            if let Some(host) = self.lan.as_mut() {
                host.hub.teleport(i, Some(at));
            }
        }
        // The player left another level for the respawn point's: placed for
        // good they tell the mods now, else the trip that is still carrying
        // them (a bed or anchor in another level, the world spawn's free
        // column) does when it arrives (`arrive_server_trips`).
        if away {
            if let Some(trip) = self.server_trips.iter_mut().find(|t| t.is(conn)) {
                trip.left = Some(left);
            }
        }
        if away && let Err(e) = self.enter_level(back) {
            log::error!("could not reopen {}: {e}", back.id());
        }
    }

    /// `ServerPlayer.die` for a guest whose health has run out: the death
    /// message, the loot and experience, the death screen.
    pub(in crate::game) fn guest_die(&mut self, gi: usize, conn: u32) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let dying = host.hub.guests[gi].health <= 0.0 && !host.hub.guests[gi].dead;
        if dying && self.guest_death_canceled(gi, conn) {
            return;
        }
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        if host.hub.guests[gi].health <= 0.0 && !host.hub.guests[gi].dead {
            self.guest_forget_digging(conn);
            // FishingHook.shouldStopFishing: its owner is dead.
            if let Some(id) = self.ecs_own_hook() {
                self.remove_fishing_hook(id);
            }
        }
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let here = host.hub.here(&host.hub.guests[gi]);
        let g = &host.hub.guests[gi];
        if g.health <= 0.0 && !g.dead {
            host.hub.guests.update(gi, |g| {
                g.dead = true;
                g.last_death = Some((g.dimension.clone(), g.position.map(|v| v.floor() as i32)));
            });
            let g = &host.hub.guests[gi];
            let (id, at) = (g.entity_id, g.position);
            let reward = Experience {
                level: g.xp_level,
                progress: g.xp_progress,
                total: g.xp_total,
                score: g.score,
            }
            .death_reward(
                self.session
                    .rules
                    .bool_at(myriade_engine::gamerules::vanilla::KEEP_INVENTORY)
                    || g.game_mode == SPECTATOR,
            );
            let (name, uuid) = (g.name.clone(), g.uuid);
            // ServerPlayer.die: dropAllDeathLoot unless a spectator, from the eyes less 0.3.
            let drops = (g.game_mode != SPECTATOR)
                .then(|| [at[0], at[1] + g.eye_height() - f64::from(0.3f32), at[2]]);
            let cause = host.play.get_mut(&conn).and_then(|p| p.death_cause.take());
            // ServerPlayer.die opens with `gameEvent(ENTITY_DIE)` at the player:
            // sculk sensors and wardens hear the death.
            if here {
                self.as_guest(conn, |game| {
                    let ctx = game.player_context(None);
                    game.post_game_event("minecraft:entity_die", at, ctx);
                });
            }
            // `getKillCredit`, as the death finds it.
            let killer = self.guest_kill_credit_type(conn);
            // ServerPlayer.die: the neutral mobs near a guest in this level
            // forget their anger at them.
            if here {
                self.tell_neutral_mobs_player_died(at, Some(conn));
            }
            // The death's last blow, read before the death message empties the
            // combat record: ServerPlayer.die's triggers read it from a record
            // that CombatTracker.recheckStatus clears only at the end of die.
            let killing = self.guest_combat.get(&conn).and_then(|c| c.killing_blow());
            // ServerPlayer.die: with showDeathMessages, the message goes to
            // the dying player's death screen and to everyone the team lets
            // see it; without it, the death screen gets an empty one.
            let message = if self
                .session
                .rules
                .bool_at(myriade_engine::gamerules::vanilla::SHOW_DEATH_MESSAGES)
            {
                let me = self.player_text(&name, uuid);
                let text = match cause {
                    Some(key) => myriade_engine::text::Text::translate(&key, &[me]),
                    None if at[1] < self.world.below_world_y() => {
                        myriade_engine::text::Text::translate("death.attack.outOfWorld", &[me])
                    }
                    None => self.guest_death_message(conn, me),
                };
                let message = myriade_engine::lan::data::component_nbt(&text.component());
                let board = &self.score.board;
                let Some(host) = self.lan.as_ref() else {
                    return;
                };
                host.hub.system_message_to(message.clone(), |other| {
                    myriade_engine::scoreboard::death_message_reaches(board, &name, other)
                });
                crate::game::dedicated::events::record(
                    "death",
                    &name,
                    uuid,
                    serde_json::json!({ "message": text.plain() }),
                );
                // Only a message for everyone is logged (`broadcastSystemMessage`).
                if board.players_team(&name).is_none_or(|t| {
                    t.death_message_visibility == myriade_engine::scoreboard::Visibility::Always
                }) {
                    crate::game::dedicated::log("INFO", &format!("System chat: {}", text.plain()));
                }
                message
            } else {
                literal("")
            };
            let Some(host) = self.lan.as_ref() else {
                return;
            };
            host.hub.send_to(
                conn,
                &C::PlayerCombatKill(play::PlayerCombatKill { id, message }),
            );
            // LivingEntity.die: the death animation, the dying player included.
            host.hub.entity_event(id, 3);
            let recently_hit = killing.is_some()
                && self
                    .guest_combat
                    .get(&conn)
                    .is_some_and(|c| c.recently_hit_by_player(self.world.game_time));
            let dropping = crate::game::creatures::DyingDrops {
                entity: id as u32,
                kind: "minecraft:player",
                player: Some(uuid),
                recently_hit,
            };
            // LivingEntity.die: the death is heard, and a sculk catalyst
            // nearby takes the experience (before the loot is dropped), as it
            // does for a mob.
            let mut catalyst_xp = 0;
            let consumed = self.catalyst_takes_experience(at, |_| {
                catalyst_xp = reward;
                catalyst_xp
            });
            // CatalystListener: with experience to take, the player who last
            // hurt them gets KILL_MOB_NEAR_SCULK_CATALYST.
            if consumed && catalyst_xp > 0 {
                self.guest_catalyst_trigger(gi, conn, at);
            }
            // ServerPlayer.die: the drops, the deathCount objectives, the
            // experience (`dropAllDeathLoot`), then a kill credit's `killed_by`,
            // awardKillScore and wither rose, then `deaths` and the two timers.
            self.as_guest(conn, |game| {
                if let Some(eyes) = drops {
                    game.drop_inventory(eyes, &dropping);
                }
                game.score_player_died();
                if let Some(kind) = &killer {
                    game.award_stat("killed_by", kind, 1);
                }
            });
            // ServerPlayer.die: a spectator never reaches dropAllDeathLoot. The
            // experience the catalyst took does not drop (`Player.skipDropExperience`),
            // and `dropAllDeathLoot` drops it before the rose.
            if drops.is_some() {
                // LivingEntity.dropExperience: Player.isAlwaysExperienceDropper.
                let reward = self.living_experience_drop(
                    id as u32,
                    "minecraft:player",
                    None,
                    reward as i32,
                );
                if !consumed {
                    self.spawn_orbs(at, reward as u32);
                }
            }
            self.guest_killed_player_trigger(conn, at, killing);
            self.guest_award_kill_score(conn, at);
            self.as_guest(conn, |game| {
                game.award_custom_stat("deaths", 1);
                game.stats.reset_death_timers();
            });
            // `ServerPlayer.die`'s `broadcastEntityEvent`, after the message,
            // the loot and the experience: Fabric's AFTER_DEATH. The blow is
            // the one `living_death` heard, so a death it cancelled returns
            // above and is never told here.
        }
    }

    /// `SculkCatalystBlockEntity.CatalystListener`'s trigger on a player's
    /// death: for the player the dying guest's combat record credits
    /// (`LivingEntity.getKillCredit`), with the death's `killing_blow`.
    pub(super) fn guest_catalyst_trigger(&mut self, gi: usize, conn: u32, at: [f64; 3]) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let now = self.world.game_time;
        let combat = self.guest_combat.get(&conn);
        let player_credit = combat.and_then(|c| c.player_credit(now));
        let who = if let Some(credit) = player_credit {
            self.resolve_player_credit(credit)
        } else {
            combat.and_then(|c| c.kill_credit(now)).and_then(|id| {
                host.hub
                    .guests
                    .iter()
                    .find(|g| g.entity_id as u32 == id)
                    .map(|g| Some(g.conn))
            })
        };
        let Some(who) = who else { return };
        let (damage, attacker) = combat
            .and_then(|c| c.killing_blow())
            .unwrap_or_else(|| ("minecraft:player_attack".into(), None));
        let g = &host.hub.guests[gi];
        let view = myriade_engine::advancements::EntityView {
            kind: "minecraft:player".into(),
            position: at,
            sneaking: g.sneaking,
            input_sneak: g.input & 32 != 0,
            gamemode: Some(
                match g.game_mode {
                    1 => "creative",
                    2 => "adventure",
                    3 => "spectator",
                    _ => "survival",
                }
                .into(),
            ),
            ..Default::default()
        };
        // The damage's causing entity: the player in place, another guest, or
        // a mob (`DamageSource.getEntity`).
        let source = attacker.and_then(|id| {
            host.hub
                .guests
                .iter()
                .find(|g| g.entity_id as u32 == id)
                .map(|g| myriade_engine::advancements::EntityView {
                    kind: "minecraft:player".into(),
                    position: g.position,
                    sneaking: g.sneaking,
                    input_sneak: g.input & 32 != 0,
                    ..Default::default()
                })
                .or_else(|| self.ecs_view(id).map(|_| self.ecs_predicate_view(id)))
        });
        let projectile = match damage.as_str() {
            "minecraft:arrow" => Some("minecraft:arrow"),
            "minecraft:fireball" | "minecraft:small_fireball" => Some("minecraft:fireball"),
            "minecraft:wither_skull" => Some("minecraft:wither_skull"),
            "minecraft:fireworks" => Some("minecraft:firework_rocket"),
            _ => None,
        };
        let blow = myriade_engine::advancements::DamageView {
            kind: damage,
            // DamageSource.isDirect: the causing entity is the direct one.
            is_direct: projectile.is_none(),
            direct: match projectile {
                Some(kind) => Some(myriade_engine::advancements::EntityView {
                    kind: kind.into(),
                    position: at,
                    ..Default::default()
                }),
                None => source.clone(),
            },
            source,
            ..Default::default()
        };
        // CatalystListener: trigger(player, killedEntity, damageSource).
        match who {
            None => {
                self.trigger_killed_as("minecraft:kill_mob_near_sculk_catalyst", &view, Some(&blow))
            }
            Some(kconn) => {
                self.as_guest(kconn, |g| {
                    g.trigger_killed_as(
                        "minecraft:kill_mob_near_sculk_catalyst",
                        &view,
                        Some(&blow),
                    );
                });
            }
        }
    }

    /// `ServerPlayer.die` → `Entity.awardKillScore` on the entity the dying
    /// guest's combat record credits (`LivingEntity.getKillCredit`): the
    /// guest's `entity_killed_player`, with that entity and the death's
    /// `killing_blow`. Nothing credited, nothing fires.
    pub(super) fn guest_killed_player_trigger(
        &mut self,
        conn: u32,
        at: [f64; 3],
        blow: Option<(String, Option<u32>)>,
    ) {
        const TRIGGER: &str = "minecraft:entity_killed_player";
        let now = self.world.game_time;
        let Some(id) = self
            .guest_combat
            .get(&conn)
            .and_then(|c| c.kill_credit(now))
        else {
            return;
        };
        let Some(killer) = self.guest_entity_view(id) else {
            return;
        };
        let blow = self.guest_killing_blow(blow, at);
        self.as_guest(conn, |g| {
            g.trigger_killed_as(TRIGGER, &killer, blow.as_ref());
        });
    }

    /// The type of the entity a dying guest's combat record credits with the
    /// kill (`LivingEntity.getKillCredit`): a player (a guest), a mob (one gone already credits nobody, as vanilla's reference
    /// no longer resolves), a mob whose own blast this tick killed the guest,
    /// or the ender dragon.
    fn guest_kill_credit_type(&self, conn: u32) -> Option<String> {
        let id = self
            .guest_combat
            .get(&conn)?
            .kill_credit(self.world.game_time)?;
        if self
            .lan
            .as_ref()
            .is_some_and(|host| host.hub.guests.iter().any(|g| g.entity_id as u32 == id))
        {
            return Some("minecraft:player".into());
        }
        if let Some(kind) = self.ecs_kind(id) {
            return Some(kind);
        }
        // A mob that blew up this tick has left the list, but vanilla's player
        // dies inside that blast, before the source is discarded.
        if let Some((_, kind)) = self.combat.exploded_kinds.iter().rfind(|(m, _)| *m == id) {
            return Some(kind.clone());
        }
        None
    }

    /// The dying guest's last blow (`LivingEntity.getLastDamageSource`), its
    /// damage type and causing entity as the combat record held them, as a
    /// damage source: the projectile as the direct entity.
    pub(super) fn guest_killing_blow(
        &self,
        blow: Option<(String, Option<u32>)>,
        at: [f64; 3],
    ) -> Option<myriade_engine::advancements::DamageView> {
        let (damage, attacker) = blow?;
        // The damage's causing entity: another guest or a mob (`DamageSource.getEntity`).
        let source = attacker.and_then(|id| self.guest_entity_view(id));
        Some(myriade_engine::advancements::recorded_blow(
            damage, source, at,
        ))
    }

    /// The entity behind a combat record's id, as the predicates see it: a
    /// guest or a mob.
    pub(super) fn guest_entity_view(
        &self,
        id: u32,
    ) -> Option<myriade_engine::advancements::EntityView> {
        self.lan
            .as_ref()?
            .hub
            .guests
            .iter()
            .find(|g| g.entity_id as u32 == id)
            .map(|g| myriade_engine::advancements::EntityView {
                kind: "minecraft:player".into(),
                position: g.position,
                sneaking: g.sneaking,
                input_sneak: g.input & 32 != 0,
                ..Default::default()
            })
            .or_else(|| self.ecs_view(id).map(|_| self.ecs_predicate_view(id)))
    }

    /// The scalar death-tail/death-message source. `living_death` uses the
    /// full accepted hit retained on the body instead.
    pub(super) fn guest_damage_source(&self, conn: u32) -> (String, u32) {
        let (damage_type, attacker) = match self
            .lan
            .as_ref()
            .and_then(|host| host.play.get(&conn))
            .and_then(|p| p.death_cause.as_deref())
        {
            Some("death.attack.genericKill") => ("minecraft:generic_kill", 0),
            _ => self
                .guest_combat
                .get(&conn)
                .and_then(|tracker| tracker.last_blow())
                .unwrap_or(("minecraft:generic", 0)),
        };
        (damage_type.to_owned(), attacker)
    }

    /// `ServerPlayer.die`'s message from a guest's own `CombatTracker`, which
    /// sits in `guest_combat` between turns.
    pub(super) fn guest_death_message(
        &mut self,
        conn: u32,
        me: myriade_engine::text::Text,
    ) -> myriade_engine::text::Text {
        let mut tracker = self.guest_combat.remove(&conn).unwrap_or_default();
        std::mem::swap(&mut self.combat, &mut tracker);
        let text = self.combat_death_message_for(me);
        std::mem::swap(&mut self.combat, &mut tracker);
        self.guest_combat.insert(conn, tracker);
        text
    }
}
