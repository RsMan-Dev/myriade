//! Primed TNT (`PrimedTnt`): 80-tick fuse, gravity and drag, explosion of
//! power 4, `TntRenderer` swell and white flash, and `TntBlock.useItemOn`
//! (flint and steel / fire charge).
use super::Game;
use horizon_client::{Result, items::ItemStack, mobs::Who, text::Text};
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
                .bool_at(horizon_client::gamerules::vanilla::TNT_EXPLODES)
    }
    /// Primes TNT a level event announced, or, when the mods refuse, puts
    /// back the block the fire or signal had already taken out.
    pub(super) fn prime_tnt_event(&mut self, pos: BlockPos, rot: f64) {
        if self.tnt_prime_allowed(pos) {
            self.add_primed_tnt(pos, rot, 80, None, None);
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
        let owner = owner.and_then(|w| self.ecs_who(w));
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
    pub(in crate::game) fn tnt_fuse_lit(&mut self, at: [f64; 3]) {
        let who = self.sculk_actor();
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
            .bool_at(horizon_client::gamerules::vanilla::TNT_EXPLODES)
        {
            return Ok(());
        }
        let bounds = horizon_client::falling_block::bounds(Vec3::new(at[0], at[1], at[2]));
        let in_water = self.bounds_in_water(bounds);
        // ExplosionInteraction.TNT: the tnt_explosion_drop_decay rule.
        let interaction = self.destroy_type(horizon_client::gamerules::vanilla::TNT_EXPLOSION_DROP_DECAY);
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
    pub(super) fn blast_owner_attack(&self) -> horizon_client::health::Attack {
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
            .bool_at(horizon_client::gamerules::vanilla::TNT_EXPLODES)
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
            let saved = horizon_client::entity_nbt::tnt_to_nbt(
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
                    .join("../../.local/horizon-client")
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
            execution_role::ExecutionRole::IntegratedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::IntegratedServer;
        let res = resources.insert(loaded.resources);
        // Keep nested fixture helpers from copying the large Game value
        // through several test-thread stack frames.
        let mut game = Box::new(
            Game::new(
                res,
                loaded.pack,
                options,
                settings,
                Audio::default(),
            )
            .unwrap(),
        );
        game.options.dedicated = Some(Vec::new());
        // Entity ticking waits for every chunk within 2 columns
        // (`ChunkMap.prepareEntityTickingChunk`).
        for z in -2..=2 {
            for x in -2..=2 {
                game.world.chunks.insert(
                    (x, z),
                    std::sync::Arc::new(horizon_client::chunk::Chunk::empty(x, z)),
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

}

#[cfg(test)]
mod entity_collision_tests {
    use horizon_client::chunk::Chunk;
    use horizon_core::position::ChunkPos;
    use std::sync::Arc;

}
