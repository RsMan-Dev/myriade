#![cfg(test)]
use crate::game::{
    native_entity_refs::tests::{run_fresh_cases, with_game_role},
};
use horizon_core::{
    geometry::Direction,
    position::{BlockPos, ChunkPos},
    random::LegacyRandom,
};
use horizon_world::{
    level::{Behaviours, BlockEntityNeighbors, LevelAccess, PushReaction as EngineReaction, Ticks},
    registry::{BlockRegistry, BlockStateId},
};
use mod_api::*;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
};

struct ReactionPart {
    enabled: Arc<AtomicBool>,
    reaction: Option<PushReaction>,
    unbreakable: Option<Arc<AtomicU8>>,
}
#[mod_api::horizon_part]
impl BlockPart for ReactionPart {
    fn piston_push_reaction(&self, event: &mut PistonPushReactionEvent) -> Option<PushReaction> {
        if !self.enabled.load(Ordering::Relaxed) {
            return None;
        }
        if let Some(value) = &self.unbreakable {
            return Some(match value.load(Ordering::Relaxed) {
                0 => PushReaction::PushPull,
                1 => PushReaction::Push,
                2 => PushReaction::Popped,
                3 => PushReaction::Immoveable,
                4 => PushReaction::IgnoreEntity,
                _ => unreachable!(),
            });
        }
        self.reaction.clone().or_else(|| {
            Some(if event.state.ordinal() == 0 {
                PushReaction::Immoveable
            } else {
                PushReaction::PushPull
            })
        })
    }
}
struct User {
    enabled: Arc<AtomicBool>,
    parts: bool,
    unbreakable: Arc<AtomicU8>,
}
#[mod_api::__subscriptions]
impl Hooks for User {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for (id, reaction) in [
            ("test:lock", None),
            ("test:fixed", Some(PushReaction::PushPull)),
            ("minecraft:chest", Some(PushReaction::Push)),
            ("minecraft:bedrock", Some(PushReaction::PushPull)),
        ] {
            let key = r.block_key(id);
            if self.parts {
                r.block_part(
                    key,
                    ReactionPart {
                        enabled: self.enabled.clone(),
                        reaction,
                        unbreakable: (id == "minecraft:bedrock").then(|| self.unbreakable.clone()),
                    },
                );
            }
        }
    }
    fn blocks(&self, _: &RegistrationContext, event: &mut BlocksEvent<'_>) {
        let mut block = Block::new("test:lock", "minecraft:stone");
        block
            .state_properties
            .insert("locked".into(), BlockStateProperty::Boolean);
        block.default_state.insert("locked".into(), "false".into());
        event.blocks.push(block);
        let mut fixed = Block::new("test:fixed", "minecraft:stone");
        fixed.properties.push_reaction = Some(PushReaction::Immoveable);
        event.blocks.push(fixed);
    }
}
struct Level {
    blocks: HashMap<BlockPos, BlockStateId>,
    air: BlockStateId,
    time: i64,
    random: LegacyRandom,
}
impl BlockEntityNeighbors for Level {}
impl LevelAccess for Level {
    fn block_state(&self, pos: BlockPos) -> BlockStateId {
        self.blocks.get(&pos).copied().unwrap_or(self.air)
    }
    fn set_raw(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
        Some(self.blocks.insert(pos, state).unwrap_or(self.air))
    }
    fn is_loaded(&self, _: BlockPos) -> bool {
        true
    }
    fn random(&mut self) -> &mut LegacyRandom {
        &mut self.random
    }
    fn game_time(&self) -> i64 {
        self.time
    }
}
fn state(registry: &BlockRegistry, name: &str, props: &[(&str, &str)]) -> BlockStateId {
    registry
        .resolve(&horizon_world::chunk::BlockState {
            name: name.into(),
            properties: props
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        })
        .unwrap()
}
fn exercise(
    b: &Behaviours,
    registry: &BlockRegistry,
    block: BlockStateId,
    moved: bool,
    popped: bool,
    pull: bool,
) {
    let base = BlockPos::new(0, 1, 0);
    let start = BlockPos::new(1, 1, 0);
    let end = BlockPos::new(2, 1, 0);
    let power_pos = BlockPos::new(-1, 1, 0);
    let piston = state(registry, "minecraft:sticky_piston", &[("facing", "east")]);
    let power = registry.default_state("minecraft:redstone_block").unwrap();
    let stone = registry.default_state("minecraft:stone").unwrap();
    let mut level = Level {
        blocks: HashMap::new(),
        air: registry.default_state("minecraft:air").unwrap(),
        time: 0,
        random: LegacyRandom::new(42),
    };
    let mut ticks = Ticks::new(0);
    for x in -1..=1 {
        for z in -1..=1 {
            ticks.add_empty_chunk(ChunkPos::new(x, z)).unwrap();
        }
    }
    for x in -2..=3 {
        level.blocks.insert(BlockPos::new(x, 0, 0), stone);
    }
    level.blocks.extend([(base, piston), (start, block)]);
    b.set_block(&mut level, &mut ticks, power_pos, power, 3);
    for _ in 0..4 {
        level.time += 1;
        b.tick_level_blocks(&mut level, &mut ticks);
        b.tick_block_entities(&mut level, &mut ticks);
    }
    assert_eq!(
        level.block_state(end),
        if moved {
            block
        } else {
            registry.default_state("minecraft:air").unwrap()
        },
        "extension destination"
    );
    if !moved && !popped {
        assert_eq!(level.block_state(start), block, "refused block stays");
    }
    if popped {
        assert_ne!(level.block_state(start), block, "bed pops");
    }
    if moved {
        b.set_block(
            &mut level,
            &mut ticks,
            power_pos,
            registry.default_state("minecraft:air").unwrap(),
            3,
        );
        for _ in 0..4 {
            level.time += 1;
            b.tick_level_blocks(&mut level, &mut ticks);
            b.tick_block_entities(&mut level, &mut ticks);
        }
        assert_eq!(
            level.block_state(end),
            if pull {
                registry.default_state("minecraft:air").unwrap()
            } else {
                block
            },
            "sticky retraction destination"
        );
        if pull {
            assert_eq!(level.block_state(start), block);
        }
    }
}
#[test]
#[ignore = "daily"]
fn piston_push_reaction_parts_cache_reload_and_replay() {
    const NAME: &str = "game::native_hooks::piston_push_reaction_tests::piston_push_reaction_parts_cache_reload_and_replay";
    const FLAG: &str = "HORIZON_PISTON_REACTION_CASE";
    if run_fresh_cases(NAME, FLAG, &["server", "no-part"]) {
        return;
    }
    let case = std::env::var(FLAG).unwrap();
    let parts = case != "no-part";
    let enabled = Arc::new(AtomicBool::new(true));
    let unbreakable = Arc::new(AtomicU8::new(0));
    let mods: Mods = vec![ModEntry::new(
        "test",
        Box::new(User {
            enabled: enabled.clone(),
            parts,
            unbreakable: unbreakable.clone(),
        }),
    )]
    .into();
    mod_platform::testing::with_mods(mods.clone(), || {
        crate::game::api_values::install_parts(&mods).unwrap();
        let base = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let root = crate::game::mods::content_root(&base, &Default::default()).unwrap();
        with_game_role(root.prepared().to_path_buf(), |game| {
                let registry = game.pack.assets.registry;
                let a = state(registry, "test:lock", &[("locked", "false")]);
                let z = state(registry, "test:lock", &[("locked", "true")]);
                assert_eq!(
                    game.res.behaviours().piston_reaction(a),
                    if parts {
                        EngineReaction::Immoveable
                    } else {
                        EngineReaction::PushPull
                    }
                );
                exercise(
                    game.res.behaviours(),
                    registry,
                    a,
                    !parts,
                    false,
                    true,
                );
                exercise(
                    game.res.behaviours(),
                    registry,
                    z,
                    true,
                    false,
                    true,
                );
                let fixed = registry.default_state("test:fixed").unwrap();
                exercise(
                    game.res.behaviours(),
                    registry,
                    fixed,
                    parts,
                    false,
                    true,
                );
                if parts {
                    let chest = registry.default_state("minecraft:chest").unwrap();
                    exercise(
                        game.res.behaviours(),
                        registry,
                        chest,
                        false,
                        false,
                        false,
                    );
                    let bedrock = registry.default_state("minecraft:bedrock").unwrap();
                    // Every reaction loses to the hardness refusal, including popped.
                    for reaction in 0..5 {
                        unbreakable.store(reaction, Ordering::Relaxed);
                        game.resolve_parts().unwrap();
                        exercise(
                            game.res.behaviours(),
                            registry,
                            bedrock,
                            false,
                            false,
                            false,
                        );
                    }
                }
                // Removing the last subscriber must clear its immovable override on both sides.
                if parts {
                    unbreakable.store(0, Ordering::Relaxed);
                    assert_eq!(
                        game.res.behaviours().piston_reaction(a),
                        EngineReaction::Immoveable
                    );
                }
                // A reload retains registry metadata; only the active subscriber set changes.
                let after: Mods = if parts {
                    Vec::new().into()
                } else {
                    mods.clone()
                };
                mod_platform::testing::with_mods(after, || {
                    if parts {
                        game.resolve_parts().unwrap();
                        assert!(!mod_api::__private::has_scoped_hook(
                            HookId::PistonPushReaction
                        ));
                        assert_eq!(
                            game.res.behaviours().piston_reaction(a),
                            EngineReaction::PushPull
                        );
                        exercise(
                            game.res.behaviours(),
                            registry,
                            a,
                            true,
                            false,
                            true,
                        );
                    }
                    // Reinstall the actual resolved parts as reload does. None restores properties.
                    enabled.store(false, Ordering::Relaxed);
                    game.resolve_parts().unwrap();
                    assert_eq!(
                        game.res.behaviours().piston_reaction(a),
                        EngineReaction::PushPull
                    );
                    for (name, moved, popped) in [
                        ("minecraft:stone", true, false),
                        ("minecraft:obsidian", false, false),
                        ("minecraft:red_bed", false, true),
                        ("test:lock", true, false),
                        ("test:fixed", false, false),
                        ("minecraft:chest", false, false),
                    ] {
                        let block = registry.default_state(name).unwrap();
                        exercise(
                            game.res.behaviours(),
                            registry,
                            block,
                            moved,
                            popped,
                            true,
                        );
                    }
                });
        });
    });
}

struct PayloadPart {
    reaction: PushReaction,
    carry: Option<bool>,
    asks: Arc<std::sync::atomic::AtomicUsize>,
}
#[mod_api::horizon_part]
impl BlockPart for PayloadPart {
    fn piston_push_reaction(&self, _: &mut PistonPushReactionEvent) -> Option<PushReaction> {
        Some(self.reaction.clone())
    }
    fn piston_carries_block_entity(
        &self,
        _: LevelSide<'_>,
        _: &mut PistonCarriesBlockEntityEvent,
    ) -> Option<bool> {
        self.asks.fetch_add(1, Ordering::Relaxed);
        self.carry
    }
}
struct PayloadUser {
    reaction: PushReaction,
    carry: Option<bool>,
    asks: Arc<std::sync::atomic::AtomicUsize>,
}
#[mod_api::__subscriptions]
impl Hooks for PayloadUser {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for name in [
            "minecraft:chest",
            "minecraft:furnace",
            "minecraft:hopper",
            "minecraft:comparator",
        ] {
            let key = r.block_key(name);
            r.block_part(
                key,
                PayloadPart {
                    reaction: self.reaction.clone(),
                    carry: self.carry,
                    asks: self.asks.clone(),
                },
            );
        }
    }
}

#[test]
#[ignore = "daily"]
fn piston_push_reaction_real_block_entity_payload_lifecycle() {
    const NAME: &str = "game::native_hooks::piston_push_reaction_tests::piston_push_reaction_real_block_entity_payload_lifecycle";
    const FLAG: &str = "HORIZON_PISTON_PAYLOAD_CASE";
    if run_fresh_cases(
        NAME,
        FLAG,
        &[
            "push-refused",
            "push-carry",
            "push-false",
            "push-pull-refused",
            "push-pull-carry",
            "ignore-refused",
            "ignore-carry",
            "popped",
            "immovable-carry",
        ],
    ) {
        return;
    }
    let case = std::env::var(FLAG).unwrap();
    let popped = case == "popped";
    let carry = case.ends_with("-carry");
    let moves = carry && case != "immovable-carry";
    let reaction = if case == "immovable-carry" {
        PushReaction::Immoveable
    } else if popped {
        PushReaction::Popped
    } else if case.starts_with("push-pull") {
        PushReaction::PushPull
    } else if case.starts_with("ignore") {
        PushReaction::IgnoreEntity
    } else {
        PushReaction::Push
    };
    let asks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mods: Mods = vec![ModEntry::new(
        "payload",
        Box::new(PayloadUser {
            reaction,
            carry: if carry {
                Some(true)
            } else if case == "push-false" {
                Some(false)
            } else {
                None
            },
            asks: asks.clone(),
        }),
    )]
    .into();
    crate::game::api_values::install_parts(&mods).unwrap();
    mod_platform::testing::with_mods(mods, || {
        crate::game::native_entity_refs::tests::with_game(|game| {
            use horizon_client::{chunk::Chunk, containers::BlockEntity};
            use horizon_items::items::ItemStack;
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            game.block_ticks
                .add_empty_chunk(ChunkPos::new(0, 0))
                .unwrap();
            let registry = game.pack.assets.registry;
            let piston = state(registry, "minecraft:sticky_piston", &[("facing", "east")]);
            let chest = registry.default_state("minecraft:chest").unwrap();
            let power = registry.default_state("minecraft:redstone_block").unwrap();
            game.with_level(|_, level, _| {
                level.set_raw(BlockPos::new(2, 64, 2), piston);
                level.set_raw(BlockPos::new(1, 64, 2), power);
                level.set_raw(BlockPos::new(3, 64, 2), chest);
            });
            let mut slots = vec![None; 27];
            slots[4] = Some(ItemStack {
                item: "minecraft:diamond".into(),
                count: 7,
                damage: 0,
                extra: None,
            });
            game.world
                .block_entities
                .insert((3, 64, 2), BlockEntity::Chest(slots.clone()));
            assert!(
                matches!(game.world.block_entities.get(&(3,64,2)), Some(BlockEntity::Chest(before)) if before == &slots)
            );
            assert_eq!(game.world.block_entities.iter_mut().count(), 1);
            assert!(game.ecs.item_ids().is_empty());
            game.with_level(|b, level, ticks| {
                b.client_block_event(
                    level,
                    ticks,
                    BlockPos::new(2, 64, 2),
                    0,
                    Direction::East as i32,
                );
            });
            if moves {
                assert_eq!(game.world.block_entities.iter_mut().count(), 0);
                let carried = game
                    .world
                    .block_entities
                    .carried(&(4, 64, 2))
                    .expect("one carried chest");
                assert!(matches!(&carried.entity, BlockEntity::Chest(during) if during == &slots));
            }
            for _ in 0..4 {
                game.world.game_time += 1;
                game.with_level(|b, level, ticks| b.client_tick_block_entities(level, ticks));
            }
            assert!(game.block_ticks.moving_pistons.is_empty());
            assert!(game.world.block_entities.carried(&(4, 64, 2)).is_none());
            assert_eq!(
                game.world.block_entities.iter_mut().count(),
                if popped { 0 } else { 1 }
            );
            if popped {
                assert_eq!(
                    asks.load(Ordering::Relaxed),
                    0,
                    "popped never asks the carry hook"
                );
                assert_eq!(
                    game.world.block(4, 64, 2),
                    registry.default_state("minecraft:air").unwrap()
                );
                assert!(game.world.block_entities.get(&(3, 64, 2)).is_none());
            } else {
                if case == "immovable-carry" {
                    assert_eq!(asks.load(Ordering::Relaxed), 0);
                } else {
                    assert!(asks.load(Ordering::Relaxed) > 0);
                }
                let destination = if moves { (4, 64, 2) } else { (3, 64, 2) };
                assert_eq!(
                    game.world
                        .block(destination.0, destination.1, destination.2),
                    chest
                );
                let Some(BlockEntity::Chest(after)) = game.world.block_entities.get(&destination)
                else {
                    panic!("one intact chest")
                };
                assert_eq!(
                    after, &slots,
                    "all 27 slots, seven diamonds in slot 4, damage 0 and no extra fields survive"
                );
                let other = if moves { (3, 64, 2) } else { (4, 64, 2) };
                assert!(game.world.block_entities.get(&other).is_none());
            }
            if case == "push-carry" {
                let before = asks.load(Ordering::Relaxed);
                game.with_level(|b, level, ticks| {
                    level.set_raw(
                        BlockPos::new(1, 64, 2),
                        registry.default_state("minecraft:air").unwrap(),
                    );
                    b.client_block_event(
                        level,
                        ticks,
                        BlockPos::new(2, 64, 2),
                        1,
                        Direction::East as i32,
                    );
                });
                for _ in 0..4 {
                    game.world.game_time += 1;
                    game.with_level(|b, level, ticks| b.client_tick_block_entities(level, ticks));
                }
                assert_eq!(
                    asks.load(Ordering::Relaxed),
                    before,
                    "wrong-direction push only never asks carry"
                );
                assert!(
                    matches!(game.world.block_entities.get(&(4,64,2)), Some(BlockEntity::Chest(after)) if after == &slots)
                );
                assert_eq!(game.world.block_entities.iter_mut().count(), 1);
            }
            let diamonds: Vec<_> = game
                .ecs.item_views()
                .into_iter()
                .filter(|e| &*e.stack.item == "minecraft:diamond")
                .collect();
            assert_eq!(
                diamonds.iter().map(|e| e.stack.count).sum::<u32>(),
                if popped { 7 } else { 0 }
            );
            for drop in diamonds {
                assert_eq!(drop.stack.damage, 0);
                assert_eq!(drop.stack.extra, None);
            }
        });
    });
}

struct TagUser;
#[mod_api::__subscriptions]
impl Hooks for TagUser {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        r.block_part_tag(
            "test:anchored",
            ReactionPart {
                enabled: Arc::new(AtomicBool::new(true)),
                reaction: Some(PushReaction::Immoveable),
                unbreakable: None,
            },
        );
    }
}
#[test]
#[ignore = "daily"]
fn piston_push_reaction_unowned_vanilla_data_stays_refused() {
    const NAME: &str = "game::native_hooks::piston_push_reaction_tests::piston_push_reaction_unowned_vanilla_data_stays_refused";
    if run_fresh_cases(NAME, "HORIZON_PISTON_UNOWNED_CASE", &["comparator"]) {
        return;
    }
    let mods: Mods = vec![ModEntry::new(
        "payload",
        Box::new(PayloadUser {
            reaction: PushReaction::Push,
            carry: Some(true),
            asks: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }),
    )]
    .into();
    crate::game::api_values::install_parts(&mods).unwrap();
    mod_platform::testing::with_mods(mods, || {
        crate::game::native_entity_refs::tests::with_game(|game| {
            use horizon_client::chunk::Chunk;
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            game.block_ticks
                .add_empty_chunk(ChunkPos::new(0, 0))
                .unwrap();
            let registry = game.pack.assets.registry;
            let comparator = registry.default_state("minecraft:comparator").unwrap();
            let piston = state(registry, "minecraft:piston", &[("facing", "east")]);
            let power = registry.default_state("minecraft:redstone_block").unwrap();
            game.with_level(|_, level, _| {
                level.set_raw(BlockPos::new(2, 64, 2), piston);
                level.set_raw(BlockPos::new(1, 64, 2), power);
                level.set_raw(BlockPos::new(3, 64, 2), comparator);
            });
            game.block_ticks
                .comparator_outputs
                .insert(BlockPos::new(3, 64, 2), 9);
            game.with_level(|b, level, ticks| {
                b.client_block_event(
                    level,
                    ticks,
                    BlockPos::new(2, 64, 2),
                    0,
                    Direction::East as i32,
                );
            });
            assert_eq!(game.world.block(3, 64, 2), comparator);
            assert_eq!(
                game.block_ticks
                    .comparator_outputs
                    .get(&BlockPos::new(3, 64, 2)),
                Some(&9)
            );
            assert!(game.block_ticks.moving_pistons.is_empty());
        })
    });
}
