//! Potions and effects on the ECS level. An effect P has a kind for (`effects::Kind`) goes on the creature's `Effects` list by an event
//! (`effects::add_flagged`), read by the ECS on its own ticks: nothing polls it here. The effects P has no kind for (the instant ones,
//! the death effects of Oozing, Weaving, Wind Charged, Infested) stay in the host and act through the bridge. Each `ecs_*` method is the
//! ECS side of one branch of `items/potions.rs`; the host side stays in that branch's `else`.
use crate::game::Game;
use bevy_ecs::entity::Entity;
use horizon_client::{ecs_bridge::EcsLevel, mobs::MobWorld};
use horizon_core::{
    geometry::{Aabb, Vec3},
    nbt::{Compound, Tag},
};
use horizon_ecs_proto::{
    bridge::host::{HurtSource, Select},
    effects::{self, Kind},
};

/// The P kind of a vanilla effect id (`minecraft:` optional); None for an effect P has no kind for.
pub(in crate::game) fn effect_kind(id: &str) -> Option<Kind> {
    Some(match id.trim_start_matches("minecraft:") {
        "speed" => Kind::Speed,
        "slowness" => Kind::Slowness,
        "weakness" => Kind::Weakness,
        "poison" => Kind::Poison,
        "wither" => Kind::Wither,
        "regeneration" => Kind::Regeneration,
        "fire_resistance" => Kind::FireResistance,
        "water_breathing" => Kind::WaterBreathing,
        "invisibility" => Kind::Invisibility,
        "levitation" => Kind::Levitation,
        "hunger" => Kind::Hunger,
        "mining_fatigue" => Kind::MiningFatigue,
        "blindness" => Kind::Blindness,
        "jump_boost" => Kind::JumpBoost,
        "weaving" => Kind::Weaving,
        "strength" => Kind::Strength,
        "infested" => Kind::Infested,
        _ => return None,
    })
}

/// `ThrowableProjectile` hit box test for the ECS creature `e`: the feet centre, the width and height of its box (`Mob.kind` in the host).
fn hit_by(b: Aabb, p: Vec3, margin: f64) -> bool {
    let feet = b.bottom_center();
    let size = b.size();
    (feet.x - p.x).abs() < size.x / 2.0 + margin && (feet.z - p.z).abs() < size.x / 2.0 + margin && p.y >= feet.y && p.y <= feet.y + size.y
}

impl Game<'_> {
    /// Lends the level to `f` with the world's view of the game (as `tick_ecs` does), so the view does not borrow `self.ecs`.
    fn ecs_lend<R>(&mut self, f: impl FnOnce(&mut EcsLevel, &MobWorld<'_>) -> R) -> R {
        let mut ecs = std::mem::replace(&mut self.ecs, EcsLevel::inert());
        let r = f(&mut ecs, &self.mob_world_base(&[]));
        self.ecs = ecs;
        r
    }
    /// `LivingEntity.hasEffect` for the ECS creature `mob`; false for an effect P has no kind for.
    pub(in crate::game) fn ecs_mob_has_effect(&self, mob: u32, id: &str) -> bool {
        let (Some(kind), Some(e), Some(w)) = (effect_kind(id), self.ecs.entity_of(mob), self.ecs.world()) else {
            return false;
        };
        effects::has(w, e, kind)
    }
    /// `LivingEntity.addEffect` for the ECS creature `mob`, by event: `visible` false hides its particles, `ambient` marks it. False when
    /// the creature is not in the level, or P refuses the effect (a kind it already has, at a lower level or shorter).
    pub(in crate::game) fn ecs_mob_add_effect(&mut self, mob: u32, id: &str, duration: i32, amp: i32, visible: bool, ambient: bool) -> bool {
        let (Some(kind), Some(e)) = (effect_kind(id), self.ecs.entity_of(mob)) else {
            return false;
        };
        let flags = (if visible { 0 } else { effects::HIDDEN }) | (if ambient { effects::AMBIENT } else { 0 });
        self.ecs.world_mut().is_some_and(|w| effects::add_flagged(w, e, kind, amp, duration, flags))
    }
    /// The bounds of the ECS creature `mob`, None when it is not in the level.
    pub(in crate::game) fn ecs_mob_bounds(&self, mob: u32) -> Option<Aabb> {
        self.ecs.bounds(mob)
    }
    /// The effects of a cloud or an arrow on the ECS creature `mob`: an instant one hurts or heals it (`instant(base, amp)` is the amount,
    /// `inverted` swaps the two for the undead), a timed one is put by event and announced to the mods as the host does.
    pub(in crate::game) fn ecs_mob_effects(
        &mut self,
        mob: u32,
        effects: &[(&'static str, i32, i32)],
        hidden: &[&str],
        ambient: &[&str],
        instant: &dyn Fn(i32, i32) -> f32,
    ) {
        let (Some(e), Some(kind)) = (self.ecs.entity_of(mob), self.ecs.kind(mob)) else {
            return;
        };
        let tags = &self.res.stations().enchantments.entity_tags;
        let inverted = tags
            .get("minecraft:inverted_healing_and_harm")
            .is_some_and(|t| t.contains(&format!("minecraft:{}", kind.trim_start_matches("minecraft:"))));
        for &(id, duration, amp) in effects {
            match id {
                "instant_damage" | "instant_health" => {
                    let harm = (id == "instant_damage") != inverted;
                    let amount = instant(if harm { 6 } else { 4 }, amp);
                    if harm {
                        self.ecs_lend(|l, mw| l.hurt(mw, e, amount, HurtSource::Generic));
                    } else {
                        self.ecs_lend(|l, mw| l.heal(mw, e, amount));
                    }
                }
                _ => {
                    if self.can_be_affected(Some(mob as i32), kind, id)
                        && let Some(name) = super::super::potions::mob_effect_id(id)
                    {
                        let new = !self.ecs_mob_has_effect(mob, name);
                        self.ecs_mob_add_effect(mob, name, duration, amp, !hidden.contains(&id), ambient.contains(&id));
                        self.mod_effect_added(name, Some(mob as i32), amp, new);
                    }
                }
            }
        }
    }
    /// `OozingMobEffect.numberOfSlimesToSpawn` on the ECS level: how many slimes fit under the cramming limit `limit` near the dying
    /// `bounds` (`bearer` is not counted).
    pub(in crate::game) fn ecs_oozing_count(&self, bounds: Aabb, bearer: Option<Entity>, limit: i32) -> i32 {
        let near = bounds.inflate(Vec3::new(2.0, 2.0, 2.0));
        let size = bounds.size();
        let feet = bounds.bottom_center();
        let q = Select { kind: None, tag: None, near: Some(([feet.x, feet.y, feet.z], 4.0 + size.y)), limit: usize::MAX };
        let nearby = self
            .ecs
            .select(&q)
            .into_iter()
            .filter(|&e| {
                Some(e) != bearer
                    && self.ecs.kind_of(e).is_some_and(|k| k.trim_start_matches("minecraft:") == "slime")
                    && self.ecs.bounds_of(e).is_some_and(|b| b.intersects(near))
            })
            .count()
            .min(limit as usize) as i32;
        (limit - nearby).clamp(0, 2)
    }
    /// A size 2 slime half a block above `at` (`spawnSlimeOffspring`), by the ECS spawn.
    pub(in crate::game) fn ecs_spawn_slime_offspring(&mut self, at: [f64; 3]) {
        let mut nbt = Compound::new();
        nbt.insert("Size".into(), Tag::Int(2));
        self.ecs_spawn("minecraft:slime", [at[0], at[1] + 0.5, at[2]], Some(&nbt));
    }
    /// One silverfish of Infested at `at`, with the velocity its bearer's random drew.
    pub(in crate::game) fn ecs_spawn_silverfish(&mut self, at: [f64; 3], v: Vec3) {
        if let Some(id) = self.ecs_spawn("minecraft:silverfish", at, None)
            && let Some(e) = self.ecs.entity_of(id)
        {
            self.ecs.set_velocity(e, v);
        }
    }
    /// The creature a thrown potion meets on its step to `p` (id, feet, is a breeze), skipping `vehicle` until the thrower has left it
    /// (`left_owner`). Read through the spatial index, not a scan of the level; the id is linked on the hit.
    pub(in crate::game) fn ecs_thrown_mob_at(&mut self, p: Vec3, vehicle: Option<u32>, left_owner: bool) -> Option<(u32, Vec3, bool)> {
        let vehicle = vehicle.and_then(|v| self.ecs.entity_of(v));
        let q = Select { kind: None, tag: None, near: Some(([p.x, p.y, p.z], 4.0)), limit: usize::MAX };
        let hit = self.ecs.select(&q).into_iter().find(|&e| {
            (left_owner || Some(e) != vehicle) && self.ecs.bounds_of(e).is_some_and(|b| hit_by(b, p, 0.125))
        })?;
        let feet = self.ecs.bounds_of(hit)?.bottom_center();
        let breeze = self.ecs.kind_of(hit).is_some_and(|k| k.trim_start_matches("minecraft:") == "breeze");
        Some((self.ecs_link(hit), feet, breeze))
    }
    /// A sound at the ECS creature `mob`'s position, `volume` and `pitch` as the host plays them. `source` and `except_actor` are the
    /// host's arguments of `play_mob_bound_sound`; the audio has no entry for them yet (listed in the report).
    pub(in crate::game) fn ecs_mob_sound(&mut self, mob: u32, name: &str, source: usize, volume: f32, pitch: f32, except_actor: bool) {
        let _ = (source, except_actor);
        let Some(at) = self.ecs.position(mob) else { return };
        self.audio.play(name, Some(at), volume, pitch);
    }
    /// The ECS side of `ThrownSplashPotion.applySplash` for the creatures in reach: each takes its instant effects (harm or heal by
    /// the nearness, undead inverted) or its timed ones (kept while longer than 20 ticks), then its reaction by the bridge
    /// (`splashed`, the Enderman's `hurtServer` response included).
    pub(in crate::game) fn ecs_splash_mobs(&mut self, at: Vec3, age: u32, effects: &[(&'static str, i32, i32)], duration_scale: f32) {
        let q = Select { kind: None, tag: None, near: Some(([at.x, at.y, at.z], 4.0)), limit: usize::MAX };
        let mut hits = Vec::new();
        for e in self.ecs.select(&q) {
            let Some(b) = self.ecs.bounds_of(e) else { continue };
            if let Some(d2) = super::super::potions::splash_distance_sqr(at, age, b) {
                hits.push((e, 1.0 - d2.sqrt() / 4.0));
            }
        }
        for (e, intensity) in hits {
            let id = self.ecs_link(e);
            for &(name, duration, amp) in effects {
                match name {
                    "instant_damage" | "instant_health" => {
                        let amount = (intensity * f64::from(6 << amp.min(30)) + 0.5) as i32 as f32;
                        let harm = (name == "instant_damage") != self.ecs_mob_inverted(e);
                        if harm {
                            self.ecs_lend(|l, mw| l.hurt(mw, e, amount, HurtSource::Generic));
                        } else {
                            self.ecs_lend(|l, mw| l.heal(mw, e, amount));
                        }
                    }
                    _ => {
                        let d = (intensity * f64::from(duration) * f64::from(duration_scale) + 0.5) as i32;
                        if let Some(n) = super::super::potions::mob_effect_id(name)
                            && d > 20
                        {
                            self.ecs_mob_add_effect(id, n, d, amp, true, false);
                        }
                    }
                }
            }
            self.ecs_lend(|l, mw| l.splashed(mw, e));
        }
    }
    /// Whether the ECS entity `e` is undead, which inverts healing and harming.
    fn ecs_mob_inverted(&self, e: Entity) -> bool {
        let tags = &self.res.stations().enchantments.entity_tags;
        self.ecs.kind_of(e).is_some_and(|k| {
            tags.get("minecraft:inverted_healing_and_harm")
                .is_some_and(|t| t.contains(&format!("minecraft:{}", k.trim_start_matches("minecraft:"))))
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs_bridge::EcsLevel;
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_ecs_proto::{
        bridge::host::HostSetup,
        effects::{self, Effects},
    };

    fn level(game: &crate::game::Game<'_>) -> EcsLevel {
        game.test_ecs_level()
    }

    #[test]
    fn a_hidden_potion_effect_is_put_on_the_ecs_creature_by_event() {
        with_game(|game| {
            game.ecs = level(game);
            let id = game.ecs_spawn("minecraft:cow", [0.5, 64.0, 0.5], None).unwrap();
            assert!(game.ecs_mob_add_effect(id, "minecraft:invisibility", 600, 0, false, false));
            assert!(game.ecs_mob_has_effect(id, "invisibility"));
            let e = game.ecs.entity_of(id).unwrap();
            let w = game.ecs.world().unwrap();
            let list = &w.get::<Effects>(e).unwrap().list;
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].flags & effects::HIDDEN, effects::HIDDEN, "a hidden effect keeps its particles off");
        });
    }

    #[test]
    fn oozing_on_an_ecs_death_makes_two_slimes_unless_the_cramming_limit_is_full() {
        with_game(|game| {
            game.ecs = level(game);
            let bearer = game.ecs_spawn("minecraft:zombie", [0.5, 64.0, 0.5], None).unwrap();
            let be = game.ecs.entity_of(bearer).unwrap();
            let bounds = game.ecs.bounds(bearer).unwrap();
            assert_eq!(game.ecs_oozing_count(bounds, Some(be), 24), 2);
            game.ecs_spawn("minecraft:slime", [0.5, 64.0, 0.5], None).unwrap();
            assert_eq!(game.ecs_oozing_count(bounds, Some(be), 1), 0, "the one slime beside fills the limit");
            assert_eq!(game.ecs_oozing_count(bounds, Some(be), 24), 2, "the limit is not full");
        });
    }
}
