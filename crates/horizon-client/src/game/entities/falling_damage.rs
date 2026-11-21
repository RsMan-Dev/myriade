//! Landing impacts for falling blocks, before their placement/drop decision,
//! and `Block.fallOn` for every entity that isn't living.
use super::Game;
use horizon_core::{
    geometry::{Aabb, Vec3, VoxelShape},
    position::BlockPos,
    random::RandomSource,
};
use horizon_world::{
    block_entity_push::BlockPushWorld,
    block_landing::{
        BlockLandingWorld, DamageSourceOwner, LandingDamage, LandingEntityKind, LandingEvent,
        LandingSound,
    },
    entity_obstruction::EntityIdentity,
    registry::BlockStateId,
};

/// An entity that isn't living, landing (`Entity.checkFallDamage` -> `Block.fallOn`).
/// Only farmland (the level random's draw), honey (the slide sound and entity
/// event 54) and the fall-damage calls reach one: it writes no block.
struct NonLivingLanding<'g, 'a> {
    game: &'g mut Game<'a>,
    id: u32,
    at: Vec3,
    bounds: Aabb,
    /// What `causeFallDamage` was called with: a falling block hurts by it.
    fall_damage: Option<f64>,
}
impl BlockPushWorld for NonLivingLanding<'_, '_> {
    type Error = String;
    fn collision_shape(&mut self, state: BlockStateId, p: BlockPos) -> Result<VoxelShape, String> {
        let mut scene = horizon_client::collisions::SceneCollisions {
            world: &self.game.world,
            binding: self.game.collision,
            subject: None,
        };
        self.game
            .collision
            .collision_shape(&mut scene, state, [p.x, p.y, p.z])
            .map_err(|e| e.to_string())
    }
    fn entities(&mut self, _: Aabb) -> Result<Vec<EntityIdentity>, String> {
        Ok(Vec::new())
    }
    fn entity_bounds(&self, _: EntityIdentity) -> Aabb {
        self.bounds
    }
    fn teleport_relative(&mut self, _: EntityIdentity, _: Vec3) -> Result<(), String> {
        Ok(())
    }
}
impl BlockLandingWorld for NonLivingLanding<'_, '_> {
    type DamageSource = ();
    type Sound = Option<&'static str>;
    fn is_server_level(&self) -> bool {
        true
    }
    fn is_client_side(&mut self) -> Result<bool, String> {
        Ok(false)
    }
    // `level.getRandom()`.
    fn random_float(&mut self) -> Result<f32, String> {
        Ok(self.game.world.random.next_float())
    }
    fn random_int(&mut self, bound: i32) -> Result<i32, String> {
        self.game
            .world
            .random
            .next_int_bound(bound)
            .map_err(|e| e.to_string())
    }
    fn under_spawn_protection(&mut self, _: BlockPos) -> Result<bool, String> {
        Ok(false)
    }
    fn mob_griefing(&mut self) -> Result<bool, String> {
        Ok(false)
    }
    fn entity_dimensions(&self) -> (f32, f32) {
        let size = self
            .bounds
            .max_position()
            .subtract(self.bounds.min_position());
        (size.x as f32, size.y as f32)
    }
    fn suppressing_bounce(&mut self) -> Result<bool, String> {
        Ok(false)
    }
    fn damage_source(&mut self, _: DamageSourceOwner, _: LandingDamage) -> Result<(), String> {
        Ok(())
    }
    // `Entity.causeFallDamage` is false for all of these; a falling block's
    // override hurts what it lands in (the caller does, by this distance).
    fn cause_fall_damage(&mut self, distance: f64, _: f32, _: ()) -> Result<bool, String> {
        self.fall_damage = Some(distance);
        Ok(false)
    }
    fn known_sound(&self, sound: LandingSound) -> Option<&'static str> {
        match sound {
            LandingSound::HoneySlide => Some("minecraft:block.honey_block.slide"),
            LandingSound::HoneyFall => Some("minecraft:block.honey_block.fall"),
            LandingSound::TurtleEggBreak => Some("minecraft:entity.turtle.egg_break"),
            LandingSound::ShelfMushroomBounce => None,
        }
    }
    fn living_fall_sound(&mut self, _: bool) -> Result<Option<&'static str>, String> {
        Ok(None)
    }
    // `Entity.playSound`: none from a silent entity, on `getSoundSource()` (NEUTRAL).
    fn entity_sound(
        &mut self,
        sound: Option<&'static str>,
        volume: f32,
        pitch: f32,
    ) -> Result<(), String> {
        if let Some(name) = sound
            && !self.game.entity_silent(self.id)
        {
            let at = [self.at.x, self.at.y, self.at.z];
            self.game.audio.play_on(6, name, Some(at), volume, pitch);
        }
        Ok(())
    }
    fn block_sound(
        &mut self,
        _: BlockPos,
        _: Option<&'static str>,
        _: f32,
        _: f32,
    ) -> Result<(), String> {
        Ok(())
    }
    // `Level.broadcastEntityEvent`.
    fn broadcast_entity_event(&mut self, event: u8) -> Result<(), String> {
        let at = [self.at.x, self.at.y, self.at.z];
        self.game.lan_entity_event(self.id as i32, event as i8, at);
        Ok(())
    }
    fn destroy_block(&mut self, _: BlockPos, _: bool) -> Result<bool, String> {
        Ok(false)
    }
    fn set_block(&mut self, _: BlockPos, _: BlockStateId, _: u32) -> Result<bool, String> {
        Ok(false)
    }
    fn block_event(
        &mut self,
        _: LandingEvent,
        _: BlockPos,
        _: BlockStateId,
        _: bool,
    ) -> Result<(), String> {
        Ok(())
    }
    fn level_event(&mut self, _: i32, _: BlockPos, _: i32) -> Result<(), String> {
        Ok(())
    }
}

impl Game<'_> {
    /// `Block.fallOn` for entity `id`, which isn't living, landing at `at`
    /// (its box `bounds`) after falling `fall`, on its supporting block (else
    /// `getOnPosLegacy`). Returns the distance it called `causeFallDamage`
    /// with: none on powder snow, the fall plus 2.5 on a stalagmite's tip,
    /// half on a bed. A client runs none of it.
    pub(super) fn non_living_fall_on(
        &mut self,
        id: u32,
        at: Vec3,
        bounds: Aabb,
        fall: f64,
    ) -> Option<f64> {
        let pos = self.landing_pos(at, bounds);
        let state = self.world.block(pos.x, pos.y, pos.z);
        let motion = self.motion;
        let mut landing = NonLivingLanding {
            game: self,
            id,
            at,
            bounds,
            fall_damage: None,
        };
        if let Err(e) =
            motion
                .landing
                .fall_on(&mut landing, state, pos, LandingEntityKind::Other, fall)
        {
            log::warn!("Horizon: fallOn: {e}");
        }
        landing.fall_damage
    }
    /// `HostEvent::Landed` of a non-living body of P (primed TNT, falling block) that fell `fall`: `fallOn` of the block under it, then HIT_GROUND.
    pub(in crate::game) fn ecs_landed(&mut self, id: u32, at: [f64; 3], fall: f64) {
        let at = horizon_core::geometry::Vec3::new(at[0], at[1], at[2]);
        let bounds = horizon_client::falling_block::bounds(at);
        self.non_living_fall_on(id, at, bounds, fall);
        self.post_landing_event(at, bounds, false);
    }
    /// The hurt of a player by a falling block (`Hurt` of P): a falling block names no attacker, so none is set.
    pub(in crate::game) fn falling_hurt_player(&mut self, position: Vec3, amount: f32, kind: &str) {
        if self.creative || self.spectator() {
            return;
        }
        if self.player.as_ref().is_none_or(|p| {
            p.health.dead_or_dying() || !horizon_client::falling_block::bounds(position).intersects(p.bounds())
        }) {
            return;
        }
        let Some(id) = self.health_resources.damage.id(kind) else {
            return;
        };
        let attack = horizon_client::health::Attack {
            position: Some([position.x, position.y, position.z]),
            ..Default::default()
        };
        let outcome = self
            .hurt_player_equipment(id, amount, None, false, 0, false, &attack, None)
            .unwrap_or_default();
        if outcome.hurt {
            // LivingEntity.hurtServer: ENTITY_HURT_PLAYER, the falling block
            // (`DamageSources.fallingBlock(this)` and the like) as the direct entity.
            let block = horizon_client::advancements::EntityView {
                kind: "minecraft:falling_block".into(),
                position: [position.x, position.y, position.z],
                ..Default::default()
            };
            self.trigger_entity_hurt_player(&horizon_client::advancements::DamageView {
                kind: kind.to_owned(),
                direct: Some(block.clone()),
                is_direct: true,
                source: Some(block),
                original: amount,
                actual: outcome.dealt,
                blocked: false,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::game::{Game, native_entity_refs::tests::with_game};
    use horizon_client::{items::ItemStack, motion::PlayerMotion};
    use horizon_core::{equipment_slot::EquipmentSlot, geometry::Vec3};
    use mod_api::{
        ArmorHurtEvent, DamageItemEvent, Flow, Hooks, ItemPart, LevelRandomHost,
        LivingIncomingDamageEvent, Registrar, RegistrationContext, ServerHost,
    };
    use std::sync::{Arc, Mutex};

    const ANVIL: &str = "minecraft:falling_anvil";

    /// What the probe answers and what it heard of a hit.
    #[derive(Default)]
    struct Log {
        refuse_hit: bool,
        cancel_wear: bool,
        new_damage: Option<f32>,
        incoming: u32,
        /// `armor_hurt`: each call's slots and vanilla damage.
        armor: Vec<Vec<(EquipmentSlot, f32)>>,
        /// `damage_item` of the iron helmet: the points, and whether its host names the acting player.
        worn: Vec<(i32, bool)>,
    }
    #[derive(Clone)]
    struct Probe(Arc<Mutex<Log>>);
    #[mod_api::__subscriptions]
    impl Hooks for Probe {
        fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
            let helmet = r.item_key("minecraft:iron_helmet");
            r.item_part(helmet, self.clone());
        }
        fn living_incoming_damage(
            &self,
            _: &mut dyn LevelRandomHost,
            _: &mut LivingIncomingDamageEvent<'_>,
        ) -> Flow {
            let mut log = self.0.lock().unwrap();
            log.incoming += 1;
            if log.refuse_hit {
                Flow::Cancel
            } else {
                Flow::Continue
            }
        }
        fn armor_hurt(&self, _: &mut dyn ServerHost, event: &mut ArmorHurtEvent<'_>) -> Flow {
            let mut log = self.0.lock().unwrap();
            let call = event.entries.iter().map(|e| (e.slot, e.damage)).collect();
            log.armor.push(call);
            if let Some(new_damage) = log.new_damage {
                for entry in event.entries.iter_mut() {
                    entry.new_damage = new_damage;
                }
            }
            if log.cancel_wear {
                Flow::Cancel
            } else {
                Flow::Continue
            }
        }
    }
    #[mod_api::horizon_part]
    impl ItemPart for Probe {
        fn damage_item(&self, host: &mut dyn ServerHost, event: &mut DamageItemEvent<'_>) -> Flow {
            let acting = host.players().acting_player().is_some();
            self.0.lock().unwrap().worn.push((event.amount, acting));
            Flow::Continue
        }
    }

    /// Runs `test` as a helmeted server player, with the probe's log.
    fn with_player(test: impl FnOnce(&mut Game<'_>, &Mutex<Log>)) {
        let log = Arc::new(Mutex::new(Log::default()));
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
            "probe",
            Box::new(Probe(log.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods.clone(), || {
            crate::game::api_values::install_parts(&mods).unwrap();
            with_game(|game| {
                game.lan_acting_uuid = Some(123);
                game.lan_acting_entity = Some(20);
                game.lan_acting_name = Some("Steve".into());
                game.lan_acting_permission = Some(0);
                test(game, &log);
            })
        });
    }
    /// A new player in `helmet`, and a log with no answers: a hit of 8 wears 2 points.
    fn start(game: &mut Game<'_>, log: &Mutex<Log>, helmet: Option<ItemStack>) {
        game.player = Some(PlayerMotion::new(game.motion, Vec3::ZERO).unwrap());
        game.armor[0] = helmet;
        game.apply_equipment();
        game.refresh_armor_protection();
        *log.lock().unwrap() = Log::default();
    }
    fn hit(game: &mut Game<'_>) {
        game.falling_hurt_player(Vec3::ZERO, 8.0, ANVIL);
    }
    fn health(game: &Game<'_>) -> f32 {
        game.player.as_ref().unwrap().health.health()
    }
    fn iron_helmet() -> ItemStack {
        ItemStack::new("minecraft:iron_helmet", 1)
    }

    #[test]
    #[ignore = "porté dans P : falling_land"]
    fn a_canceled_wear_leaves_the_helmet_and_its_share_of_the_hit() {
        with_player(|game, log| {
            start(game, log, Some(iron_helmet()));
            log.lock().unwrap().cancel_wear = true;
            hit(game);
            assert_eq!(game.armor[0], Some(iron_helmet()));
            {
                let log = log.lock().unwrap();
                assert_eq!(
                    log.armor,
                    [
                        vec![(EquipmentSlot::Head, 2.0)],
                        vec![(EquipmentSlot::Head, 1.5)]
                    ]
                );
                assert!(log.worn.is_empty());
                assert_eq!(log.incoming, 1);
            }
            // 8 less the helmet's quarter is 6; 2 armor points leave 98.4% of it.
            assert!((health(game) - 14.096).abs() < 0.001, "{}", health(game));

            // Both equipment operations already ran; draining events cannot repeat them.
            let events = std::mem::take(&mut game.player.as_mut().unwrap().events);
            game.wear_armor(&events);
            assert_eq!(log.lock().unwrap().armor.len(), 2);
            assert_eq!(game.armor[0], Some(iron_helmet()));
        });
    }

    #[test]
    #[ignore = "porté dans P : falling_land"]
    fn the_answered_wear_runs_on_the_real_host_before_the_cooldown_refuses() {
        with_player(|game, log| {
            start(game, log, Some(iron_helmet()));
            log.lock().unwrap().new_damage = Some(7.0);
            hit(game);
            assert_eq!(game.armor[0].as_ref().unwrap().damage, 14);
            assert_eq!(log.lock().unwrap().worn, [(7, true), (7, true)]);
            let after_first = health(game);

            // The cooldown takes the same hit whole, after `hurtHelmet` has worn the piece.
            log.lock().unwrap().new_damage = None;
            hit(game);
            assert_eq!(game.armor[0].as_ref().unwrap().damage, 16);
            assert_eq!(health(game), after_first);
            assert_eq!(log.lock().unwrap().incoming, 2);
        });
    }

    #[test]
    #[ignore = "porté dans P : falling_land"]
    fn a_helmet_the_hit_breaks_stops_protecting_in_that_hit() {
        with_player(|game, log| {
            let mut helmet = iron_helmet();
            helmet.damage = game.items().get(&helmet.item).unwrap().max_damage - 1;
            helmet.enchant("minecraft:protection", 4);
            start(game, log, Some(helmet));
            let status = &game.player.as_ref().unwrap().status;
            assert_eq!(status.protection(ANVIL), 4.0);
            hit(game);
            assert_eq!(game.armor[0], None);
            // The quarter stays: `hurtServer` takes it whatever the helmet became.
            assert_eq!(health(game), 14.0);
            assert_eq!(log.lock().unwrap().incoming, 1);
        });
    }

    #[test]
    #[ignore = "porté dans P : falling_land"]
    fn no_helmet_or_a_refused_hit_fires_no_helmet_wear() {
        with_player(|game, log| {
            start(game, log, None);
            hit(game);
            assert!(log.lock().unwrap().armor.is_empty());
            assert_eq!(health(game), 12.0);

            start(game, log, Some(iron_helmet()));
            log.lock().unwrap().refuse_hit = true;
            hit(game);
            assert!(log.lock().unwrap().armor.is_empty());
            assert_eq!(game.armor[0], Some(iron_helmet()));
            assert_eq!(health(game), 20.0);
        });
    }
}

#[cfg(test)]
#[path = "falling_damage/helmet_tests.rs"]
mod helmet_tests;
