//! Entering a world without blocking the window: the deferred entry, world creation, the world
//! resources' background load, and the world's own content tables.
use super::*;

impl Game<'_> {
    /// The server's data-pack enchantments, or the local world's registry.
    pub(super) fn item_enchantments(&self) -> &Arc<myriade_engine::enchant::Enchantments> {
        &self.stations().enchantments
    }

    // ---- the world's data packs' content tables ---------------------------

    /// The content tables the world plays with: its data packs' over the
    /// game's own when it has packs, `Resources.rest`'s own otherwise.
    pub(super) fn items(&self) -> &Items {
        world_items!(self)
    }
    pub(super) fn mob_suffocation(
        &self,
    ) -> &horizon_world::suffocation::SuffocationRegistry<'static> {
        self.world_tables
            .as_ref()
            .map_or_else(|| self.res.suffocation(), |t| &t.suffocation)
    }
    pub(super) fn recipes(&self) -> &Recipes {
        world_recipes!(self)
    }
    pub(super) fn stations(&self) -> &myriade_engine::stations::StationData {
        world_stations!(self)
    }

    pub(super) fn pack_files(&self) -> horizon_world::loot::PackOverlay<'_> {
        pack_files(&self.world_tables)
    }
    /// Load the world's data packs' content tables (`None`: the world has no
    /// packs, and `Resources.rest` stays in use). This re-reads the tables,
    /// so it runs once per world open and on `/reload`.
    pub(super) fn stage_world_tables(
        &mut self,
        stack: &functions::PackStack,
        reload: bool,
        root: &std::path::Path,
    ) -> Result<(Option<WorldTables>, myriade_engine::health::HealthResources)> {
        let files = functions::overlay_files(stack, Vec::new());
        let conditions = horizon_world::conditions::LoadConditions::vanilla();
        let overlay = horizon_world::loot::PackOverlay {
            conditions: &conditions,
            files: &files,
        };
        let base = if reload {
            &self.health_resources
        } else {
            &self.res.motion().health
        };
        let health = base.with_packs(root, &overlay, reload)?;
        // With no pack, `res` holds the tables, unless content mods switched
        // live since it was loaded: their data is in the new root.
        let mut items = Items::load(
            root,
            self.pack.assets.registry,
            self.res.facts(),
            &overlay,
            self.res.item_defaults(),
        )?;
        class_tags::install(&mut items, self.res.facts());
        // Item damage predicates use the same frozen registry as player hurt.
        Arc::make_mut(&mut items.enchantments).bind_damage_catalog(&health.damage);
        items.bind_component_damage_catalog(&health.damage);
        let sources = if reload {
            lan::sync::prepare_server_sources(root, &self.res.core.item_data, &overlay)?
        } else {
            lan::sync::load_sources(
                root,
                &self.res.core.item_data,
                horizon_world::native_load_context::DataLoadOrigin::ServerDataLoad,
                &overlay,
            )?
        };
        let recipes = Recipes::from_resolved(root, &items, &overlay, &sources)?;
        let mut stations = myriade_engine::stations::StationData::from_sources(
            root,
            &items,
            &overlay,
            self.res.item_defaults(),
            (&sources, &recipes),
        );
        self.prepare_professions(&items, &mut stations)?;
        // A registry whose pack tags don't load keeps the game's own tags.
        let pack_tags = WorldTables::PACK_TAG_REGISTRIES
            .into_iter()
            .filter_map(|registry| match myriade_engine::items::load_tags(root, registry, &overlay) {
                Ok(tags) => Some((registry, tags)),
                Err(e) => {
                    log::warn!("Horizon: the world's {registry} tags are not applied: {e}");
                    None
                }
            })
            .collect();
        let causes_suffocation = items
            .block_tags
            .get("minecraft:causes_suffocation")
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        let suffocation = myriade_engine::mobs::suffocation::bind(
            self.res.core.collision.clone(),
            &causes_suffocation,
        )?;
        // Items already decoded the root's command export. Block-state-only
        // command metadata is not the registry universe, and professions and
        // biomes belong to the live world candidate rather than that export.
        for registry in ["minecraft:sound_event", "minecraft:game_event"] {
            if items.component_registry.ids(registry).is_empty() {
                return Err(format!(
                    "builtin data maps require {registry} in command-registries.json"
                )
                .into());
            }
        }
        conditions.check_errors()?;
        Ok((
            Some(WorldTables {
                conditions,
                suffocation,
                files,
                items: Arc::new(items),
                recipes,
                stations,
                reload_sources: reload.then_some(sources),
                pack_tags,
            }),
            health,
        ))
    }
    /// A world open's tables.
    pub(super) fn open_world_tables(
        &mut self,
        stack: &functions::PackStack,
        root: &std::path::Path,
    ) -> Result<()> {
        // A new world load: a saved base clamped into its bounds warns again (arismoko/horizon#3728).
        myriade_engine::living_attributes::LOAD_CLAMPS.begin_load();
        let (tables, health) = self.stage_world_tables(stack, false, root)?;
        self.world_tables = tables;
        let bonus = self.trident_hit_bonus_fn();
        self.ecs.set_trident_hit_bonus(bonus);
        self.ecs.set_melee_gear(self.melee_gear_fn());
        self.ecs.set_item_modifiers_table(self.item_modifiers_fn());
        self.health_resources = health;
        self.install_profession_save_snapshots();
        Ok(())
    }
    /// Reload the content tables with the world's packs, as
    /// `/reload` reloads a server's (`ReloadableServerResources`).
    /// Publish tables before function admission, leaving advancement rewards queued
    /// until the reload's new function store is available.
    pub(super) fn install_world_tables(&mut self, prepared: PreparedWorldReload) {
        let PreparedWorldReload {
            structure_spawns,
            tables,
            advancements,
            health,
            wire,
        } = prepared;
        lan::sync::forget_server_sources();
        if let Some(tables) = &tables
            && let Some(sources) = &tables.reload_sources
        {
            let overlay = horizon_world::loot::PackOverlay {
                conditions: &tables.conditions,
                files: &tables.files,
            };
            lan::sync::install_server_sources(
                &self.options.resources,
                &self.res.core.item_data,
                &overlay,
                sources.clone(),
            );
        }
        self.health_resources = health;
        self.world_tables = tables;
        let bonus = self.trident_hit_bonus_fn();
        self.ecs.set_trident_hit_bonus(bonus);
        self.ecs.set_melee_gear(self.melee_gear_fn());
        self.ecs.set_item_modifiers_table(self.item_modifiers_fn());
        self.install_profession_save_snapshots();
        self.world.pois.clear();
        for world in self.levels.parked_worlds() {
            world.pois.clear();
        }
        self.structure_spawns = structure_spawns;
        self.server_items = None;
        self.refresh_infiniburn();
        self.install_parts();
        self.damage_tag_generation = self.damage_tag_generation.wrapping_add(1);
        self.forget_book();
        myriade_engine::structure_blocks::forget_loot_data();
        self.load_server_loot();
        // PlayerList.reloadResources: the tags and recipes go to the players
        // and their advancements reload.
        lan::sync::install_wire_reload(
            &self.options.resources,
            &self.res.core.item_data,
            &self.pack_files(),
            wire.clone(),
        );
        if self.lan.is_some()
            && let Some(sync) = wire
        {
            self.with_advancement_hub(|hub, owners| hub.reload_resources(sync, owners));
        }
        self.adv.tree = advancements;
        self.adv.reset_tree();
    }

    pub(in crate::game) fn join_world_rest(&mut self) -> Result<()> {
        if self.res.rest.get().is_none() {
            return Err("world resources are unavailable".into());
        }
        Ok(())
    }
}

impl Game<'_> {
    pub(super) fn active_enchantment_definitions(&self) -> &myriade_engine::enchant::Enchantments {
        &self.stations().enchantments
    }
    /// Build and publish one server snapshot for this world load or reload.
    pub(super) fn load_server_loot(&mut self) {
        self.server_items = None;
        let overlay = pack_files(&self.world_tables);
        let loot = myriade_engine::structure_blocks::world_loot_data(
            &self.options.resources,
            &overlay,
            self.res.item_defaults(),
        );
        self.server_loot.set(loot);
    }

    /// Reload the content tables with the world's packs, as
    /// `/reload` reloads a server's (`ReloadableServerResources`).
    pub(super) fn prepare_world_reload(
        &mut self,
        stack: &functions::PackStack,
        root: &std::path::Path,
    ) -> Result<PreparedWorldReload> {
        let (tables, health) = self.stage_world_tables(stack, true, root)?;
        let overlay = pack_files(&tables);
        let kept = self
            .lan
            .as_ref()
            .map(|host| host.hub.lan.shared.sync())
            .or_else(lan::sync::cached_reload_sync);
        let wire = if let Some(kept) = kept {
            let sources = match tables
                .as_ref()
                .and_then(|tables| tables.reload_sources.clone())
            {
                Some(sources) => sources,
                None => {
                    lan::sync::prepare_server_sources(root, &self.res.core.item_data, &overlay)?
                }
            };
            Some(
                lan::sync::prepare_wire_reload(
                    root,
                    &self.res.core.item_data,
                    &overlay,
                    &kept,
                    &sources,
                )
                .map_err(|error| format!("wire reload: {error}"))?,
            )
        } else {
            None
        };
        let advancements = Arc::new(myriade_engine::advancements::Tree::load_with(
            &root.join("data"),
            &overlay,
        ));
        overlay.conditions.check_errors()?;
        Ok(PreparedWorldReload {
            advancements,
            structure_spawns: myriade_engine::mobs::spawn_place::StructureSpawns::load(root),
            tables,
            health,
            wire,
        })
    }

    pub(super) fn install_parts(&self) {
        self.resolve_block_paths();
        self.resolve_primary_items();
    }
    pub(super) fn resolve_block_paths(&self) {
        use myriade_engine::mobs::data::PartAnswers;
        let data = self.res.mob_data();
        data.resolve_parts(self.res.facts(), &PartAnswers { pathfindable: None });
    }
}
