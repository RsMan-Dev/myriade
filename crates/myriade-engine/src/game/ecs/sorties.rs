//! The host's effects of the ECS outputs that had none: a death, a spawn, a totem asked then popped, a target
//! cleared, a hit dealt, a lead taken off, an item or an orb at the end of its life, equipment that changed or broke.
use crate::game::Game;
use bevy_ecs::entity::Entity;
use myriade_engine::items::ItemStack;
use horizon_ecs_proto::host_out_equipment::EquipmentEvent;
use horizon_ecs_proto::host_out_items::ItemEvent;
use horizon_ecs_proto::host_out_using::UsingEvent;
use horizon_ecs_proto::targeting::Who;

/// `EquipmentSlot` names in the order of P's `equip_stack::Equipment`.
/// `LivingEntity.entityEventForEquipmentBreak`: the entity event of a worn piece that broke, by slot (main, off, head, chest, legs, feet, body 65, saddle 68).
const BREAK_EVENTS: [i8; 8] = [47, 48, 49, 50, 51, 52, 65, 68];
/// `LivingEntity.handleEntityEvent` 35: the totem's animation.
const TOTEM_EVENT: i8 = 35;

impl Game<'_> {
    /// `HostEvent::Death`: the game event is posted where it died. While a mod hears `living_death_post` P keeps the entity `Dying`
    /// (`set_death_vetoable`): the loot leaves (`loot_dying`), the hook reads the entity still there, then `despawn`. The attacker is
    /// the mob's id, 0 for a player or none.
    pub(in crate::game) fn ecs_death_event(&mut self, e: Entity, cause: &str, _attacker: Option<Who>, at: [f64; 3]) {
        let ctx = self.who_context(None, None);
        self.post_game_event("minecraft:entity_die", at, ctx);
        if self.ecs.world().is_none_or(|w| w.get_entity(e).is_err()) {
            return;
        }
        let id = self.entity_id(e);
        // `LivingEntity.getKillCredit`: the player hurt within 100 ticks; the view is read before the death removes the entity.
        let credited = self.ecs.world().and_then(|w| horizon_ecs_proto::credit::player_of(w, e));
        let victim = self.ecs_host_view(id);
        self.ecs_with(|ecs, mw| ecs.resolve_death(mw, id, None));
        if let (Some(by), Some(victim)) = (credited, victim) {
            let me = myriade_engine::advancements::EntityView { kind: "minecraft:player".into(), ..Default::default() };
            let blow = myriade_engine::advancements::DamageView { kind: format!("minecraft:{}", cause.trim_start_matches("minecraft:")), is_direct: true, direct: Some(me.clone()), source: Some(me), ..Default::default() };
            self.ecs_award_kill(by, &victim, Some(&blow));
        }
    }

    /// `HostEvent::TotemAsk`: no mod cancels the totem of a lethal hit, so P uses it up.
    pub(in crate::game) fn ecs_totem_asked(&mut self, e: Entity) {
        self.ecs_with(|ecs, mw| ecs.resolve_totem(mw, e, false));
    }

    /// `HostEvent::ConversionAsk`: `living_conversion_pre` may keep the entity; else the new one is born, `living_conversion_post` is
    /// told while both are alive, and the old one leaves.
    pub(in crate::game) fn ecs_conversion_asked(&mut self, e: Entity, _into: &str) {
        let old = self.entity_id(e);
        if let Some(new) = self.ecs.resolve_conversion(e, false) {
            self.ecs_converted(old, new);
            self.ecs.finish_conversion(e);
        }
    }

    /// `HostEvent::TargetAsk`: no mod listens to `living_change_target`, so the brain's proposed target is kept.
    pub(in crate::game) fn ecs_target_asked(&mut self, e: Entity) {
        use horizon_ecs_proto::brain::behaviours::start_attacking::TargetAnswer;
        if let Some(w) = self.ecs.world_mut() {
            horizon_ecs_proto::brain::behaviours::start_attacking::resolve_target(w, e, TargetAnswer::Keep);
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

    /// `EquipmentEvent`: a piece that broke is told to the clients (no mod hears a changed slot).
    pub(in crate::game) fn ecs_equipment_event(&mut self, ev: EquipmentEvent) {
        match ev {
            EquipmentEvent::Changed { .. } => {}
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
