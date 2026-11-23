use super::*;

pub(super) fn load_world_rest(
    resource_root: &horizon_assets::resource_source::ResourceRoot,
    resources: &Resources,
    assets: &Assets,
    behaviour_json: &serde_json::Value,
    gameplay_projection: &[myriade_engine::blocks::GameplayProjection],
    progress: &Progress,
    started: Instant,
) -> Result<WorldRest> {
    let root = resource_root.prepared();
    let facts = resources.facts();
    let biomes = resources.biomes();
    let threads = resources.threads();
    let registry = assets.registry;
    let content = myriade_engine::datapack::DataPack::from_source(
        "horizon-content".into(),
        resource_root.data_changes().clone(),
    );
    let files = myriade_engine::datapack::DataStore::load(&[&content])
        .files
        .into_iter()
        .collect::<Vec<_>>();
    let conditions = horizon_world::conditions::LoadConditions::vanilla();
    let overlay = &horizon_world::loot::PackOverlay {
        conditions: &conditions,
        files: &files,
    };
    progress.set(6);
    // Independent tables load together, within the worker budget the settings
    // allow. Only the recipes and the stations read another table: the items.
    let load_items = || -> Result<_> {
        let mut items = startup::time("rest.items", || {
            Items::load(root, registry, facts, overlay, resources.item_defaults())
        })?;
        items.bind_component_damage_catalog(&resources.motion().health.damage);
        log::info!("load items: {:.2}s", started.elapsed().as_secs_f32());
        progress.set(7);
        let sources = startup::time("rest.recipe_sources", || {
            lan::sync::initial_sources(root, &resources.core.item_data, overlay)
        })?;
        let recipes = startup::time("rest.recipes", || {
            Recipes::from_resolved(root, &items, overlay, &sources)
        })?;
        let stations = startup::time("rest.stations", || {
            myriade_engine::stations::StationData::from_sources(
                root,
                &items,
                overlay,
                resources.item_defaults(),
                (&sources, &recipes),
            )
        });
        Ok((items, recipes, stations))
    };
    let load_behaviours = || {
        startup::time("rest.behaviours", || {
            Behaviours::load_with_gameplay(registry, root, behaviour_json, gameplay_projection)
        })
    };
    let load_mobs = || -> Result<_> {
        let _step = startup::step("rest.mobs");
        let catalog = &resources.motion().attributes;
        let types = MobTypes::load_for_server(root, catalog)?;
        let mob_types: HashMap<_, _> = types
            .types
            .into_iter()
            .map(|(k, v)| (k, Arc::new(v)))
            .collect();
        let mob_data = myriade_engine::mobs::MobData::load(resource_root, facts);
        myriade_engine::trial_config::load_registry(root);
        Ok((mob_types, mob_data))
    };
    let load_transformers = || -> Result<_> {
        let _step = startup::step("rest.transformers_models");
        myriade_engine::block_transform::Transformers::load(root, registry)
    };
    // The errors cross threads as text.
    fn text<T, E: std::fmt::Display>(
        result: std::result::Result<T, E>,
    ) -> std::result::Result<T, String> {
        result.map_err(|e| e.to_string())
    }
    let (items, behaviours, mobs, transformers, providers, nether_environment, end_environment) =
        if threads > 1 {
            std::thread::scope(|scope| {
                let items = named_scope(scope, "items", || text(load_items()));
                let behaviours = named_scope(scope, "behaviours", || text(load_behaviours()));
                let mobs = named_scope(scope, "mobs", || text(load_mobs()));
                let transformers = named_scope(scope, "transformers", || text(load_transformers()));
                let providers = named_scope(scope, "providers", || {
                    startup::time("rest.providers", || Providers::load(root))
                });
                let nether_environment = named_scope(scope, "nether_environment", || {
                    startup::time("rest.nether_environment", || {
                        text(Environment::load_dimension(
                            &horizon_assets::resource_source::ResourceSource::directory(
                                root.to_path_buf(),
                            ),
                            "the_nether",
                        ))
                    })
                });
                let end_environment = named_scope(scope, "end_environment", || {
                    startup::time("rest.end_environment", || {
                        text(Environment::load_dimension(
                            &horizon_assets::resource_source::ResourceSource::directory(
                                root.to_path_buf(),
                            ),
                            "the_end",
                        ))
                    })
                });
                (
                    items.join().unwrap(),
                    behaviours.join().unwrap(),
                    mobs.join().unwrap(),
                    transformers.join().unwrap(),
                    providers.join().unwrap(),
                    nether_environment.join().unwrap(),
                    end_environment.join().unwrap(),
                )
            })
        } else {
            (
                text(load_items()),
                text(load_behaviours()),
                text(load_mobs()),
                text(load_transformers()),
                Providers::load(root),
                startup::time("rest.nether_environment", || {
                    text(Environment::load_dimension(
                        &horizon_assets::resource_source::ResourceSource::directory(root.to_path_buf()),
                        "the_nether",
                    ))
                }),
                startup::time("rest.end_environment", || {
                    text(Environment::load_dimension(
                        &horizon_assets::resource_source::ResourceSource::directory(root.to_path_buf()),
                        "the_end",
                    ))
                }),
            )
        };
    let (items, recipes, stations) = items?;
    let (mob_types, mob_data) = mobs?;
    let nether_environment = nether_environment?;
    let end_environment = end_environment?;
    log::info!(
        "Horizon: resources loaded in {:.1}s; {} block states, {} items, {} biomes; {} worker threads",
        started.elapsed().as_secs_f32(),
        facts.states.len(),
        items.items.len(),
        biomes.list.len(),
        threads
    );
    progress.set(8);
    Ok(WorldRest {
        stations,
        items,
        recipes,
        transformers: transformers?,
        providers,
        nether_environment,
        end_environment,
        behaviours: behaviours?,
        mob_types,
        mob_data,
        // The player and spin-attack models are drawn: none on the server.
        player_arm: [None, None],
        player_arm_slim: [None, None],
        player_type: None,
        spin_attack: None,
    })
}
