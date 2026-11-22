//! Reads of a living entity's state by network id (greffe L0, lot 42): effects, flags, attributes, pose, equipment and capabilities,
//! read from the ECS world through the level's table. The effects come back as the registry path builds them.
use super::level::EcsLevel;
use horizon_client::{effects::EffectInstance, items::ItemStack};
use horizon_client_core::potions::{self, EffectDef};
use horizon_core::geometry::Aabb;
use horizon_ecs_proto::{
    attr_mod::Operation,
    behaviours::ender_teleport::WaterSensitive,
    bridge::{host::Flag, host_read, host_read_living::{effect_kind, effect_name}},
    effects::{AMBIENT, Effect, HIDDEN, Kind, NO_ICON},
    goals::Head,
};
use horizon_world::attribute_map::ValueKind;
use mod_api::{AttributeModifier, AttributeOperation, Identifier};

/// The kind of the registry effect at `index`, with its definition; None for a mod's effect (its name is not `minecraft:`).
fn kind_at(index: usize) -> Option<(&'static EffectDef, Kind)> {
    let def = potions::effect_by_index(index)?;
    Some((def, effect_kind(&format!("minecraft:{}", def.id))?))
}

/// The instance of `def` with the effect's amplifier, flags and remaining ticks (the icon shows as the particles do).
fn instance(def: &EffectDef, x: Effect, remaining: i32) -> EffectInstance {
    let mut i = EffectInstance::from_def(def, remaining, x.amp);
    i.ambient = x.flags & AMBIENT != 0;
    i.visible = x.flags & HIDDEN == 0;
    i.show_icon = i.visible && x.flags & NO_ICON == 0;
    i
}

/// The state name a flag reads by (`EcsWorld::flag`).
fn flag_name(flag: Flag) -> Option<&'static str> {
    Some(match flag {
        Flag::OnFire => "on_fire",
        Flag::Sleeping => "sleeping",
        Flag::Invisible => "invisible",
        Flag::Frozen => "frozen",
        Flag::Gliding => "gliding",
    })
}

impl EcsLevel {
    /// The facts a loot predicate reads about the entity `id` (`/loot ... kill`); None when inert or gone.
    pub fn loot_entity(&self, id: u32) -> Option<horizon_world::loot::LootEntity> {
        self.ecs()?.loot_entity(self.entity_of(id)?)
    }

    /// `LivingEntity.getEffect` for the registry effect at `index`, or None when `id` has none.
    pub fn effect(&self, id: u32, index: usize) -> Option<EffectInstance> {
        let (ecs, e) = (self.ecs()?, self.entity_of(id)?);
        let (def, kind) = kind_at(index)?;
        Some(instance(def, ecs.effect(e, kind)?, ecs.effect_remaining(e, kind)?))
    }
    pub fn has_effect(&self, id: u32, index: usize) -> bool {
        let (Some(ecs), Some(e)) = (self.ecs(), self.entity_of(id)) else { return false };
        kind_at(index).is_some_and(|(_, kind)| ecs.has_effect(e, kind))
    }
    /// `LivingEntity.getActiveEffects`: every effect of `id`, empty when it has none.
    pub fn effects(&self, id: u32) -> Vec<EffectInstance> {
        let (Some(ecs), Some(e)) = (self.ecs(), self.entity_of(id)) else { return Vec::new() };
        ecs.effects(e)
            .into_iter()
            .filter_map(|x| Some(instance(potions::effect(effect_name(x.kind))?, x, ecs.effect_remaining(e, x.kind)?)))
            .collect()
    }
    /// The mods' swirls kept on the entity `id` (`ModifiedSwirls`), as the host last left them.
    pub fn modified_swirls(&self, id: u32) -> Option<crate::effects::ModifiedEffectParticles> {
        self.ecs()?.modified_swirls(self.entity_of(id)?)
    }
    /// Keeps the mods' swirls on the entity `id`, or clears them with None; nothing when inert.
    pub fn set_modified_swirls(&mut self, id: u32, swirls: Option<crate::effects::ModifiedEffectParticles>) {
        let Some(e) = self.entity_of(id) else { return };
        if let Some(w) = self.ecs_mut() {
            w.set_modified_swirls(e, swirls);
        }
    }
    pub fn flag(&self, id: u32, flag: Flag) -> Option<bool> {
        self.ecs()?.flag(self.entity_of(id)?, flag_name(flag)?)
    }
    /// The value of an attribute: `Computed` with its modifiers, `Base` without them.
    pub fn attribute(&self, id: u32, name: &str, kind: ValueKind) -> Option<f64> {
        let (ecs, e) = (self.ecs()?, self.entity_of(id)?);
        let v = ecs.attribute(e, name)?;
        Some(match kind {
            ValueKind::Computed => v,
            ValueKind::Base => v - ecs.attribute_modifiers(e, name).iter().map(|m| m.amount).sum::<f64>(),
        })
    }
    pub fn attribute_modifiers(&self, id: u32, name: &str) -> Option<Vec<AttributeModifier>> {
        let (ecs, e) = (self.ecs()?, self.entity_of(id)?);
        ecs.attribute(e, name)?;
        Some(
            ecs.attribute_modifiers(e, name)
                .into_iter()
                .filter_map(|m| {
                    let op = match m.operation {
                        Operation::AddValue => AttributeOperation::AddValue,
                        Operation::AddMultipliedBase => AttributeOperation::AddMultipliedBase,
                        Operation::AddMultipliedTotal => AttributeOperation::AddMultipliedTotal,
                    };
                    Some(AttributeModifier::new(Identifier::parse(&m.id).ok()?, m.amount, op))
                })
                .collect(),
        )
    }
    pub fn pose_dimensions(&self, id: u32, pose: u8) -> Option<(f32, f32, f32)> {
        self.ecs()?.pose_dimensions(self.entity_of(id)?, pose)
    }
    pub fn equipment_stack(&self, id: u32, slot: usize) -> Option<ItemStack> {
        self.ecs()?.equipment_stack(self.entity_of(id)?, slot)
    }
    pub fn can_freeze(&self, id: u32) -> Option<bool> {
        Some(self.ecs()?.can_freeze(self.entity_of(id)?))
    }
    pub fn breathes_water(&self, id: u32) -> Option<bool> {
        Some(self.ecs()?.breathes_water(self.entity_of(id)?))
    }
    pub fn melee_reaches(&self, id: u32, bounds: Aabb) -> Option<bool> {
        Some(self.ecs()?.melee_reaches(self.entity_of(id)?, bounds))
    }
    /// Whether the entity `id` stands in water (`FluidState`); false when absent.
    pub fn in_water(&self, id: u32) -> bool {
        let (Some(w), Some(e)) = (self.world(), self.entity_of(id)) else { return false };
        host_read::in_water(w, e)
    }
    /// The category name of the species of `id` (`Mob.category`); None when absent.
    pub fn category(&self, id: u32) -> Option<&'static str> {
        host_read::category(self.world()?, self.entity_of(id)?)
    }
    /// `Mob.isEnemy` of `id`; false when absent.
    pub fn is_enemy(&self, id: u32) -> bool {
        let (Some(w), Some(e)) = (self.world(), self.entity_of(id)) else { return false };
        host_read::is_enemy(w, e)
    }
    /// Whether the entity `id` is in its first tick (`Mob.first_tick`); false when absent.
    pub fn first_tick(&self, id: u32) -> bool {
        let (Some(w), Some(e)) = (self.world(), self.entity_of(id)) else { return false };
        host_read::first_tick(w, e)
    }
    /// The largest half width of the mob types, the reach of a box around a block.
    pub fn max_half_width(&self) -> f32 {
        host_read::max_half_width()
    }
    /// Whether the creature `id` is hurt by water (`WaterSensitive`).
    pub fn water_sensitive(&self, id: u32) -> bool {
        let (Some(w), Some(e)) = (self.world(), self.entity_of(id)) else { return false };
        w.get::<WaterSensitive>(e).is_some()
    }
    /// `Entity.getLookAngle` of `id`: the view vector of its head's pitch and its yaw (`Mob.look_angle`); None without a yaw.
    pub fn look_angle(&self, id: u32) -> Option<[f64; 3]> {
        use horizon_core::geometry::trig::Mth;
        let (w, e) = (self.world()?, self.entity_of(id)?);
        let to_rad = std::f32::consts::PI / 180.0;
        let x = w.get::<Head>(e).map_or(0.0, |h| h.pitch) * to_rad;
        let y = -host_read::yaw(w, e)? * to_rad;
        let (y_cos, y_sin, x_cos, x_sin) = (y.mcos(), y.msin(), x.mcos(), x.msin());
        Some([f64::from(y_sin * x_cos), f64::from(-x_sin), f64::from(y_cos * x_cos)])
    }
}
