//! The lightning, crystal, fang and cloud events of the ECS (`MiscEvent`), applied by the host: a strike's fire, rods and copper, the
//! bite's sound, and an area cloud's effects on the creatures it reached. Creation is `ecs_strike` and `add_fangs` (the mobs' fangs).
use super::super::Game;
use myriade_engine::{effects::EffectInstance, random_source};
use horizon_core::random::EntityRandom;
use super::potions::effect_kind;
use horizon_ecs_proto::{bridge::host_read_living::effect_name, effects::Brew, host_out_misc::MiscEvent, targeting::Who};

impl Game<'_> {
    /// A bolt at `at` in the ECS (`cosmetic` for a skeleton trap's); its fire, rods and copper come back as `MiscEvent::Lightning`.
    pub(in crate::game) fn ecs_strike(&mut self, at: [f64; 3], cosmetic: bool) {
        let mut next = self.next_entity;
        self.ecs_with(|ecs, mw| {
            ecs.strike(mw, at, cosmetic, None, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
    }

    /// The brews of a cloud's `(id, ticks, level)` effects: timed kinds by id, the instant ones as harm or heal.
    pub(in crate::game) fn ecs_brews(effects: &[(&'static str, i32, i32)]) -> Vec<Brew> {
        effects
            .iter()
            .filter_map(|&(id, ticks, level)| match id {
                "instant_damage" => Some(Brew::Harm(level)),
                "instant_health" => Some(Brew::Heal(level)),
                _ => effect_kind(id).map(|k| Brew::Timed(k, level, ticks)),
            })
            .collect()
    }

    /// Applies the misc events the ECS level queued since the last call, in order.
    pub(in crate::game) fn ecs_misc_events(&mut self, evs: impl IntoIterator<Item = MiscEvent>) {
        let mut victims = Vec::new();
        for ev in evs {
            match ev {
                MiscEvent::Lightning { at, cosmetic } => {
                    if !cosmetic {
                        if self.session.difficulty >= 2 {
                            let mut random = EntityRandom::own(random_source::entity());
                            self.bolt_fire(at, 4, &mut random);
                        }
                        {
                            self.bolt_clear_copper(at);
                        }
                    }
                    {
                        self.strike_rod(at);
                    }
                }
                MiscEvent::CrystalBlast { .. } => {}
                MiscEvent::BeeEnterHive { bee, hive } => {
                    let id = self.ecs_id_of(bee);
                    self.bee_enter_hive(id, hive);
                }
                MiscEvent::TreasureAsk { dolphin, at } => {
                    // ServerLevel.findNearestMapStructure(#dolphin_located, 50).
                    let names = self.structure_tag("minecraft:dolphin_located");
                    let found = self.end.locator().filter(|_| !names.is_empty()).and_then(|t| t.locate(&names, at, 50)).map(|(_, p)| p);
                    self.ecs.dolphin_treasure(dolphin, found);
                    if found.is_some() {
                        let id = self.ecs_id_of(dolphin);
                        self.mob_event_particles(id, 38);
                    }
                }
                MiscEvent::Drowned { entity, at, bubbles } => {
                    let id = self.ecs_id_of(entity);
                    self.mob_drown_ecs(id, at, bubbles);
                }
                MiscEvent::LeashBroke { entity, at, sound, knot } => {
                    let id = self.ecs_id_of(entity);
                    self.ecs_leash_broke(id, at, sound, knot);
                }
                MiscEvent::FangsBite { at } => {
                    self.play_fang_bite(at);
                }
                MiscEvent::CloudEffect { target: Who::Player(p), effect, amplifier, ticks, hidden, ambient, source: _, owner: _ } => {
                    let mut inst = EffectInstance::new(effect_name(effect), ticks, amplifier);
                    inst.visible = !hidden;
                    inst.ambient = ambient;
                    let guest = self.lan.as_ref().and_then(|host| host.hub.guests.iter().find(|g| g.entity_id as u32 == p).map(|g| g.conn));
                    if let Some(conn) = guest {
                        self.as_guest(conn, |g| g.add_player_effect(inst));
                    } else if self.player.as_ref().is_some_and(|me| me.entity_id as u32 == p) {
                        self.add_player_effect(inst);
                    }
                }
                MiscEvent::CloudEffect { target: Who::Entity(_), .. } => {}
                MiscEvent::PlayerEffect { player, effect, ticks, amplifier, ambient } => {
                    // The client's effect ids carry no namespace (`speed`, `breath_of_the_nautilus`).
                    let mut inst = EffectInstance::new(effect.strip_prefix("minecraft:").unwrap_or(effect), ticks, amplifier);
                    inst.ambient = ambient;
                    let guest = self.lan.as_ref().and_then(|host| host.hub.guests.iter().find(|g| g.entity_id as u32 == player).map(|g| g.conn));
                    if let Some(conn) = guest {
                        self.as_guest(conn, |g| g.add_player_effect(inst));
                    } else if self.player.as_ref().is_some_and(|me| me.entity_id as u32 == player) {
                        self.add_player_effect(inst);
                    }
                }
                MiscEvent::DestroyAsk { breaker, pos, drop } => {
                    let id = self.ecs_id_of(breaker);
                    if self.mob_may_destroy(id, pos)
                        && let Err(error) = self.destroy_block_as(pos, drop, Some(id))
                    {
                        log::warn!("a creature's block break failed: {:?}", error.message);
                    }
                }
                MiscEvent::PlayerIgnite { player, ticks } => {
                    let guest = self.lan.as_ref().and_then(|host| host.hub.guests.iter().find(|g| g.entity_id as u32 == player).map(|g| g.conn));
                    // `Entity.igniteForSeconds` asks only `fireImmune()` (the type): a creative player ignites too, its invulnerability stops the damage.
                    let result = if let Some(conn) = guest {
                        self.as_guest(conn, |g| g.player.as_mut().map_or(Ok(()), |p| p.ignite_for_ticks(ticks))).unwrap_or(Ok(()))
                    } else if self.player.as_ref().is_some_and(|me| me.entity_id as u32 == player) {
                        self.player.as_mut().map_or(Ok(()), |p| p.ignite_for_ticks(ticks))
                    } else {
                        Ok(())
                    };
                    if let Err(error) = result {
                        log::warn!("a player's ignition failed: {error}");
                    }
                }
                MiscEvent::PufferSting { fish, player, size, silent } => self.ecs_puffer_sting(fish, player, size, silent),
                MiscEvent::LinkedHurt { entity, by_player } => {
                    let id = self.ecs_id_of(entity);
                    if let Some(link) = self.ecs.heart_link(id) {
                        self.creaking_hurt(link.pos, id, by_player);
                        self.ecs.creaking_voice(id, "entity.creaking.sway");
                    }
                }
                MiscEvent::HeartAsked { entity, pos } => {
                    let id = self.ecs_id_of(entity);
                    self.creaking_check_heart(pos, id);
                }
                MiscEvent::Crumbled { center, spread, .. } => self.creaking_tear_down(center, spread),
                MiscEvent::LightningVictim { victim, .. } => {
                    let id = self.ecs_id_of(victim);
                    victims.push(self.ecs_predicate_view(id));
                }
            }
        }
        if !victims.is_empty() {
            self.trigger_channeled_lightning(&victims);
        }
    }
}
