//! The ECS's tick as `tick_mobs` runs it: the players' facts, one step of the dimension's schedule, the mob events it wrote
//! for the mobs' handler, then the rest for the host. A client's level is inert, so the tick returns at once.
use super::super::{Game, Result};
use horizon_client::ecs_bridge::{EcsLevel, events::apply};
use horizon_ecs_proto::bridge::host::HostSetup;
use horizon_gameplay::dim::Dim;

/// An item default as the NBT tag `decode` reads (`attack_range`): numbers as doubles, objects as compounds.
fn default_tag(v: &serde_json::Value) -> Option<horizon_core::nbt::Tag> {
    use horizon_core::nbt::{Compound, Tag};
    use serde_json::Value;
    Some(match v {
        Value::Null => return None,
        Value::Bool(b) => Tag::Byte(i8::from(*b)),
        Value::Number(n) => Tag::Double(n.as_f64()?.to_bits()),
        Value::String(s) => Tag::String(s.as_str().into()),
        Value::Array(a) => Tag::List(a.iter().map(default_tag).collect::<Option<Vec<_>>>()?),
        Value::Object(m) => {
            let mut c = Compound::new();
            for (k, x) in m {
                c.insert(k.as_str().into(), default_tag(x)?);
            }
            Tag::Compound(c)
        }
    })
}

/// The game's `living_visibility` answers, lent to the ECS for one step; `scaled_visibility` finds the listeners and the epoch.
struct LivingVisibilityAnswers<'g, 'a>(&'g Game<'a>);
impl horizon_ecs_proto::targeting::LivingVisibilityHost for LivingVisibilityAnswers<'_, '_> {
    fn visibility(&self, player: u32, looking: u32, visibility: f64) -> f64 {
        self.0.scaled_visibility(|epoch| mod_api::EntityRef::new(epoch, player), looking, visibility)
    }
}

impl Game<'_> {
    pub(in crate::game) fn tick_ecs(&mut self) -> Result<()> {
        if !self.ecs.is_active() {
            return Ok(());
        }
        self.sync_player_facts()?;
        // The level leaves the game for the step and the events: the view of the game borrows it otherwise.
        let mut ecs = std::mem::replace(&mut self.ecs, EcsLevel::inert());
        // The block writes since the last tick (`World::set_with_notification`): what waits on a block wakes to them.
        let writes = std::mem::take(self.world.path_writes.get_or_insert_with(Vec::new));
        if !writes.is_empty() {
            let mw = self.mob_world_base(&[]);
            for &((x, y, z), _, _) in &writes {
                ecs.set_block_event([x, y, z]);
                ecs.hook_block_changed(&mw, [x, y, z]);
            }
        }
        if let (Some(loot), Some(w)) = (self.server_loot.get(), ecs.ecs_mut()) {
            w.set_loot(loot);
        }
        for (cx, cz) in std::mem::take(&mut self.world.path_bulk) {
            ecs.chunk_rewritten(cx, cz);
        }
        // An item's end of life is held for the host only when a mod hears it (`ecs_item_expiring`); else P removes it itself.
        ecs.set_item_expiry_hooked(
            mod_api::__private::listened(mod_api::HookId::ItemExpire) || mod_api::__private::listened(mod_api::HookId::ItemDespawn),
        );
        // A conversion is held for the host only when a mod hears it (`ecs_conversion_asked`).
        ecs.set_conversion_watched(
            mod_api::__private::listened(mod_api::HookId::LivingConversionPre) || mod_api::__private::listened(mod_api::HookId::LivingConversionPost),
        );
        // A new target is held for the host only when a mod hears `living_change_target` (`ecs_target_asked`); else the brain takes it.
        ecs.set_target_watched(mod_api::__private::listened(mod_api::HookId::LivingChangeTarget));
        // A death is held for the host only when a mod hears it (`ecs_death_event`); else P removes the entity itself.
        ecs.set_death_vetoable(mod_api::__private::listened(mod_api::HookId::LivingDeathPost));
        // A lethal hit on a holder of a totem waits for the host only while a mod hears `living_use_totem` (`ecs_totem_asked`).
        ecs.set_totem_watched(mod_api::__private::listened(mod_api::HookId::LivingUseTotem));
        // `HitDealt` is emitted only while a mod hears `hurt_enemy` (`ecs_hit_dealt`).
        ecs.set_hit_watched(mod_api::__private::listened(mod_api::HookId::HurtEnemy));
        // Movement game events are posted only while a sculk block is loaded or a mod hears game events; the movement sounds always play.
        ecs.set_move_events(self.vibration_heard());
        // `SpecialDates.isHalloween`: P reads it at a spawn; the level is told only when the calendar date changes.
        ecs.refresh_date(
            || {
                use chrono::Datelike;
                let today = chrono::Local::now();
                [today.month(), today.day()]
            },
            |[month, day]| horizon_client::mobs::finalize::halloween_date(month, day),
        );
        // The mod goals of this step call their hooks under this native attempt's epoch; the game answers the reads the step does not hold.
        let g: &Game = &*self;
        let epoch = g.native_lifecycle.epoch();
        let level = epoch.map(|ep| mod_api::LevelRef::new(ep, g.dim.key()));
        let host = horizon_client::ecs_bridge::mod_goals::ModGoalsHost { epoch, level, game: g };
        // The natural pass's spawn hooks are lent only while a mod listens to one (`ecs_spawn_hooks`): else it asks nothing.
        let spawn_hooks = g.ecs_spawn_hooks();
        let spawn_host = spawn_hooks.as_ref().map(|h| h as &dyn horizon_ecs_proto::spawn_mod_host::SpawnModHost);
        // The mods' `living_visibility` answers are lent for this step, and only while a mod listens to it.
        let visibility = LivingVisibilityAnswers(g);
        ecs.lend_living_visibility(mod_api::__private::listened(mod_api::HookId::LivingVisibility).then_some(&visibility as &dyn horizon_ecs_proto::targeting::LivingVisibilityHost));
        let evs = ecs.step_with(&g.mob_world_base(&[]), &host, spawn_host);
        let mut mob = Vec::new();
        let kept = match ecs.world() {
            Some(world) => apply(world, evs, |m| {
                mob.append(m);
                Ok(())
            })?,
            None => evs,
        };
        self.ecs = ecs;
        self.handle_mob_events_reusable(&mut mob)?;
        // The blows the creatures landed on players (`Mob.doHurtTarget` on a player): each is the hit of the player it names.
        for (player, amount, from, by) in self.ecs.take_player_hits() {
            let mob = by.map(|e| self.ecs_id_of(e));
            self.with_player(player, |g| {
                g.combat.melee_mob = mob;
                let _ = g.handle_mob_events(vec![horizon_client::mobs::MobEvent::AttackPlayer(amount, from)]);
                g.combat.melee_mob = None;
            });
        }
        // The second knockback of those blows (`causeExtraKnockback`, along the creature's facing), by the knockback path of a player.
        for (player, from, (x, z, power), by) in self.ecs.take_player_pushes() {
            let mob = self.ecs_id_of(by);
            self.with_player(player, |g| {
                let event = horizon_client::mobs::MobEvent::Knockback {
                    target: horizon_client::mobs::Who::Player,
                    power,
                    x,
                    z,
                    source: horizon_client::mobs::Damage::mob(mob, from),
                    apply_blocking: None,
                };
                let _ = g.handle_mob_events(vec![event]);
            });
        }
        self.host_events(kept)
    }

    /// The ECS world of `dim`, seeded as its level is.
    pub(in crate::game) fn new_ecs_level(&self, dim: Dim, seed: i64) -> EcsLevel {
        self.build_ecs_level(HostSetup { seed, dimension: dim.id(), simulation_radius: self.settings.simulation_distance.max(2) })
    }

    /// An active level of the overworld, seed 0, whatever the role (the tests').
    #[cfg(test)]
    pub(in crate::game) fn test_ecs_level(&self) -> EcsLevel {
        let mut ecs = self.build_ecs_level(HostSetup { seed: 0, dimension: "minecraft:overworld", simulation_radius: 4 });
        // A creature far from every player sleeps (`zone`): the level's tests stand a player at the origin, as a game always has one.
        let player = horizon_ecs_proto::goals::Player { id: 1, x: 0.5, y: 64.0, z: 0.5, ..Default::default() };
        ecs.set_player(player, horizon_ecs_proto::bridge::host::PlayerFact { alive: true, targetable: true, hero: None, ..Default::default() });
        // The zone learns the players' places at a tick (`zone_step`, then the list refresh): two ticks, so what is spawned next is awake.
        for _ in 0..2 {
            ecs.step(&self.mob_world_base(&[]));
        }
        ecs
    }

    fn build_ecs_level(&self, setup: HostSetup) -> EcsLevel {
        // The species' rows are read once per process and referenced by every level, so the table lives as long as it.
        static TYPES: std::sync::OnceLock<horizon_client::mobs::MobTypes> = std::sync::OnceLock::new();
        let types = TYPES.get_or_init(|| horizon_client::mobs::MobTypes {
            types: self.res.mob_types().iter().map(|(id, kind)| (id.clone(), (**kind).clone())).collect(),
        });
        let mw = self.mob_world_base(&[]);
        let mut ecs = EcsLevel::new(&setup, &mw, types, &self.options.resources);
        // The natural spawning starts with the rules the session holds (`session_state`: `set_spawn_rules`) and the level's structures.
        let rules = &self.session.rules;
        let spawn_rules = (rules.bool_at(horizon_client::gamerules::vanilla::SPAWN_MOBS), rules.bool_at(horizon_client::gamerules::vanilla::SPAWN_MONSTERS));
        ecs.enable_spawning(&mw, setup.seed, spawn_rules, &self.structure_spawns);
        ecs.set_spawn_policy(self.ecs_spawn_policy());
        self.ecs_initial_pose(&mut ecs);
        // The trade registries are read by the merchants of every level; the item tables are kept for the process like `TYPES`.
        static TRADE_TABLES: std::sync::OnceLock<(horizon_client::items::Items, horizon_items::enchant::Enchantments)> = std::sync::OnceLock::new();
        let (items, enchantments) =
            TRADE_TABLES.get_or_init(|| (self.items().clone(), (*self.stations().enchantments).clone()));
        // P's profession indices are the host's (`stations().professions`, mods' professions included).
        let mut trades = self.stations().trades.clone();
        trades.professions = self.stations().professions.clone();
        ecs.install_trades(trades, items, enchantments, setup.seed);
        let (motives, placeable) = self.painting_variants.motives();
        ecs.set_motives(motives, placeable);
        ecs.set_trident_hit_bonus(self.trident_hit_bonus_fn());
        ecs.set_melee_gear(self.melee_gear_fn());
        ecs.set_item_resists_table(self.item_resists_fn());
        let res = self.res.clone();
        ecs.set_prototype_range_table(Box::new(move |item: &str| res.item_defaults().entries().get(item)?.get("minecraft:attack_range").and_then(default_tag)));
        ecs.set_item_modifiers_table(self.item_modifiers_fn());
        ecs
    }
}

#[cfg(test)]
mod grass_tests {
    use crate::game::ecs::test_support::active_server_game;
    use horizon_client::ecs_bridge::EcsLevel;
    use std::sync::Arc;

    /// Nine grass chunks around the origin, spawnable, a player at the origin: 1200 ticks of natural spawning put cows on them, and none
    /// with the mob rule off.
    fn cows_after_1200_ticks(spawn_mobs: bool) -> usize {
        let mut resources = None;
        let mut game = active_server_game(&mut resources);
        for cx in -1..=1 {
            for cz in -1..=1 {
                let mut chunk = horizon_client::chunk::Chunk::empty(cx, cz);
                chunk.lit = true;
                game.world.chunks.insert((cx, cz), Arc::new(chunk));
                game.block_ticks.add_empty_chunk(horizon_core::position::ChunkPos::new(cx, cz)).unwrap();
                game.ecs.set_chunk_spawnable(cx, cz, true);
            }
        }
        assert!(game.run_command("fill -16 63 -16 31 63 31 grass_block"));
        game.ecs.set_spawn_rules(spawn_mobs, false);
        for _ in 0..1200 {
            let mut ecs: EcsLevel = std::mem::replace(&mut game.ecs, EcsLevel::inert());
            let mw = game.mob_world_base(&[]);
            ecs.step(&mw);
            game.ecs = ecs;
        }
        game.ecs.species_count("minecraft:cow")
    }

    #[test]
    fn natural_spawning_puts_cows_on_a_grass_chunk_only_with_the_mob_rule() {
        assert!(cows_after_1200_ticks(true) > 0);
        assert_eq!(cows_after_1200_ticks(false), 0);
    }
}
