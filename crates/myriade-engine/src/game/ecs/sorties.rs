//! The host's effects of the ECS outputs that had none: a death, a spawn, a totem asked then popped, a target
//! cleared, a hit dealt, a lead taken off, an item or an orb at the end of its life, equipment that changed or broke.
use crate::game::Game;
use bevy_ecs::entity::Entity;
use myriade_engine::health::{HurtSource, Hurting};
use myriade_engine::items::ItemStack;
use horizon_ecs_proto::host_out_equipment::EquipmentEvent;
use horizon_ecs_proto::host_out_items::ItemEvent;
use horizon_ecs_proto::host_out_using::UsingEvent;
use horizon_ecs_proto::targeting::Who;

/// `EquipmentSlot` names in the order of P's `equip_stack::Equipment`.
const SLOTS: [&str; 8] = ["mainhand", "offhand", "head", "chest", "legs", "feet", "body", "saddle"];
/// `LivingEntity.entityEventForEquipmentBreak`: the entity event of a worn piece that broke, by slot (main, off, head, chest, legs, feet, body 65, saddle 68).
const BREAK_EVENTS: [i8; 8] = [47, 48, 49, 50, 51, 52, 65, 68];
/// `LivingEntity.handleEntityEvent` 35: the totem's animation.
const TOTEM_EVENT: i8 = 35;

impl Game<'_> {
    /// `HostEvent::Death`: the game event is posted where it died. While a mod hears `living_death_post` P keeps the entity `Dying`
    /// (`set_death_vetoable`): the loot leaves (`loot_dying`), the hook reads the entity still there, then `despawn`. The attacker is
    /// the mob's id, 0 for a player or none.
    pub(in crate::game) fn ecs_death_event(&mut self, e: Entity, cause: &str, attacker: Option<Who>, at: [f64; 3]) {
        let ctx = self.who_context(None, None);
        self.post_game_event("minecraft:entity_die", at, ctx);
        if self.ecs.world().is_none_or(|w| w.get_entity(e).is_err()) {
            return;
        }
        let id = self.entity_id(e);
        // A hook that went away since the flag was set: the death simply goes on.
        if !false {
            self.ecs_with(|ecs, mw| ecs.resolve_death(mw, id, None));
            return;
        }
        let by = match attacker {
            Some(Who::Entity(a)) => self.entity_id(a),
            _ => 0,
        };
        // As the mob's: the loot leaves first (`loot_dying`), the hook reads the entity still there, then it is removed. A false
        // answer means P no longer holds it `Dying` (gone already): there is nothing to tell or remove.
        if !self.ecs_with(|ecs, mw| ecs.loot_dying(mw, id)) {
            return;
        }
        self.native_ecs_death_post(id, &format!("minecraft:{cause}"), by);
        self.ecs.despawn(e);
    }

    /// `HostEvent::TotemAsk`: `living_use_totem` may cancel the totem of a lethal hit before P uses it up; the answer goes back with
    /// `resolve_totem` (cancelled: the entity dies and keeps the item).
    pub(in crate::game) fn ecs_totem_asked(&mut self, e: Entity, off_hand: bool, cause: &str, attacker: Option<Who>) {
        let id = self.entity_id(e);
        let by = self.ecs_who_id(attacker) as i32;
        let cancel = match (self.ecs_kind(id), self.ecs.position(id)) {
            (Some(kind), at) => {
                let damage_type = format!("minecraft:{cause}");
                let source = HurtSource { damage_type: &damage_type, causing: by, direct: by, position: at };
                crate::game::entity::damage_level_of!(self).totem_canceled(Hurting::Entity(id as i32, &kind, source), off_hand)
            }
            _ => false,
        };
        self.ecs_with(|ecs, mw| ecs.resolve_totem(mw, e, cancel));
    }

    /// `HostEvent::TargetChanged` with no target: `living_change_target` hears the clear (not a brain's). A refusal puts `previous` back,
    /// a replacement is written as the new target; both go through `set_target`, whose `Some` event has no handler for an ECS mob.
    pub(in crate::game) fn ecs_target_cleared(&mut self, e: Entity, previous: Option<Who>) {
        if !false {
            return;
        }
        let Some(epoch) = self.native_lifecycle.epoch() else { return };
        let id = self.entity_id(e);
        let mob = mod_api::MobRef::new(mod_api::EntityRef::new(epoch, id));
        match self.native_mob_target_event(mob, None, false) {
            Some(None) => {}
            None => {
                if previous.is_some() {
                    self.ecs_with(|ecs, mw| ecs.set_target(mw, id, previous));
                }
            }
            Some(Some(new)) => {
                let new_id = new.entity().id();
                let who = match self.ecs.entity_of(new_id) {
                    Some(t) => Some(Who::Entity(t)),
                    None => self.with_player(new_id, |_| ()).map(|_| Who::Player(new_id)),
                };
                if who.is_some() {
                    self.ecs_with(|ecs, mw| ecs.set_target(mw, id, who));
                }
            }
        }
    }

    /// `HostEvent::HitDealt`: the mods' `hurt_enemy` hears the stack the attacker holds, a player's (the acting one while it is told) or the
    /// ECS mob's main hand.
    pub(in crate::game) fn ecs_hit_dealt(&mut self, attacker: Who, victim: Entity) {
        let vid = self.entity_id(victim);
        match attacker {
            Who::Player(p) => {
                self.with_player(p, |g| {
                    if let Some(stack) = g.held().cloned() {
                        g.native_ecs_hurt_enemy(&stack, vid);
                    }
                });
            }
            Who::Entity(a) => {
                let aid = self.entity_id(a);
                if let Some(stack) = self.ecs.equipment_stack(aid, 0) {
                    self.native_ecs_hurt_enemy(&stack, vid);
                }
            }
        }
    }

    /// `HostEvent::ConversionAsk`: `living_conversion_pre` may keep the entity; else the new one is born, `living_conversion_post` is
    /// told while both are alive, and the old one leaves.
    pub(in crate::game) fn ecs_conversion_asked(&mut self, e: Entity, into: &str) {
        let outcome = if into.contains(':') { into.to_owned() } else { format!("minecraft:{into}") };
        let old = self.entity_id(e);
        let cancel = self.ecs_conversion_pre(old, &outcome);
        if let Some(new) = self.ecs.resolve_conversion(e, cancel) {
            self.ecs_converted(old, new);
            self.ecs.finish_conversion(e);
        }
    }

    /// `HostEvent::TargetAsk`: `living_change_target` decides the target a brain proposed (held only while a mod hears it); the answer
    /// goes back through `resolve_target`. A player proposal reaches the hook as no target (its `LivingRef` is not built here).
    pub(in crate::game) fn ecs_target_asked(&mut self, e: Entity, proposed: Option<Who>) {
        use horizon_ecs_proto::brain::behaviours::start_attacking::TargetAnswer;
        let answer = match self.native_lifecycle.epoch() {
            None => TargetAnswer::Keep,
            Some(epoch) => {
                let id = self.entity_id(e);
                let mob = mod_api::MobRef::new(mod_api::EntityRef::new(epoch, id));
                let original = match proposed {
                    Some(Who::Entity(t)) => Some(mod_api::LivingRef::new(mod_api::EntityRef::new(epoch, self.entity_id(t)))),
                    _ => None,
                };
                match self.native_mob_target_event(mob, original, false) {
                    None => TargetAnswer::Keep,
                    Some(None) => TargetAnswer::Set(None),
                    Some(Some(new)) => {
                        let new_id = new.entity().id();
                        let who = match self.ecs.entity_of(new_id) {
                            Some(t) => Some(Who::Entity(t)),
                            None => self.with_player(new_id, |_| ()).map(|_| Who::Player(new_id)),
                        };
                        TargetAnswer::Set(who)
                    }
                }
            }
        };
        self.ecs.resolve_target(e, answer);
    }

    /// `HostEvent::Spawned`: the mods are told of a mob that joined the level, and its mod goals are attached (every spawn path announces).
    pub(in crate::game) fn ecs_spawned_event(&mut self, e: Entity, natural: bool) {
        let id = self.entity_id(e);
        if let (Some(kind), Some(w)) = (self.ecs.kind(id), self.ecs.ecs_mut()) {
            myriade_engine::ecs_bridge::mod_goals::attach(w.world_mut(), e, kind);
        }
        self.native_ecs_joined(id, natural);
    }

    /// `HostEvent::Tamed`: `entity_tame` with the owner's uuid (the player's, resolved where it acts). The single emitter: the host's
    /// own tame paths no longer fire the hook.
    pub(in crate::game) fn ecs_tamed_event(&mut self, e: Entity, owner: Who) {
        let id = self.entity_id(e);
        let Some(kind) = self.ecs_kind(id) else { return };
        let owner = match owner {
            Who::Player(p) => self.with_player(p, |g| myriade_engine::lan::playerdata::ints_uuid(g.acting_player_uuid())),
            Who::Entity(_) => None,
        };
        self.native_entity_tame(id, &kind, owner);
    }

    /// `HostEvent::Unleashed`: `EntityUnleashEvent`, told while the entity is still in the level.
    pub(in crate::game) fn ecs_unleashed_event(&mut self, e: Entity) {
        let id = self.entity_id(e);
        if let Some(kind) = self.ecs_kind(id) {
            self.native_entity_unleash(id, &kind);
        }
    }

    /// `HostEvent::BecomeBlock` (`CopperGolem.turnToStatue`): the block is set where the entity stood, facing by `Direction.fromYRot`, in
    /// the pose P drew, its custom name in the block entity's components; then the entity goes (P dropped its kept equipment).
    pub(in crate::game) fn ecs_become_block(&mut self, e: Entity, block: &str, at: [i32; 3], pose: u8, yaw: f32, name: Option<String>, lead: Option<Option<[i32; 3]>>) {
        use horizon_core::nbt::{Compound, Tag};
        let facing = ["south", "west", "north", "east"][((f64::from(yaw) / 90.0 + 0.5).floor() as i32 & 3) as usize];
        let pose = ["standing", "sitting", "running", "star"][usize::from(pose) & 3];
        let state = horizon_world::chunk::BlockState {
            name: block.into(),
            properties: [
                ("facing".to_string(), facing.to_string()),
                ("copper_golem_pose".to_string(), pose.to_string()),
                ("waterlogged".to_string(), "false".to_string()),
            ]
            .into(),
        };
        let Some(id) = self.pack.assets.registry.id(&state) else { return };
        if self.level_set(at, id).is_err() {
            return;
        }
        let mut components = Compound::new();
        if let Some(n) = name {
            components.insert("minecraft:custom_name".into(), Tag::String(n.as_str().into()));
        }
        self.world.block_entities.insert((at[0], at[1], at[2]), myriade_engine::containers::BlockEntity::CopperGolemStatue(components));
        let id = self.ecs_id_of(e);
        // `Entity.discard` then `dropLeash` / `removeLeash` (under `entityDrops`): the lead falls where the entity stood.
        if let Some(knot) = lead {
            let at = [f64::from(at[0]) + 0.5, f64::from(at[1]), f64::from(at[2]) + 0.5];
            self.ecs_leash_broke(id, at, false, knot);
        }
        self.entity_left_now(id);
        self.ecs.despawn(e);
    }

    /// `HostEvent::Swing`:`ClientboundSwingAnimationPacket` for the players around the creature (the swinger is not a player here).
    pub(in crate::game) fn ecs_swing_event(&mut self, e: Entity, off_hand: bool) {
        let id = self.entity_id(e);
        let Some(at) = self.ecs.position(id) else { return };
        if let Some(host) = self.lan.as_ref() {
            host.hub.swing(id as i32, i32::from(off_hand), at, None);
        }
    }

    /// `HostEvent::Hurt`: `ServerLevel.broadcastDamageEvent` to the players around. The ids were resolved by P when the hit landed. A
    /// projectile is the direct entity of its hit: `direct` is 0 then, as the vanilla packet's.
    pub(in crate::game) fn ecs_hurt_event(&mut self, id: u32, damage: &str, cause: u32, direct: bool, at: [f64; 3]) {
        let Some(host) = self.lan.as_ref() else { return };
        let cause = (cause != 0).then_some(cause as i32);
        host.hub.damage_event(id as i32, &format!("minecraft:{damage}"), cause, cause.filter(|_| direct), at);
    }

    /// `ItemEvent`: the end of life of an item or an orb, sent only when a mod listens (`set_*_expiry_hooked`).
    pub(in crate::game) fn ecs_item_event(&mut self, ev: ItemEvent) {
        match ev {
            ItemEvent::Expiring { entity, stack, at } => {
                let stack: ItemStack = stack;
                self.ecs_item_expiring(entity, &stack, at)
            }
            ItemEvent::Hurt { at, by } => {
                let by = self.boat_hurt_by(by);
                self.item_damage_event(horizon_core::geometry::Vec3::new(at[0], at[1], at[2]), by);
            }
            // No mod hook reads an orb's end of life: P resolves it as it would have without the pause.
            ItemEvent::OrbExpiring { entity, .. } => self.ecs.resolve_item_expiry(entity, None),
        }
    }

    /// `EquipmentEvent`: the mods hear a changed slot (`living_equipment_change`); a piece that broke is told to the clients.
    pub(in crate::game) fn ecs_equipment_event(&mut self, ev: EquipmentEvent) {
        match ev {
            EquipmentEvent::Changed { entity, slot, from, to } => {
                if !mod_api::__private::enabled_listened(mod_api::HookId::LivingEquipmentChange) {
                    return;
                }
                let id = self.entity_id(entity);
                let (Some(kind), Some(name)) = (self.ecs_kind(id), SLOTS.get(slot)) else { return };
                self.native_living_equipment_change(id, &kind, name, from.as_ref(), to.as_ref());
            }
            EquipmentEvent::Broke { entity, slot } => {
                let id = self.entity_id(entity);
                if let (Some(&code), Some(at)) = (BREAK_EVENTS.get(slot), self.ecs.position(id)) {
                    self.lan_entity_event(id as i32, code, at);
                }
            }
        }
    }

    /// `UsingEvent`: a totem that popped plays its animation (the start, stop and release of a use say nothing: the host's own calls
    /// did their effect).
    pub(in crate::game) fn ecs_using_event(&mut self, ev: UsingEvent) {
        match ev {
            UsingEvent::TotemPopped { entity, .. } => {
                let id = self.entity_id(entity);
                if let Some(at) = self.ecs.position(id) {
                    self.lan_entity_event(id as i32, TOTEM_EVENT, at);
                }
            }
            UsingEvent::Started { .. } | UsingEvent::Stopped { .. } | UsingEvent::Released { .. } => {}
        }
    }
}
