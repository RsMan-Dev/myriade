//! What other mobs see of a cow (`Mob::seen`, cow constants).
use super::view::CowItem;
use crate::mobs::{Seen, Species, visibility};
use std::sync::Arc;

impl CowItem<'_, '_> {
    /// The category name of the cow's type, as `Mob::category`.
    fn category(&self) -> &'static str {
        match self.id.kind.category.as_str() {
            "monster" => "monster",
            "creature" => "creature",
            "ambient" => "ambient",
            "axolotls" => "axolotls",
            "underground_water_creature" => "underground_water_creature",
            "water_creature" => "water_creature",
            "water_ambient" => "water_ambient",
            _ => "misc",
        }
    }
    /// `LivingEntity.isPushable` of a cow.
    pub fn pushable(&self) -> bool {
        self.alive() && !self.motion.on_climbable
    }
    #[inline]
    pub fn seen(&self) -> Seen {
        Seen {
            kind: Arc::clone(&self.id.kind),
            in_water: self.fluids.in_water,
            playing_dead: false,
            last_damage: self.vitals.last_damage.map(|(d, _)| d),
            id: self.id.id,
            species: self.species(),
            position: self.motion.position,
            bounds: self.bounds(),
            hitbox: self.hitbox(),
            eye_y: self.eye_y(),
            alive: self.alive(),
            removed: self.vitals.removed,
            invulnerable: self.is_invulnerable(),
            baby: self.is_baby(),
            in_love: self.extra.in_love > 0,
            tame: false,
            bred: false,
            owner: None,
            invisible: false,
            hostile: false,
            target: self.ai.target,
            health: self.vitals.health,
            variant: self.extra.variant,
            size: 0,
            color: 0,
            job: None,
            bed: None,
            yaw: self.motion.yaw,
            velocity: self.motion.velocity,
            known_speed: self.motion.position.subtract(self.motion.previous),
            leash: None,
            caravan: 0,
            school: (None, 0),
            section: ([0; 3], 0),
            delay: 0,
            hurt_by: self.ai.last_hurt_by.map(|w| (w, self.ai.last_hurt_by_time)),
            cover: visibility::Cover { armor: 0.0, head: None },
            panicking: self.ai.panicking,
            active_raid: false,
            opened_chest: None,
            passenger: false,
            vehicle: None,
            pushable: self.pushable(),
            sleeping: false,
            cold_strider: false,
            category: self.category(),
            potential_job: None,
            meeting: None,
            profession: None,
            custom_profession: None,
            secondary_variant: 0,
            xp: 0,
            villager: None,
            stats: [f64::from(self.attrs.max_health), 0.0, self.attrs.speed],
            sitting: false,
            silent: self.clk.silent,
            lying: false,
            ridden: false,
            is_vehicle: false,
            still: false,
            carrying: false,
            collisions: 0,
            knockback_resistance: self.attrs.knockback_resistance,
            effects: Vec::new(),
            hunted_recently: false,
            hurt_recently: false,
            huntable: false,
        }
    }
}
const _: Species = Species::Cow;
