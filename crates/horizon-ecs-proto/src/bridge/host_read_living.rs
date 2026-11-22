//! The reads of a living entity's state (greffe L0, lot 42): effects, flags, attributes, pose, equipment and the capabilities a host asks
//! of an entity. Each read goes to the component that holds the state, by entity: no cache, no scan.
use crate::attr_mod::{AttrMods, Modifier, Operation};
use crate::attributes::ArmorModifiers;
use crate::behaviours::bed::Sleeping;
use crate::bridge::host::EcsWorld;
use crate::cow::{AttackDamage, Body, Bounds, Burning, Frozen, MaxHealth, Resist, now_of};
use crate::effects::{self, CanBreatheUnderWater, Effect, Effects, Invisible, Kind};
use crate::gliding::FallFlying;
use crate::equipment::Armor;
use crate::targeting::{Eye, FollowRange};
use crate::type_tags;
use bevy_ecs::{entity::Entity, prelude::*};
use horizon_client_lib::items::ItemStack;
use horizon_core::geometry::{Aabb, Vec3};

/// The flags a host reads by name (`EcsWorld::flag`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flag {
    OnFire,
    Sleeping,
    Invisible,
    Frozen,
    Gliding,
}
/// `Mob.DEFAULT_ATTACK_REACH` (`Math.sqrt(2.04) - 0.6`): the reach of a held item that has no `ATTACK_RANGE` (not modelled here).
fn default_attack_reach() -> f64 {
    2.04f64.sqrt() - 0.6
}

/// The vanilla registry names of the effects this crate models, with their kind.
const NAMES: [(&str, Kind); 19] = [
    ("minecraft:speed", Kind::Speed),
    ("minecraft:slowness", Kind::Slowness),
    ("minecraft:weakness", Kind::Weakness),
    ("minecraft:poison", Kind::Poison),
    (crate::species::lists::WITHER_EFFECT, Kind::Wither),
    ("minecraft:regeneration", Kind::Regeneration),
    ("minecraft:fire_resistance", Kind::FireResistance),
    ("minecraft:water_breathing", Kind::WaterBreathing),
    ("minecraft:invisibility", Kind::Invisibility),
    ("minecraft:levitation", Kind::Levitation),
    ("minecraft:hunger", Kind::Hunger),
    ("minecraft:mining_fatigue", Kind::MiningFatigue),
    ("minecraft:blindness", Kind::Blindness),
    ("minecraft:jump_boost", Kind::JumpBoost),
    ("minecraft:glowing", Kind::Glowing),
    ("minecraft:weaving", Kind::Weaving),
    ("minecraft:strength", Kind::Strength),
    ("minecraft:infested", Kind::Infested),
    ("minecraft:absorption", Kind::Absorption),
];

/// The kind of the effect named `name` (`minecraft:poison`); None for any other name, a mod's effect included.
pub fn effect_kind(name: &str) -> Option<Kind> {
    NAMES.iter().find(|(n, _)| *n == name).map(|(_, k)| *k)
}

/// The registry name of `kind` (`minecraft:poison`).
pub fn effect_name(kind: Kind) -> &'static str {
    NAMES.iter().find(|(_, k)| *k == kind).map_or("", |(n, _)| n)
}

/// The standing pose's ordinal (`Pose.STANDING`).
const STANDING: u8 = 0;

impl EcsWorld {
    /// The loot-predicate facts of the entity `e` (see `loot::entity_for_loot`).
    pub fn loot_entity(&self, e: Entity) -> Option<horizon_world::loot::LootEntity> {
        crate::loot::entity_for_loot(self.world(), e)
    }

    /// The world, when `e` exists: a dead entity answers no flag.
    fn existing(&self, e: Entity) -> Option<&World> {
        let w = self.world();
        w.get_entity(e).ok().map(|_| w)
    }
    /// The effect of `kind` on `e`.
    pub fn effect(&self, e: Entity, kind: Kind) -> Option<Effect> {
        self.world().get::<Effects>(e)?.list.iter().find(|x| x.kind == kind).copied()
    }
    pub fn has_effect(&self, e: Entity, kind: Kind) -> bool {
        effects::has(self.world(), e, kind)
    }
    /// All the effects of `e`, empty when it has none.
    pub fn effects(&self, e: Entity) -> Vec<Effect> {
        self.world().get::<Effects>(e).map_or_else(Vec::new, |f| f.list.clone())
    }

    /// The mods' swirls kept on `e` (`ModifiedSwirls`), while the host keeps them.
    pub fn modified_swirls(&self, e: Entity) -> Option<horizon_client_render::effects::ModifiedEffectParticles> {
        self.world().get::<crate::effects::ModifiedSwirls>(e).map(|s| s.0.clone())
    }

    /// Keeps the mods' swirls on `e`, or clears them with None (`effects::set_modified_swirls`).
    pub fn set_modified_swirls(&mut self, e: Entity, swirls: Option<horizon_client_render::effects::ModifiedEffectParticles>) {
        crate::effects::set_modified_swirls(self.world_mut(), e, swirls);
    }
    /// The ticks left on the effect of `kind`: its last tick less the entity's own time.
    pub fn effect_remaining(&self, e: Entity, kind: Kind) -> Option<i32> {
        let w = self.world();
        let x = self.effect(e, kind)?;
        i32::try_from(x.ends.saturating_sub(now_of(w, e))).ok()
    }
    /// The state a flag names: `on_fire`, `sleeping`, `invisible`, `frozen`, `gliding`. Any other name is None.
    pub fn flag(&self, e: Entity, name: &str) -> Option<bool> {
        let w = self.existing(e)?;
        let now = now_of(w, e);
        Some(match name {
            "on_fire" => w.get::<Burning>(e).is_some_and(|b| b.until > now as i64),
            "sleeping" => w.get::<Sleeping>(e).is_some(),
            "invisible" => w.get::<Invisible>(e).is_some(),
            "frozen" => w.get::<Frozen>(e).is_some_and(|f| f.0 > 0),
            "gliding" => w.get::<FallFlying>(e).is_some(),
            _ => return None,
        })
    }
    /// The value of an attribute, by its name with or without `minecraft:`. A component the entity lacks is None, except the armour,
    /// which a creature without `Armor` has at 0.
    pub fn attribute(&self, e: Entity, name: &str) -> Option<f64> {
        let w = self.world();
        Some(match name.strip_prefix("minecraft:").unwrap_or(name) {
            "max_health" => match w.get::<MaxHealth>(e) {
                Some(m) => f64::from(m.0),
                // The base only: the modifiers come with `attribute_modifiers`.
                None => f64::from(w.get::<crate::cow::SpeciesId>(e).map(|_| crate::cow::base_max_health(w, e))?),
            },
            "knockback_resistance" => w.get::<Resist>(e)?.0,
            "attack_damage" => f64::from(w.get::<AttackDamage>(e)?.0),
            "follow_range" => w.get::<FollowRange>(e)?.0,
            "armor" => w.get::<Armor>(e).map_or(0.0, |a| f64::from(a.armor)),
            "armor_toughness" => w.get::<Armor>(e).map_or(0.0, |a| f64::from(a.toughness)),
            "movement_speed" => w.get::<Body>(e)?.speed,
            "fall_damage_multiplier" => 1.0,
            _ => return None,
        })
    }
    /// The modifiers of an attribute: the armour's (`ArmorModifiers`, added values) or those the host wrote (`AttrMods`).
    pub fn attribute_modifiers(&self, e: Entity, name: &str) -> Vec<Modifier> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        if name == "armor" {
            let armour = |m: &ArmorModifiers| {
                m.0.iter().map(|(id, amount)| Modifier { id: (*id).to_owned(), amount: *amount, operation: Operation::AddValue, permanent: false }).collect()
            };
            let mut out: Vec<Modifier> = self.world().get::<ArmorModifiers>(e).map_or_else(Vec::new, armour);
            out.extend(self.world().get::<AttrMods>(e).and_then(|l| l.0.iter().find(|x| x.0 == name)).map_or_else(Vec::new, |x| x.2.clone()));
            out.extend(crate::equip_attrs::of_attribute(self.world(), e, name));
            return out;
        }
        let mut out = self.world().get::<AttrMods>(e).and_then(|l| l.0.iter().find(|x| x.0 == name)).map_or_else(Vec::new, |x| x.2.clone());
        out.extend(crate::equip_attrs::of_attribute(self.world(), e, name));
        out.extend(crate::effects::attribute_modifiers(self.world(), e, name));
        out
    }
    /// The width, the height and the eyes' height of a pose. Only the standing pose is modelled; `Body` already holds the adult or
    /// baby size that `set_baby` wrote.
    pub fn pose_dimensions(&self, e: Entity, pose: u8) -> Option<(f32, f32, f32)> {
        if pose != STANDING {
            return None;
        }
        let w = self.world();
        let b = w.get::<Body>(e)?;
        Some((b.width, b.height, w.get::<Eye>(e)?.0 as f32))
    }
    /// The whole stack in `slot` (order of `equip_stack::SLOTS`), read from the `Equipment` component.
    pub fn equipment_stack(&self, e: Entity, slot: usize) -> Option<ItemStack> {
        crate::equip_stack::get(self.world(), e, slot)
    }
    /// Whether `e` can freeze: it is not in `minecraft:freeze_immune_entity_types`.
    pub fn can_freeze(&self, e: Entity) -> bool {
        !type_tags::named(self.world(), "freeze_immune_entity_types", e)
    }
    /// Whether `e` breathes under water (`can_breathe_under_water`).
    pub fn breathes_water(&self, e: Entity) -> bool {
        type_tags::is::<CanBreatheUnderWater>(self.world(), e)
    }
    /// `Mob.isWithinMeleeAttackRange` for a held item without `ATTACK_RANGE`: the box of `e` grown by the default reach on x and z
    /// (`getAttackBoundingBox`, no vertical growth) meets `target`.
    pub fn melee_reaches(&self, e: Entity, target: Aabb) -> bool {
        let Some(b) = self.world().get::<Bounds>(e) else { return false };
        let r = default_attack_reach();
        b.0.inflate(Vec3::new(r, 0.0, r)).intersects(target)
    }
}
