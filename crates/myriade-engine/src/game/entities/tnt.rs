//! Primed TNT (`PrimedTnt`): 80-tick fuse, gravity and drag, explosion of
//! power 4, `TntRenderer` swell and white flash, and `TntBlock.useItemOn`
//! (flint and steel / fire charge).
use super::Game;
use myriade_engine::{Result, items::ItemStack, mobs::Who, text::Text};
use horizon_core::{
    geometry::{Aabb, Vec3},
    position::BlockPos,
    random::RandomSource,
};
use horizon_world::level::flags as level_flags;

pub(super) use horizon_gameplay::tnt::PrimedTnt;

impl Game<'_> {
    /// Asks the mods whether the TNT at `pos` may be primed
    /// (`native_tnt_primed`); the acting guest, if any, is its player.
    pub(super) fn tnt_prime_allowed(&mut self, pos: BlockPos) -> bool {
        let mut cancel = false;
        self.native_tnt_primed(self.lan_acting_uuid, [pos.x, pos.y, pos.z], &mut cancel);
        // `TntBlock.prime` leaves the block alone with `tnt_explodes` off, for
        // redstone, fire and an unstable block breaking alike.
        !cancel
            && self
                .session
                .rules
                .bool_at(myriade_engine::gamerules::vanilla::TNT_EXPLODES)
    }
    /// Primes TNT a level event announced, or, when the mods refuse, puts
    /// back the block the fire or signal had already taken out.
    pub(super) fn prime_tnt_event(&mut self, pos: BlockPos, rot: f64) {
        if self.tnt_prime_allowed(pos) {
            // `TntBlock.prime(level, pos, igniter)`: whoever acts at this moment lit it and answers for its blast.
            let igniter = self.sculk_actor();
            self.add_primed_tnt(pos, rot, 80, igniter, None);
            return;
        }
        // Refused: the level queued the fuse sound and the game event right
        // after the prime; they are dropped as they come up.
        self.tnt_refused = Some(pos);
        let Some(tnt) = self.pack.assets.registry.default_state("minecraft:tnt") else {
            return;
        };
        if self.world.block(pos.x, pos.y, pos.z) == self.world.air {
            let flags = level_flags::UPDATE_CLIENTS | level_flags::UPDATE_SKIP_ON_PLACE;
            self.with_level(|b, level, ticks| b.set_block(level, ticks, pos, tnt, flags));
        }
    }
    /// `PrimedTnt` at the block's bottom centre, lit in the ECS (`prime_tnt_ecs`); `short` draws `wasExploded`'s random short fuse.
    pub(super) fn spawn_primed_tnt(&mut self, pos: BlockPos, short: bool, owner: Option<Who>, _guest: Option<u32>) {
        let fuse = if short {
            // PrimedTnt.getRandomShortFuse
            self.world.random.next_int_bound(20).unwrap_or(0) + 10
        } else {
            80
        };
        self.prime_tnt_ecs(pos, fuse, owner);
    }
    /// Lights a TNT at the block's bottom centre, `fuse` ticks long (the ECS pushes it; `rot` and `guest` no longer shape it).
    pub(super) fn add_primed_tnt(&mut self, pos: BlockPos, _rot: f64, fuse: i32, owner: Option<Who>, _guest: Option<u32>) {
        self.prime_tnt_ecs(pos, fuse, owner);
    }
    /// `EcsLevel::prime_tnt` at the block's bottom centre, credited to `owner`; the network id, 0 when the level is inert.
    pub(super) fn prime_tnt_ecs(&mut self, pos: BlockPos, fuse: i32, owner: Option<Who>) -> u32 {
        let at = [f64::from(pos.x) + 0.5, f64::from(pos.y), f64::from(pos.z) + 0.5];
        // A player with no entity of its own yet (a headless level) still answers for its TNT: the id 0 stands for it.
        let owner = owner.and_then(|w| self.ecs_who(w).or_else(|| matches!(w, Who::Player).then(|| horizon_ecs_proto::targeting::Who::Player(u32::try_from(self.own_entity_id()).unwrap_or(0)))));
        let mut next = self.next_entity;
        let id = self.ecs_with(|ecs, mw| {
            ecs.prime_tnt(mw, at, fuse, owner, 4.0, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        id
    }
    /// `TntEvent::Fuse`: the hiss and the `prime_fuse` game event of a TNT the ECS lit at `at`.
    pub(in crate::game) fn tnt_fuse_lit(&mut self, at: [f64; 3], owner: Option<Who>) {
        let who = owner.or_else(|| self.sculk_actor());
        let block = [at[0].floor() as i32, at[1].floor() as i32, at[2].floor() as i32];
        self.post_block_game_event("minecraft:prime_fuse", block, who, None);
        self.audio.play("minecraft:entity.tnt.primed", Some(at), 1.0, 1.0);
    }
    /// `TntEvent::Landed`: the fall of a TNT on the block (`Entity.checkFallDamage`), then its landing event.
    pub(in crate::game) fn tnt_landed(&mut self, id: u32, at: Vec3, bounds: Aabb, fall: f64) {
        self.non_living_fall_on(id, at, bounds, fall);
        self.post_landing_event(at, bounds, false);
    }
    /// `TntEvent::Explode`: the blast of a TNT whose fuse ended at `at` (`PrimedTnt.explode`), credited to `owner`.
    /// With `tnt_explodes` off there is no blast.
    pub(in crate::game) fn tnt_blast(&mut self, at: [f64; 3], power: f32, owner: Option<Who>, used_portal: bool) -> Result<()> {
        if !self
            .session
            .rules
            .bool_at(myriade_engine::gamerules::vanilla::TNT_EXPLODES)
        {
            return Ok(());
        }
        let bounds = myriade_engine::falling_block::bounds(Vec3::new(at[0], at[1], at[2]));
        let in_water = self.bounds_in_water(bounds);
        // ExplosionInteraction.TNT: the tnt_explosion_drop_decay rule.
        let interaction = self.destroy_type(myriade_engine::gamerules::vanilla::TNT_EXPLOSION_DROP_DECAY);
        self.combat.explosion_in_water = in_water;
        self.combat.explosion_direct = None;
        self.set_blast_owner(owner, None);
        let result = self.with_hive_emptying_blast(|g| {
            g.server_explode_from(at, power, interaction, false, super::explosion::ExplosionSource::default().with_portal_proof(used_portal))
        });
        self.combat.blast_owner = None;
        self.combat.blast_owner_player = false;
        self.combat.blast_guest = None;
        self.combat.explosion_in_water = false;
        result
    }
    /// `Explosion.getIndirectSourceEntity`: a blast whose cause is `owner`
    /// (a TNT's igniter, a player's projectile, a player) names it, which
    /// `blast_attacker`, the damage type and kill credit read. A player is the
    /// one on `guest`, the player in place without one.
    pub(super) fn set_blast_owner(&mut self, owner: Option<Who>, guest: Option<u32>) {
        self.combat.blast_guest = guest;
        self.combat.blast_owner = tnt_blast_owner(
            owner,
            || match guest {
                Some(_) => self.player_attacker(guest),
                None => self
                    .player
                    .as_ref()
                    .map(|p| (p.entity_id as u32, self.player_display_name())),
            },
            |id| self.mob_display_name(id),
        );
        self.combat.blast_owner_player =
            owner == Some(Who::Player) && self.combat.blast_owner.is_some();
    }
    /// The attack a player-owned blast's hits land in: its causing player's,
    /// so a mob it kills is that player's kill.
    pub(super) fn blast_owner_attack(&self) -> myriade_engine::health::Attack {
        let id = self
            .combat
            .blast_owner
            .as_ref()
            .map_or(0, |(id, _)| *id as i32);
        let mut attack = self.attack_of(id);
        attack.player_uuid = self.player_uuid_of(self.combat.blast_guest);
        attack
    }
    /// `TntBlock.useItemOn`: flint and steel or a fire charge primes TNT.
    pub(super) fn ignite_tnt(&mut self, stack: &ItemStack) -> Result<bool> {
        let item = &*stack.item;
        if item != "minecraft:flint_and_steel" && item != "minecraft:fire_charge" {
            return Ok(false);
        }
        let Some(hit) = self.pick() else {
            return Ok(false);
        };
        let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        if &*self.res.facts().get(state).name != "minecraft:tnt" {
            return Ok(false);
        }
        let pos = BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]);
        if !self
            .session
            .rules
            .bool_at(myriade_engine::gamerules::vanilla::TNT_EXPLODES)
        {
            // TntBlock.useItemOn: the rule is off, the acting player's overlay
            // says so, and PASS lets the item's own useOn light a fire there.
            let message = Text::translate("block.minecraft.tnt.disabled", &[]);
            self.send_overlay_message(message);
            return Ok(false);
        }
        // `TntBlock.prime`: an adventure player lights TNT only with a stack
        // whose `can_break` matches it. The refused click is still SUCCESS: the
        // arm swings, and neither the block, the stack nor the fuse changes.
        if !self.may_build() && !self.can_break_in_adventure(hit.pos) {
            self.swing_arm();
            return Ok(true);
        }
        if !self.tnt_prime_allowed(pos) {
            return Ok(true);
        }
        let guest = self.lan_acting;
        self.spawn_primed_tnt(pos, false, Some(Who::Player), guest);
        let air = self.world.air;
        self.with_level(|b, level, ticks| {
            b.set_block(level, ticks, pos, air, level_flags::UPDATE_ALL_IMMEDIATE)
        });
        self.process_level_events()?;
        self.swing_arm();
        if item == "minecraft:flint_and_steel" {
            self.damage_held(1);
        } else {
            self.consume_held();
        }
        self.award_stat("used", item, 1);
        Ok(true)
    }
}

/// `PrimedTnt.getOwner` as the blast's causing entity: the player, or a mob
/// that is still in the level (null once it is gone).
fn tnt_blast_owner(
    owner: Option<Who>,
    player: impl FnOnce() -> Option<(u32, Text)>,
    mob_name: impl Fn(u32) -> Option<Text>,
) -> Option<(u32, Text)> {
    match owner? {
        Who::Player => player(),
        Who::Mob(id) => mob_name(id).map(|name| (id, name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_block_state_follows_the_model_instead_of_raw_input() {
        use {
            horizon_core::nbt::Tag,
            horizon_world::registry::{BlockRegistry, BlockStateId},
        };
        let unstable = Tag::compound([
            ("id", Tag::String("minecraft:tnt".into())),
            (
                "properties",
                Tag::compound([("unstable", Tag::String("true".into()))]),
            ),
        ]);
        let registry = BlockRegistry::from_nbt(&Tag::compound([
            ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
            (
                "states",
                Tag::List(vec![
                    Tag::String("minecraft:tnt".into()),
                    Tag::String("minecraft:stone".into()),
                    unstable.clone(),
                ]),
            ),
            (
                "defaults",
                Tag::compound([
                    ("minecraft:tnt", Tag::Int(0)),
                    ("minecraft:stone", Tag::Int(1)),
                ]),
            ),
        ]))
        .unwrap();
        let mut tnt = PrimedTnt {
            attachment_lifetime: Default::default(),
            rotation: [0.0, 0.0],
            join: Default::default(),
            id: 1,
            position: Vec3::ZERO,
            previous: Vec3::ZERO,
            velocity: Vec3::ZERO,
            fuse: 400,
            on_ground: false,
            fall_distance: 0.0,
            stuck: Vec3::ZERO,
            in_water: false,
            owner: None,
            guest: None,
            no_gravity: true,
            power: 4.0,
            used_portal: false,
            block_state: Some(BlockStateId(1)),
        };
        let raw = Tag::compound([("block_state", Tag::String("audit_missing:block".into()))]);
        for (state, expected) in [
            (Some(BlockStateId(1)), Tag::String("minecraft:stone".into())),
            (Some(BlockStateId(2)), unstable),
            (None, Tag::String("minecraft:tnt".into())),
        ] {
            tnt.block_state = state;
            let saved = myriade_engine::entity_nbt::tnt_to_nbt(
                tnt.position,
                tnt.velocity,
                tnt.on_ground,
                tnt.fall_distance,
                tnt.fuse,
                tnt.power,
                tnt.block_state_nbt(&registry),
                [0.0, 0.0],
                [221, 0, 0, 1],
                None,
                Some(&raw),
            );
            assert_eq!(saved.get("block_state"), Some(&expected));
        }
    }

    #[test]
    fn a_tnt_lit_by_a_skeletons_arrow_names_the_skeleton_as_its_cause() {
        let name = || Text::translate("entity.minecraft.skeleton", &[]);
        let by_mob = tnt_blast_owner(Some(Who::Mob(7)), || None, |id| (id == 7).then(name));
        assert_eq!(by_mob, Some((7, name())));
        // The skeleton died before the blast: `getOwner` is null.
        assert_eq!(tnt_blast_owner(Some(Who::Mob(7)), || None, |_| None), None);
        let player = Text::literal("Steve");
        let by_player = tnt_blast_owner(Some(Who::Player), || Some((1, player.clone())), |_| None);
        assert_eq!(by_player, Some((1, player)));
        assert_eq!(tnt_blast_owner(None, || None, |_| None), None);
    }

    #[test]
    fn a_piston_slime_bounce_launches_primed_tnt() {
        use horizon_core::geometry::Direction;
        use horizon_world::piston_push::bounce_velocity;
        let mut tnt = PrimedTnt {
            attachment_lifetime: Default::default(),
            rotation: [0.0, 0.0],
            join: Default::default(),
            no_gravity: false,
            id: 0,
            position: Vec3::new(0.5, 1.0, 0.5),
            previous: Vec3::new(0.5, 1.0, 0.5),
            velocity: Vec3::ZERO,
            fuse: 80,
            on_ground: true,
            fall_distance: 0.0,
            stuck: Vec3::ZERO,
            in_water: false,
            owner: None,
            guest: None,
            power: 4.0,
            used_portal: false,
            block_state: None,
        };
        tnt.velocity = bounce_velocity(Direction::Up, tnt.velocity);
        tnt.on_ground = false;
        assert_eq!(tnt.velocity.y, 1.0);
        assert!(!tnt.on_ground);
    }

    /// A server's game on the prepared resources with the chunk at the origin
    /// loaded, as `native_machines`' tests make one; None without them.
    fn server_game<'a>(
        resources: &'a mut Option<std::sync::Arc<crate::game::Resources>>,
    ) -> Option<Box<Game<'a>>> {
        use crate::game::*;
        let root = std::env::var_os("HORIZON_TEST_RESOURCES")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../.local/myriade-engine")
                    .join(horizon_core::VANILLA_VERSION)
            });
        if !root.join("movement-properties.nbt").exists() {
            return None;
        }
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            execution_role::ExecutionRole::DedicatedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let res = resources.insert(loaded.resources);
        // Keep nested fixture helpers from copying the large Game value
        // through several test-thread stack frames.
        let mut game = Box::new(
            Game::new(
                res,
                loaded.pack,
                options,
                settings,
            )
            .unwrap(),
        );        // Entity ticking waits for every chunk within 2 columns
        // (`ChunkMap.prepareEntityTickingChunk`).
        for z in -2..=2 {
            for x in -2..=2 {
                game.world.chunks.insert(
                    (x, z),
                    std::sync::Arc::new(myriade_engine::chunk::Chunk::empty(x, z)),
                );
            }
        }
        game.streaming.forced.insert((0, 0));
        Some(game)
    }

    /// Optional.empty resistance means the rays behave exactly as through air,
    /// not like a zero-resistance solid (which still costs 0.09 per step).
    #[test]
    #[ignore = "daily"]
    fn portal_blast_rays_pass_through_without_resistance() {
        use super::super::explosion::{BlockInteraction, ExplosionSource};
        let mut resources = None;
        let mut game = server_game(&mut resources).expect("set HORIZON_TEST_RESOURCES");
        let registry = game.pack.assets.registry;
        let portal = registry.default_state("minecraft:nether_portal").unwrap();
        let glass = registry.default_state("minecraft:glass").unwrap();
        let air = game.world.air;
        for seed in [0, 1, 19, 314159] {
            let mut outcomes = Vec::new();
            for center in [air, portal] {
                for x in -4i32..=4 {
                    for y in -4i32..=4 {
                        for z in -4i32..=4 {
                            let distance = x.abs().max(y.abs()).max(z.abs());
                            let state = if distance <= 1 {
                                center
                            } else if distance == 3 {
                                glass
                            } else {
                                air
                            };
                            game.world.set(8 + x, 64 + y, 8 + z, state).unwrap();
                        }
                    }
                }
                game.world.random.set_seed(seed);
                game.server_explode_from(
                    [8.5, 64.5, 8.5],
                    2.0,
                    BlockInteraction::Destroy,
                    false,
                    ExplosionSource::default().with_portal_proof(true),
                )
                .unwrap();
                let mut remaining = Vec::new();
                for x in -3i32..=3 {
                    for y in -3i32..=3 {
                        for z in -3i32..=3 {
                            if x.abs().max(y.abs()).max(z.abs()) == 3 {
                                remaining.push(game.world.block(8 + x, 64 + y, 8 + z) == glass);
                            }
                        }
                    }
                }
                outcomes.push(remaining);
            }
            assert!(
                outcomes[0].iter().any(|&solid| !solid),
                "blast reaches the shell"
            );
            assert!(
                outcomes[0].iter().any(|&solid| solid),
                "shell also extends beyond the blast"
            );
            assert_eq!(
                outcomes[0], outcomes[1],
                "seed {seed}: portal must not attenuate rays"
            );
        }
    }

    /// The 7-block cube around (8, 64, 8) of the blast tests: air inside, a shell
    /// at distance 3 of stone on the +x half and glass on the other.
    fn fill_shell(game: &mut crate::game::Game<'_>) {
        let registry = game.pack.assets.registry;
        let stone = registry.default_state("minecraft:stone").unwrap();
        let glass = registry.default_state("minecraft:glass").unwrap();
        let air = game.world.air;
        for x in -3i32..=3 {
            for y in -3i32..=3 {
                for z in -3i32..=3 {
                    let state = match x.abs().max(y.abs()).max(z.abs()) {
                        3 if x >= 0 => stone,
                        3 => glass,
                        _ => air,
                    };
                    game.world.set(8 + x, 64 + y, 8 + z, state).unwrap();
                }
            }
        }
    }

    /// The shell after `fill_shell` and a blast: per cell, 0 air, 1 stone, 2 glass.
    fn shell_states(game: &crate::game::Game<'_>) -> Vec<u8> {
        let registry = game.pack.assets.registry;
        let stone = registry.default_state("minecraft:stone").unwrap();
        let air = game.world.air;
        let mut cells = Vec::new();
        for x in -3i32..=3 {
            for y in -3i32..=3 {
                for z in -3i32..=3 {
                    if x.abs().max(y.abs()).max(z.abs()) == 3 {
                        let state = game.world.block(8 + x, 64 + y, 8 + z);
                        cells.push(if state == air { 0 } else if state == stone { 1 } else { 2 });
                    }
                }
            }
        }
        cells
    }

    /// `USED_PORTAL_DAMAGE_CALCULATOR` only changes the portal: stone and glass
    /// keep the normal calculator's resistance and breaking (oracle: other blocks).
    #[test]
    #[ignore = "daily"]
    fn portal_blast_other_blocks_keep_the_normal_calculator() {
        use super::super::explosion::{BlockInteraction, ExplosionSource};
        let mut resources = None;
        let mut game = server_game(&mut resources).expect("set HORIZON_TEST_RESOURCES");
        for seed in [0, 1, 19, 314159] {
            let mut outcomes = Vec::new();
            for used_portal in [false, true] {
                fill_shell(&mut game);
                game.world.random.set_seed(seed);
                game.server_explode_from(
                    [8.5, 64.5, 8.5],
                    2.0,
                    BlockInteraction::Destroy,
                    false,
                    ExplosionSource::default().with_portal_proof(used_portal),
                )
                .unwrap();
                outcomes.push(shell_states(&game));
            }
            assert!(outcomes[0].contains(&0), "seed {seed}: the blast reaches the shell");
            assert_eq!(
                outcomes[0], outcomes[1],
                "seed {seed}: the used-portal calculator must not change other blocks"
            );
        }
    }

    /// `USED_PORTAL_DAMAGE_CALCULATOR` spares a nether portal (oracle: portal
    /// block, no resistance and no breaking); the normal calculator breaks it.
    #[test]
    #[ignore = "daily"]
    fn portal_blast_portal_survives_only_the_used_calculator() {
        use super::super::explosion::{BlockInteraction, ExplosionSource};
        let mut resources = None;
        let mut game = server_game(&mut resources).expect("set HORIZON_TEST_RESOURCES");
        let registry = game.pack.assets.registry;
        let portal = registry.default_state("minecraft:nether_portal").unwrap();
        let air = game.world.air;
        for (used_portal, survives) in [(false, false), (true, true)] {
            for x in -1i32..=1 {
                for y in -1i32..=1 {
                    for z in -1i32..=1 {
                        game.world.set(8 + x, 64 + y, 8 + z, air).unwrap();
                    }
                }
            }
            game.world.set(8, 64, 9, portal).unwrap();
            game.server_explode_from(
                [8.5, 64.5, 8.5],
                2.0,
                BlockInteraction::Destroy,
                false,
                ExplosionSource::default().with_portal_proof(used_portal),
            )
            .unwrap();
            assert_eq!(
                game.world.block(8, 64, 9) == portal,
                survives,
                "used_portal {used_portal}"
            );
        }
    }

    /// A sulfur cube's blast (`MobEvent::Explode`) spares the portal only when
    /// the cube came through one (`PortalCooldown` > 0, `portal_proof`).
    #[test]
    #[ignore = "daily"]
    fn portal_blast_sulfur_cube_spares_a_portal_only_after_its_trip() {
        use crate::game::ecs::test_support::place_test_mob;
        use myriade_engine::mobs::{MobEvent, Species};
        use horizon_core::nbt::Tag;
        let mut resources = None;
        let mut game = server_game(&mut resources).expect("set HORIZON_TEST_RESOURCES");
        let registry = game.pack.assets.registry;
        let portal = registry.default_state("minecraft:nether_portal").unwrap();
        let air = game.world.air;
        for (n, (cooldown, survives)) in [(300, true), (0, false)].into_iter().enumerate() {
            for x in -1i32..=1 {
                for y in -1i32..=1 {
                    for z in -1i32..=1 {
                        game.world.set(8 + x, 64 + y, 8 + z, air).unwrap();
                    }
                }
            }
            game.world.set(8, 64, 9, portal).unwrap();
            let cube = place_test_mob(&mut game, 50 + n as u32, "minecraft:sulfur_cube", [8.5, 64.0, 8.5])
                .expect("the sulfur cube spawns");
            game.restore_portal_cooldown_of(cube, Some(&Tag::Int(cooldown)));
            game.handle_mob_events_legacy(MobEvent::Explode(
                [8.5, 64.0625, 8.5],
                4.0,
                Species::SulfurCube,
                false,
            ))
            .unwrap();
            assert_eq!(
                game.world.block(8, 64, 9) == portal,
                survives,
                "cooldown {cooldown}"
            );
        }
    }

    /// The sulfur cube's blast after a portal trip changes no other block
    /// (`VanillaSulfurPortalBlastOracle`: the other blocks, as the normal calculator).
    #[test]
    #[ignore = "daily"]
    fn portal_blast_sulfur_cube_other_blocks_keep_the_normal_calculator() {
        use crate::game::ecs::test_support::place_test_mob;
        use myriade_engine::mobs::{MobEvent, Species};
        use horizon_core::nbt::Tag;
        let mut resources = None;
        let mut game = server_game(&mut resources).expect("set HORIZON_TEST_RESOURCES");
        let mut outcomes = Vec::new();
        for (n, cooldown) in [300, 0].into_iter().enumerate() {
            fill_shell(&mut game);
            let cube = place_test_mob(&mut game, 60 + n as u32, "minecraft:sulfur_cube", [8.5, 64.0, 8.5])
                .expect("the sulfur cube spawns");
            game.restore_portal_cooldown_of(cube, Some(&Tag::Int(cooldown)));
            game.handle_mob_events_legacy(MobEvent::Explode(
                [8.5, 64.0625, 8.5],
                4.0,
                Species::SulfurCube,
                false,
            ))
            .unwrap();
            outcomes.push(shell_states(&game));
        }
        assert!(outcomes[1].contains(&0), "the blast reaches the shell");
        assert_eq!(outcomes[0], outcomes[1], "the portal trip must not change other blocks");
    }

}

#[cfg(test)]
mod entity_collision_tests {
    use myriade_engine::chunk::Chunk;
    use horizon_core::position::ChunkPos;
    use std::sync::Arc;

}
