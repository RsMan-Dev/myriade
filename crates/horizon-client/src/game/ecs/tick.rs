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
        self.build_ecs_level(HostSetup { seed: 0, dimension: "minecraft:overworld", simulation_radius: 4 })
    }

    fn build_ecs_level(&self, setup: HostSetup) -> EcsLevel {
        // The species' rows are read once per process and referenced by every level, so the table lives as long as it.
        static TYPES: std::sync::OnceLock<horizon_client::mobs::MobTypes> = std::sync::OnceLock::new();
        let types = TYPES.get_or_init(|| horizon_client::mobs::MobTypes {
            types: self.mob_types.iter().map(|(id, kind)| (id.clone(), (**kind).clone())).collect(),
        });
        let mut ecs = EcsLevel::new(&setup, &self.mob_world_base(&[]), types, &self.options.resources);
        ecs.set_trident_hit_bonus(self.trident_hit_bonus_fn());
        ecs.set_item_resists_table(self.item_resists_fn());
        ecs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Audio, Options, Progress, Settings, execution_role::ExecutionRole, load_resources};

    #[test]
    fn a_client_ticks_no_ecs() {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            false,
            ExecutionRole::Client,
            &Progress::new(),
            &mut |_, _| unreachable!(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.game_dir = Some(std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap()).join("ecs-tick-client"));
        options.execution_role = ExecutionRole::Client;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            loaded.gui.unwrap(),
            options,
            settings,
            Audio::silent(&horizon_assets::resource_source::ResourceSource::directory(root.to_path_buf())),
            None,
        )
        .unwrap();
        game.build_registries();
        assert!(!game.ecs_role_active());
        assert!(!game.ecs.is_active());
        game.tick_ecs().unwrap();
        assert!(!game.ecs.is_active());
    }
}
