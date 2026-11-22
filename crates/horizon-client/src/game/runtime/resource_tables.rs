//! The game's shared read-only tables: `Resources` (blocks, items, recipes, mob types, biomes),
//! `UiResources` and `PackResources`.
use super::*;
use horizon_client::item_names::Names;

impl Resources {
    pub(super) fn rest(&self) -> &WorldRest {
        self.rest
            .get()
            .expect("world resources joined before entry")
    }
    pub(super) fn facts(&self) -> &BlockFacts {
        &self.core.facts
    }
    pub(super) fn item_defaults(&self) -> &horizon_world::item_defaults::ItemDefaults {
        self.core.item_data.defaults()
    }
    pub(super) fn items(&self) -> &Items {
        &self.rest().items
    }
    pub(super) fn recipes(&self) -> &Recipes {
        &self.rest().recipes
    }
    pub(super) fn transformers(&self) -> &horizon_client::block_transform::Transformers {
        &self.rest().transformers
    }
    pub(super) fn biomes(&self) -> &Arc<Biomes> {
        &self.core.biomes
    }
    pub(super) fn environment(&self) -> &Environment {
        &self.core.environment
    }
    /// The environment of a dimension type id; a type with no table of its own gets the overworld's.
    pub(super) fn environment_of(&self, dimension_type: &str) -> &Environment {
        match dimension_type {
            "minecraft:the_nether" => self.nether_environment(),
            "minecraft:the_end" => self.end_environment(),
            _ => self.environment(),
        }
    }
    pub(super) fn mob_types(&self) -> &HashMap<String, Arc<MobType>> {
        &self.rest().mob_types
    }
    pub(super) fn mob_data(&self) -> &horizon_client::mobs::MobData {
        &self.rest().mob_data
    }
    pub(super) fn providers(&self) -> &Providers {
        &self.rest().providers
    }
    pub(super) fn nether_environment(&self) -> &Environment {
        &self.rest().nether_environment
    }
    pub(super) fn end_environment(&self) -> &Environment {
        &self.rest().end_environment
    }
    pub(super) fn behaviours(&self) -> &Behaviours {
        &self.rest().behaviours
    }
    pub(super) fn collision(&self) -> &BlockCollisionRegistry<'static> {
        &self.core.collision
    }
    pub(super) fn suffocation(&self) -> &horizon_world::suffocation::SuffocationRegistry<'static> {
        &self.core.suffocation
    }
    pub(super) fn motion(&self) -> &MotionResources {
        &self.core.motion
    }
    /// The variant registries finalizeSpawn selects from
    /// (`data/minecraft/<registry>`), shared with world generation.
    pub fn spawn_registries(&self) -> &horizon_client::mobs::finalize::SpawnRegistries {
        &self.core.spawn_registries
    }
    pub(super) fn threads(&self) -> usize {
        self.core.threads
    }
    pub(super) fn stations(&self) -> &horizon_client::stations::StationData {
        &self.rest().stations
    }
}

impl PackResources {
    /// The workers' context over `biomes`: this pack's own when `biomes` is
    /// the table it was built with, else the same parts with that table.
    pub(super) fn context_with(&self, biomes: Arc<Biomes>) -> Arc<WorkerContext> {
        let old = &self.context;
        if Arc::ptr_eq(&old.biomes, &biomes) {
            return old.clone();
        }
        Arc::new(WorkerContext { assets: old.assets.clone(), biomes, light: old.light.clone() })
    }
    /// The pack tables over their parts, which `load_resources` builds on separate workers.
    pub(super) fn assemble(
        root: &horizon_assets::resource_source::ResourceRoot,
        assets: Arc<Assets>,
        biomes: Arc<Biomes>,
        light: Arc<LightTables>,
    ) -> Self {
        Self {
            context: Arc::new(WorkerContext { assets: assets.clone(), biomes, light }),
            block_registries: native_block_states::block_registries(assets.registry),
            assets,
            source: root.source().clone(),
            data_source: root.data().clone(),
            data_changes: root.data_changes().clone(),
            additional_data: Arc::from([]),
            names: Arc::new(Names::load(root.source())),
            // The packs' Polytone environment layers are drawn only: none on the server.
            environments: None,
        }
    }
}
