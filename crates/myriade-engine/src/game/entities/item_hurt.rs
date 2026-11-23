//! Shared server damage for dropped items.
use super::{Game, commands::Target};
use horizon_core::nbt::{JavaString, Tag};
use myriade_engine::mobs::Who;

impl Game<'_> {
    /// `ItemEntity.hurtServer`: the item branch reached by `DamageCommand.damage`.
    pub(in crate::game) fn hurt_item_entity(
        &mut self,
        id: u32,
        amount: f32,
        damage_type: &str,
        direct: Option<Target>,
        causing: Option<Target>,
        position: Option<[f64; 3]>,
    ) -> bool {
        let Some(item) = self.ecs.item(id) else {
            return false;
        };
        let registry = &self.health_resources.damage;
        let Some(kind) = registry.id(damage_type) else {
            return false;
        };
        let creative = self.creative_source_of(causing);
        let vanilla = self.saved_refuses_damage(id, damage_type, creative);
        let entity_id = |target| match target {
            Some(Target::Player) => self.own_entity_id(),
            Some(Target::Guest(conn)) => self.command_guest(conn).map_or(0, |g| g.entity_id),
            Some(target) => {
                crate::game::entity::target_id(target).map_or(0, |id| id as i32)
            }
            None => 0,
        };
        let causing_id = entity_id(causing);
        let source = myriade_engine::health::HurtSource {
            damage_type,
            causing: causing_id,
            direct: entity_id(direct),
            position,
        };
        if crate::game::entity::damage_level_of!(self, scoped).invulnerability_check(
            myriade_engine::health::Hurting::Entity(id as i32, "minecraft:item", source),
            vanilla,
        ) {
            return false;
        }
        if !self
            .session
            .rules
            .bool_at(myriade_engine::gamerules::vanilla::MOB_GRIEFING)
            && matches!(causing, Some(Target::Mob(_)))
            || registry.in_tag(kind, "minecraft:is_fall")
                && self.entity_tag_matches("minecraft:fall_damage_immune", "minecraft:item")
        {
            return false;
        }
        if self.stack_can_be_hurt_by(&item.stack, damage_type) == Some(false)
            || registry.in_tag(kind, "minecraft:is_fire")
                && self.stack_can_be_hurt_by(&item.stack, "minecraft:in_fire") == Some(false)
        {
            return false;
        }
        // Java subtracts in float before truncating to int, including fractions
        // and accepted zero-damage hits.
        let health = (item.health - amount) as i32;
        self.ecs.set_item_health(id, health as f32);
        let at = item.position;
        self.sculk_prepare();
        let who = match causing {
            Some(Target::Player) => Some(Who::Player),
            Some(Target::Mob(id)) => Some(Who::Mob(id)),
            _ => None,
        };
        let context = self.who_context(who, None);
        self.post_game_event("minecraft:entity_damage", [at.x, at.y, at.z], context);
        if health <= 0 {
            if let Some(held) = self.ecs.item(id) {
                let mut stack = held.stack.clone();
                let mut spilled = crate::bundle::destroyed(&mut stack);
                spilled.extend(myriade_engine::item_data::destroyed_container(&mut stack));
                for s in spilled {
                    self.spawn_item_own_id(s, [at.x, at.y, at.z], 0);
                }
            }
            self.ecs_remove_item(id);
        }
        true
    }
}

impl Game<'_> {
    /// Whether a named causing entity is a creative player.
    pub(in crate::game) fn creative_damage_source_id(&self, id: i32) -> bool {
        id != 0
            && ((id == self.own_entity_id() && self.creative)
                || self.lan.as_ref().is_some_and(|host| {
                    host.hub.guests.iter().any(|g| g.entity_id == id && g.creative())
                }))
    }
    /// The `Invulnerable` flag and `invulnerable_time` window of entity `id`: the ECS entity's, else its saved data's.
    pub(in crate::game) fn saved_invulnerability(&self, id: u32) -> (bool, i32) {
        if self.ecs_has(id) {
            return (self.ecs_flag(id, "Invulnerable") == Some(true), self.ecs.invulnerable_time(id).unwrap_or(0));
        }
        match self.persist.raw.get(&id) {
            Some(Tag::Compound(map)) => (
                matches!(map.get(&JavaString::from("Invulnerable")), Some(Tag::Byte(1))),
                match map.get(&JavaString::from("invulnerable_time")) {
                    Some(Tag::Int(ticks)) => *ticks,
                    _ => 0,
                },
            ),
            _ => (false, 0),
        }
    }
    /// `Entity.isInvulnerableToBase` over a non-mob kind's saved flag and window: past only for
    /// `#bypasses_invulnerability` damage and a creative player's hit (`isCreativePlayer`).
    pub(in crate::game) fn saved_refuses_damage(&self, id: u32, damage_type: &str, creative_source: bool) -> bool {
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
    /// `DamageSource.isCreativePlayer`: the causing player's creative mode, the host's or a guest's.
    pub(in crate::game) fn creative_source_of(&self, cause: Option<Target>) -> bool {
        match cause {
            Some(Target::Player) => self.creative,
            Some(Target::Guest(conn)) => self.command_guest(conn).is_some_and(|g| g.creative()),
            _ => false,
        }
    }
    /// `ItemStack.canBeHurtBy`, shared by armour wear and detached stack reads.
    pub(in crate::game) fn stack_can_be_hurt_by(&self, stack: &crate::items::ItemStack, name: &str) -> Option<bool> {
        self.items().get(&stack.item)?;
        let default = self
            .res
            .item_defaults()
            .entries()
            .get(stack.item.as_ref())
            .and_then(|components| components.get("minecraft:damage_resistant"));
        myriade_engine::item_damage::stack_can_be_hurt_by(default, &self.health_resources.damage, stack, name)
    }
}
