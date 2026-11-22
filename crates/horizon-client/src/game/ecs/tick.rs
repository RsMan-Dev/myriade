//! The ECS's tick as `tick_mobs` runs it: the players' facts, one step of the dimension's schedule, the mob events it wrote
//! for the mobs' handler, then the rest for the host. A client's level is inert, so the tick returns at once.
use super::super::{Game, Result};
use horizon_client::ecs_bridge::{EcsLevel, events::apply};
use horizon_ecs_proto::bridge::host::HostSetup;
use horizon_gameplay::dim::Dim;

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
        // A death is held for the host only when a mod hears it (`ecs_death_event`); else P removes the entity itself.
        ecs.set_death_vetoable(mod_api::__private::listened(mod_api::HookId::LivingDeathPost));
        // A lethal hit on a holder of a totem waits for the host only while a mod hears `living_use_totem` (`ecs_totem_asked`).
        ecs.set_totem_watched(mod_api::__private::listened(mod_api::HookId::LivingUseTotem));
        // `HitDealt` is emitted only while a mod hears `hurt_enemy` (`ecs_hit_dealt`).
        ecs.set_hit_watched(mod_api::__private::listened(mod_api::HookId::HurtEnemy));
        // Movement game events cost a little per walking entity: only while a sculk block is loaded or a mod hears game events.
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
        let evs = ecs.step(&self.mob_world_base(&[]));
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
        self.host_events(kept)
    }

    /// Only the logical server (integrated or dedicated) simulates a world, so only its level runs an ECS.
    pub(in crate::game) fn ecs_role_active(&self) -> bool {
        self.options.execution_role.is_logical_server()
    }

    /// The ECS world of `dim`, seeded as its level is; inert on a client.
    pub(in crate::game) fn new_ecs_level(&self, dim: Dim, seed: i64) -> EcsLevel {
        if !self.ecs_role_active() {
            return EcsLevel::inert();
        }
        self.build_ecs_level(HostSetup { seed, dimension: dim.id(), simulation_radius: self.settings.simulation_distance.max(2) })
    }

    /// An active level of the overworld, seed 0, whatever the role (the tests').
    #[cfg(test)]
    pub(in crate::game) fn test_ecs_level(&self) -> EcsLevel {
        let mut ecs = self.build_ecs_level(HostSetup { seed: 0, dimension: "minecraft:overworld", simulation_radius: 4 });
        // A creature far from every player sleeps (`zone`): the level's tests stand a player at the origin, as a game always has one.
        let player = horizon_ecs_proto::goals::Player { id: 1, x: 0.5, y: 64.0, z: 0.5, ..Default::default() };
        ecs.set_player(player, horizon_ecs_proto::bridge::host::PlayerFact { alive: true, targetable: true, hero: None });
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
        let mut ecs = EcsLevel::new(&setup, &self.mob_world_base(&[]), types, &self.options.resources);
        // The trade registries are read by the merchants of every level; the item tables are kept for the process like `TYPES`.
        static TRADE_TABLES: std::sync::OnceLock<(horizon_client::items::Items, horizon_items::enchant::Enchantments)> = std::sync::OnceLock::new();
        let (items, enchantments) =
            TRADE_TABLES.get_or_init(|| (self.items().clone(), (*self.stations().enchantments).clone()));
        // P's profession indices are the host's (`stations().professions`, mods' professions included).
        let mut trades = self.stations().trades.clone();
        trades.professions = self.stations().professions.clone();
        ecs.install_trades(trades, items, enchantments, setup.seed);
        ecs.set_trident_hit_bonus(self.trident_hit_bonus_fn());
        ecs.set_melee_gear(self.melee_gear_fn());
        ecs.set_item_resists_table(self.item_resists_fn());
        ecs.set_item_modifiers_table(self.item_modifiers_fn());
        ecs
    }
}
