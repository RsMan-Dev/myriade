//! The `MobEvent`s the ECS emits that touch a creature, applied on the ECS through the bridge:
//! `EntityEvent` (the ravager's roar), `Push` on a creature, `SetTarget` (the mods' `living_change_target`, the guest binding) and
//! `ParrotToShoulder`. The others the ECS emits (`Darkness`, `Experience`, `ChestOpen`, `ChestSlot`) touch the world and the players only.
use crate::game::Game;
use crate::mobs::Who;
use horizon_client::mobs::tame::owned_by;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::nbt::Tag;
use horizon_ecs_proto::targeting::Who as PWho;

/// `Ravager.roar`'s event: it ends the roar and pushes the armor stands around.
const ROAR_END_EVENT: i8 = 69;
/// The ravager's box (1.95 wide, 2.2 high) inflated by 4 (`getBoundingBox().inflate(4.0)`), from its position.
const ROAR_REACH: (f64, f64, f64) = (0.975 + 4.0, 4.0, 4.0 + 2.2);

impl Game<'_> {
    /// `MobEvent::EntityEvent` of an ECS creature: the entity event for the clients.
    pub(in crate::game) fn ecs_mob_entity_event(&mut self, mob: u32, event: i8, at: [f64; 3]) {
        self.lan_entity_event(mob as i32, event, at);
    }

    /// `MobEvent::Push` on a creature: the velocity is added to the ECS entity's.
    pub(in crate::game) fn ecs_mob_push(&mut self, id: u32, velocity: [f64; 3]) {
        self.ecs.push(id, velocity);
    }

    /// `MobEvent::SetTarget` of an ECS creature. P already holds the target; the host adds what it owns: a fake guest cannot be
    /// targeted, a mod may veto or redirect it (`living_change_target`), and a player target is bound to the guest it came from.
    pub(in crate::game) fn ecs_mob_target(&mut self, mob: u32, target: Who) {
        let player = self.lan.as_ref().and_then(|h| h.mob_players.get(&mob).copied()).or(self.lan_acting);
        let fake = self.lan_acting_fake
            || player.is_some_and(|c| self.lan.as_ref().and_then(|h| h.hub.guest(c)).is_some_and(|g| g.is_fake()));
        if matches!(target, Who::Player) && fake {
            self.ecs_with(|ecs, mw| ecs.set_target(mw, mob, None));
            return;
        }
        if mod_api::__private::listened(mod_api::HookId::LivingChangeTarget) {
            let Some(epoch) = self.native_lifecycle.epoch() else { return };
            let target_id = match target {
                Who::Mob(t) => Some(t),
                // The id P holds in the target; the connection only when the target has not reached P yet.
                Who::Player => self.ecs.ai_target_player(mob).or_else(|| self.player_entity_id_by_connection(player).and_then(|id| u32::try_from(id).ok())),
            };
            let Some(target_id) = target_id else { return };
            let living = |id| mod_api::LivingRef::new(mod_api::EntityRef::new(epoch, id));
            let original = Some(living(target_id));
            let me = mod_api::MobRef::new(mod_api::EntityRef::new(epoch, mob));
            match self.native_mob_target_event(me, original, self.ecs.has_brain(mob)) {
                // Cancelled: the event does not carry the previous target, so the creature is left without one.
                None => {
                    self.ecs_with(|ecs, mw| ecs.set_target(mw, mob, None));
                    return;
                }
                Some(new) if new != original => {
                    let who = new.and_then(|n| {
                        let id = n.entity().id();
                        match self.ecs.entity_of(id) {
                            Some(e) => Some(PWho::Entity(e)),
                            None => self.with_player(id, |_| ()).map(|_| PWho::Player(id)),
                        }
                    });
                    self.ecs_with(|ecs, mw| ecs.set_target(mw, mob, who));
                    return;
                }
                Some(_) => {}
            }
        }
        if let (Who::Player, Some(host)) = (target, self.lan.as_mut()) {
            match player {
                Some(conn) => host.mob_players.insert(mob, conn),
                None => host.mob_players.remove(&mob),
            };
        }
    }

    /// `ServerPlayer.setEntityOnShoulder` for an ECS parrot that touched its owner: its compound goes in the player's first free
    /// shoulder slot and the entity leaves the ECS. The acting player must be the saved owner (`getOwner`).
    pub(in crate::game) fn ecs_parrot_to_shoulder(&mut self, mob: u32) {
        let Some(e) = self.ecs.entity_of(mob) else { return };
        let owner = self.ecs.tamable_values(mob).and_then(|(_, owner)| owner);
        if !owned_by(owner, self.acting_uuid()) {
            return;
        }
        let Some(nbt) = self.ecs.entity_nbt(e) else { return };
        if self.put_on_shoulder(|_| Tag::Compound(nbt)) {
            self.ecs.despawn(e);
        }
    }
}
