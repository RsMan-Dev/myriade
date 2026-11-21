//! The lightning, crystal, fang and cloud events of the ECS (`MiscEvent`), applied by the host: a strike's fire, rods and copper, the
//! bite's sound, and an area cloud's effects on the creatures it reached. Creation is `ecs_strike` and `add_fangs` (the mobs' fangs).
use super::super::Game;
use horizon_client::{effects::EffectInstance, random_source};
use horizon_core::random::{EntityRandom, RandomSource};
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
        for ev in evs {
            match ev {
                MiscEvent::Lightning { at, cosmetic } => {
                    if !cosmetic {
                        if self.ui.difficulty >= 2 {
                            let mut random = EntityRandom::own(random_source::entity());
                            self.bolt_fire(at, 4, &mut random);
                        }
                        if self.remote.is_none() {
                            self.bolt_clear_copper(at);
                        }
                    }
                    if self.remote.is_none() {
                        self.strike_rod(at);
                    }
                }
                MiscEvent::CrystalBlast { .. } => {}
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
            }
        }
    }
}
