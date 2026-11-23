//! The arrow events of the ECS (`ArrowEvent`), applied by the host's own functions: the hit's sound and advancement on a mob, the blow's
//! knockback on a player (gap: no direction in the event), the pickup a player or a guest asks (the stack goes to the inventory, then
//! `arrow_taken` answers), the potion effects of a hit, the mod impact of its kind, and the launch of a shot arrow with its end effects
//! read by `arrow_shot_of`. P hurts the victim itself. Written, not run.
use horizon_core::random::RandomSource as _;
use crate::game::{Game, pickups::{Collector, Taken}};
use myriade_engine::{advancements::EntityView, arrows::ArrowState, effects::EffectInstance, health::{Attack, HurtSource}, items::ItemStack};
use horizon_ecs_proto::{arrow_more::{ArrowShot, Def}, host_out_arrows::ArrowEvent, targeting::Who};

impl Game<'_> {
    /// Shoots an arrow from `at` with `velocity`, owned by `owner` and carrying `shot`; its network id, 0 when the level is inert.
    pub(in crate::game) fn ecs_shoot_arrow(&mut self, at: [f64; 3], velocity: [f64; 3], owner: Option<Who>, shot: ArrowShot, weapon: Option<ItemStack>) -> u32 {
        let mut next = self.next_entity;
        let id = self.ecs_with(|ecs, mw| {
            ecs.shoot_arrow(mw, at, velocity, owner, shot, weapon, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        if id != 0 && self.native_hears_impacts() {
            self.ecs.set_mod_gate(id, true);
        }
        id
    }

    /// The shot of an arrow the host aimed: its pickup and ammo, and its end effects read from `arrow` and the weapon it was fired
    /// from (Flame sets the fire of a hit, Punch the push), with `effect` the potion of its item.
    pub(in crate::game) fn arrow_shot_of(&self, arrow: &ArrowState, effect: Option<(&'static str, i32)>) -> ArrowShot {
        let level = |name: &str| {
            arrow.fired_from_weapon.as_ref().and_then(|w| w.all_enchantments().iter().find(|(e, _)| &**e == name).map(|(_, l)| *l)).unwrap_or(0)
        };
        let fx = Def {
            fire: if level("minecraft:flame") > 0 { 100 } else { 0 },
            punch: level("minecraft:punch") as f32,
            pierce: arrow.pierce,
            effects: effect.map(|(id, ticks)| vec![(id, 0, ticks)]).unwrap_or_default(),
        };
        let sound = if arrow.crossbow_hit_sound { "minecraft:item.crossbow.hit" } else { "minecraft:entity.arrow.hit" };
        ArrowShot { pickup: arrow.pickup, stack: arrow.pickup_item.clone(), fx, sound }
    }

    /// The id a `Who` carries on the ECS and the mod API: a player's entity id, a mob's network id, 0 for none.
    pub(in crate::game) fn ecs_who_id(&mut self, who: Option<Who>) -> u32 {
        match who {
            Some(Who::Player(p)) => p,
            Some(Who::Entity(e)) => self.ecs_id_of(e),
            None => 0,
        }
    }

    pub(in crate::game) fn ecs_arrow_event(&mut self, ev: ArrowEvent) {
        match ev {
            ArrowEvent::Hit { victim: Some(Who::Entity(victim)), at, damage, sound, pitch, .. } => {
                self.audio.play(sound, Some(at), 1.0, pitch);
                let vid = self.ecs_id_of(victim);
                if let Some(view) = self.ecs_host_view(vid) {
                    let direct = myriade_engine::advancements::EntityView {
                        kind: "minecraft:arrow".into(),
                        position: at,
                        ..Default::default()
                    };
                    self.thrower_dealt_absorbed(None);
                    self.thrower_hit(None, &view, direct, damage);
                }
            }
            ArrowEvent::Hit { entity, victim: Some(Who::Player(player)), owner, at, damage, damage_type, knockback, dx, dz, sound, pitch, .. } => {
                self.audio.play(sound, Some(at), 1.0, pitch);
                let (direct, causing) = (self.ecs_id_of(entity) as i32, self.ecs_who_id(owner) as i32);
                let source = HurtSource { damage_type, causing, direct, position: None };
                let attack = Attack { attacker: causing, projectile: Some(direct), projectile_is_arrow: true, ..Default::default() };
                let power = f64::from(knockback);
                let guest = self.lan.as_ref().and_then(|host| host.hub.guests.iter().find(|g| g.entity_id as u32 == player).map(|g| g.conn));
                match guest {
                    Some(conn) => {
                        self.as_guest(conn, |g| g.knock_back_player(power, dx, dz, source, &attack, damage));
                    }
                    None if self.player.as_ref().is_some_and(|me| me.entity_id as u32 == player) => {
                        self.knock_back_player(power, dx, dz, source, &attack, damage);
                    }
                    None => {}
                }
            }
            ArrowEvent::Hit { victim: None, at, sound, pitch, .. } => self.audio.play(sound, Some(at), 1.0, pitch),
            ArrowEvent::PickedUp { entity, by: Who::Entity(_), .. } => self.ecs.arrow_taken(entity, false),
            ArrowEvent::PickedUp { entity, by: Who::Player(player), stack } => {
                let id = self.ecs_id_of(entity);
                let at = self.ecs_view(id).map_or([0.0; 3], |v| v.position);
                let guest = self.lan.as_ref().and_then(|host| host.hub.guests.iter().find(|g| g.entity_id as u32 == player).map(|g| (g.conn, g.entity_id)));
                if let Some((conn, eid)) = guest {
                    let rest = self.as_guest(conn, |g| g.add_stack_to_inventory(stack.clone()));
                    if let Some(Some(rest)) = rest {
                        self.spawn_item(rest, at, [0.0; 3], 0);
                    }
                    if let Some(host) = self.lan.as_ref() {
                        host.hub.take_item(id as i32, eid as i32, 1, at);
                    }
                    self.ecs.arrow_taken(entity, true);
                    return;
                }
                let took = self.native_pickup_insert(&stack).is_none();
                let got = if took { stack.count as i32 } else { 0 };
                if got > 0 {
                    self.award_stat("picked_up", &stack.item, got);
                    self.native_entity_left(id);
                    self.add_pickup(Taken::Ecs { id, count: got, at, kind: "arrow" }, Collector::Player);
                    let mut random = myriade_engine::random_source::create();
                    let pitch = (random.next_float() - random.next_float()) * 1.4 + 2.0;
                    let feet = self.player.as_ref().map_or(at, |p| [p.position.x, p.position.y, p.position.z]);
                    self.audio.play("minecraft:entity.item.pickup", Some(feet), 0.2, pitch);
                }
                self.ecs.arrow_taken(entity, took);
            }
            ArrowEvent::ModImpact { entity, .. } => {
                let projectile = self.ecs_id_of(entity);
                self.ecs.resolve_impact(projectile, false);
            }
            ArrowEvent::Killed { kinds, weapon, .. } if !kinds.is_empty() => {
                // The event carries the kinds, not the victims themselves: each victim is an EntityView of its kind.
                let victims: Vec<EntityView> = kinds.iter().map(|k| EntityView { kind: (*k).into(), ..Default::default() }).collect();
                self.trigger_killed_by_arrow(&victims, weapon.as_ref());
            }
            ArrowEvent::Killed { .. } => {}
            ArrowEvent::PotionEffect { target: Who::Entity(e), effect, amplifier, ticks } => {
                let id = self.ecs_id_of(e);
                self.ecs_add_effect(id, effect, ticks, amplifier);
            }
            ArrowEvent::PotionEffect { target: Who::Player(p), effect, amplifier, ticks } => {
                let guest = self.lan.as_ref().and_then(|host| host.hub.guests.iter().find(|g| g.entity_id as u32 == p).map(|g| g.conn));
                if let Some(conn) = guest {
                    self.as_guest(conn, |g| g.add_player_effect(EffectInstance::new(effect, ticks, amplifier)));
                } else if self.player.as_ref().is_some_and(|me| me.entity_id as u32 == p) {
                    self.add_player_effect(EffectInstance::new(effect, ticks, amplifier));
                }
            }
        }
    }
}
