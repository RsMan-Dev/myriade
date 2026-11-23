//! The End: End portal frames filled with eyes of ender, End portal and End
//! gateway travel, the obsidian arrival platform, the exit portal and the End
//! sky. The dragon fight itself lives in game/mobs/dragon.rs: the podium is
//! inactive until the dragon dies.
use super::{Dim, Game};
use myriade_engine::{
    Result,
    containers::BlockEntity,
    items::ItemStack,
    mobs::Who,
    projectiles::{Projectile, ProjectileKind},
    vanillagen::VanillaTerrain,
    world_spawn::PlayerSpawnFinder,
};
use horizon_core::{
    geometry::Vec3,
    position::BlockPos,
    random::{LegacyRandom, RandomSource},
};

use std::{collections::HashMap, sync::Arc};

/// `TicketType.PORTAL`'s timeout, in ticks.
const PORTAL_TICKET: u32 = 300;

/// ServerLevel.END_SPAWN_POINT.
pub(super) const END_SPAWN: [i32; 3] = [100, 50, 0];
/// EnderDragonFight gateway ring radius and height.
const GATEWAY_RADIUS: f64 = 96.0;
const GATEWAY_Y: i32 = 75;

/// A gateway teleport waiting for its destination chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GatewayJob {
    /// TheEndGatewayBlockEntity.findOrCreateValidTeleportPos from this gateway.
    /// `exact` is the gateway's own `ExactTeleport`, read while its chunk is loaded.
    Find { gateway: [i32; 3], exact: bool },
    /// A known exit (exact, or findExitPosition around it).
    Known { exit: [i32; 3], exact: bool },
}

/// An entity travelling through an End gateway: the entity and the job that
/// finds where it lands (`TheEndGatewayBlockEntity.getPortalPosition`).
#[derive(Clone, Debug)]
struct GatewayArrival {
    /// The entity, as it waits for the exit's chunks.
    traveller: Arriving,
    /// The gateway's exit, or the search for one.
    job: GatewayJob,
    /// The spot to load before the job is resolved.
    target: Vec3,
}

/// What waits at a gateway's exit.
#[derive(Clone, Debug)]
enum Arriving {
    /// A thrown ender pearl (projectiles are not saved).
    Pearl(Box<Projectile>),
    /// An ordinary entity remains live in this level while its destination loads.
    Live(u32),
}

impl GatewayJob {
    /// The source of a new exit must stay loaded until its block entity is updated.
    pub(super) fn source_chunk(self) -> Option<(i32, i32)> {
        match self {
            Self::Find { gateway, .. } => Some((gateway[0] >> 4, gateway[2] >> 4)),
            Self::Known { .. } => None,
        }
    }

    /// TheEndGatewayBlockEntity.getPortalPosition, using the gateway block entity.
    fn for_gateway(gateway: [i32; 3], entity: Option<&BlockEntity>) -> Self {
        let configured = match entity {
            Some(BlockEntity::EndGateway(g)) => g.exit_portal.map(|exit| (exit, g.exact_teleport)),
            _ => None,
        };
        match configured {
            Some((exit, exact)) => Self::Known { exit, exact },
            // Worldgen return gateways (end_gateway_return) lead exactly to the End spawn.
            None if f64::from(gateway[0]).hypot(f64::from(gateway[2])) > 512.0 => Self::Known {
                exit: END_SPAWN,
                exact: true,
            },
            None => Self::Find {
                gateway,
                exact: exact_teleport(entity),
            },
        }
    }
}

/// `TheEndGatewayBlockEntity.exactTeleport` of the block entity at a gateway
/// (false without one).
fn exact_teleport(entity: Option<&BlockEntity>) -> bool {
    matches!(entity, Some(BlockEntity::EndGateway(g)) if g.exact_teleport)
}

/// The end of `getPortalPosition`: the exit itself when the teleport is exact,
/// else the spot [`Game::resolve_gateway`] finds around it (`findExitPosition`).
fn exit_target(exit: [i32; 3], exact: bool, find: impl FnOnce([i32; 3]) -> [i32; 3]) -> [i32; 3] {
    if exact { exit } else { find(exit) }
}

/// `findOrCreateValidTeleportPos`'s `findTallestBlock(level, pos, 16, true)`.
const TALLEST_REACH: i32 = 16;
/// `findExitPosition`'s `findTallestBlock(level, exitPortal.above(2), 5, false)`.
const FIND_EXIT_REACH: i32 = 5;

/// The chunks holding a block within `reach` of `around` in x and z.
fn chunks_around(around: [i32; 3], reach: i32) -> impl Iterator<Item = (i32, i32)> {
    let span = move |v: i32| ((v - reach) >> 4)..=((v + reach) >> 4);
    span(around[2]).flat_map(move |cz| span(around[0]).map(move |cx| (cx, cz)))
}

fn fallback_spawn(own: Option<Vec3>, world: Option<[i32; 3]>) -> Vec3 {
    let s = own.unwrap_or_else(|| {
        let [x, y, z] = world.unwrap_or([0, 64, 0]);
        Vec3::new(f64::from(x), f64::from(y), f64::from(z))
    });
    Vec3::new(s.x.floor() + 0.5, s.y, s.z.floor() + 0.5)
}

#[derive(Default)]
pub(super) struct EndState {
    /// Ticks until the player can use a gateway again (TheEndGatewayBlockEntity cooldown).
    gateway_cooldown: u32,
    gateways: HashMap<u32, myriade_engine::end_gateway::Traveller>,
    /// The entities that went through a gateway, waiting for its exit.
    arrivals: Vec<GatewayArrival>,
    /// The chunks kept loaded for those arrivals.
    areas: Vec<(i32, i32)>,
    /// Every candidate chunk an exit search has tested
    /// (`findExitPortalXZPosTentative`), by the gateway whose exit it looks
    /// for: kept loaded with the areas until no traveller waits for that
    /// gateway, so that a retry never finds an earlier candidate unloaded.
    search: Vec<([i32; 3], Vec<(i32, i32)>)>,
    /// `Entity.placePortalTicket` of the pearls that came out of a gateway:
    /// the chunk each landed in, kept loaded for the ticks left of its
    /// `TicketType.PORTAL`, so that it can come down and take its owner there.
    tickets: Vec<((i32, i32), u32)>,
    /// The Overworld generator, for stronghold positions (ServerLevel.findNearestMapStructure).
    locator: std::sync::Mutex<Option<Arc<VanillaTerrain>>>,
    /// Thrown eyes of ender in flight.
    eyes: Vec<EyeOfEnder>,
}

impl EndState {
    pub(super) fn locator(&self) -> Option<Arc<VanillaTerrain>> {
        self.locator.lock().unwrap().clone()
    }
    pub(super) fn set_locator(&self, terrain: Option<Arc<VanillaTerrain>>) {
        *self.locator.lock().unwrap() = terrain;
    }
}

impl EndState {
    /// The eyes' entity ids, for a server to number them.
    pub(super) fn eye_ids(&mut self) -> impl Iterator<Item = &mut u32> {
        self.eyes.iter_mut().map(|e| &mut e.id)
    }
    /// The numbered eyes as a server's clients track them: (id, position, motion).
    pub(super) fn eyes_tracked(&self) -> impl Iterator<Item = (u32, [f64; 3], [f64; 3])> {
        self.eyes
            .iter()
            .filter(|e| e.id != 0)
            .map(|e| (e.id, e.position.to_array(), e.velocity.to_array()))
    }
}

/// An eye of ender from `/summon` or a saved chunk. `signalTo` never ran for
/// it, so it has no target and `surviveAfterDeath` stays false.
#[derive(Clone, Copy, Debug)]
pub(in crate::game) struct SummonedEye {
    /// `getDeltaMovement`, which nothing changes without a target.
    pub(super) motion: Vec3,
    /// `EyeOfEnder.life`: not saved, so a loaded eye starts again at 0.
    life: u32,
}
impl SummonedEye {
    /// `Entity.load`'s motion and `EyeOfEnder.readAdditionalSaveData`'s
    /// `setItem`, which keeps one of the saved item, or an eye of ender.
    pub(super) fn load(tag: &mut horizon_core::nbt::Tag) -> Self {
        use horizon_core::nbt::Tag;
        let motion = myriade_engine::entity_nbt::loaded_motion(tag);
        if let Tag::Compound(c) = tag {
            let mut item = match c.remove(&"Item".into()) {
                Some(Tag::Compound(item)) => item,
                _ => Default::default(),
            };
            let id = match item.get(&"id".into()) {
                Some(Tag::String(s)) => {
                    myriade_data::identifier::normalize_id(&s.to_string_lossy())
                }
                _ => None,
            }
            .filter(|id| id != "minecraft:air");
            if id.is_none() {
                item.clear();
            }
            let id = id.unwrap_or_else(|| "minecraft:ender_eye".into());
            item.insert("id".into(), Tag::String(id.as_str().into()));
            item.insert("count".into(), Tag::Int(1));
            c.insert("Item".into(), Tag::Compound(item));
        }
        Self { motion, life: 0 }
    }
}

impl Game<'_> {
}

/// The EyeOfEnder entity.
struct EyeOfEnder {
    /// Its entity id once a server's clients track it (0 until then).
    id: u32,
    position: glam::DVec3,
    previous: glam::DVec3,
    velocity: glam::DVec3,
    target: glam::DVec3,
    life: u32,
    survive: bool,
}
impl EyeOfEnder {
    /// EyeOfEnder.updateDeltaMovement.
    fn update_delta(old: glam::DVec3, position: glam::DVec3, target: glam::DVec3) -> glam::DVec3 {
        let horizontal = glam::DVec3::new(target.x - position.x, 0.0, target.z - position.z);
        let length = horizontal.length();
        let old_horizontal = old.x.hypot(old.z);
        let mut wanted_speed = old_horizontal + 0.0025 * (length - old_horizontal);
        let mut y = old.y;
        if length < 1.0 {
            wanted_speed *= 0.8;
            y *= 0.8;
        }
        let wanted_y = if position.y - old.y < target.y {
            1.0
        } else {
            -1.0
        };
        horizontal * (wanted_speed / length)
            + glam::DVec3::new(0.0, y + (wanted_y - y) * 0.015, 0.0)
    }
}

impl Game<'_> {
    pub(super) fn block_name(&self, p: [i32; 3]) -> std::sync::Arc<str> {
        self.res
            .facts()
            .get(self.world.block(p[0], p[1], p[2]))
            .name
            .clone()
    }

    /// EnderEyeItem.useOn: fill an empty End portal frame, then light the
    /// portal when the ring of twelve filled frames is complete.
    pub(super) fn use_ender_eye(&mut self, stack: &ItemStack, use_on_block: bool) -> Result<bool> {
        if &*stack.item != "minecraft:ender_eye" {
            return Ok(false);
        }
        let registry = self.pack.assets.registry;
        let target = self.pick().and_then(|hit| {
            let state = registry.state(self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]))?;
            (state.name == "minecraft:end_portal_frame").then(|| (hit, state.clone()))
        });
        let Some((hit, state)) = target else {
            // EnderEyeItem.useOn passes off a frame: a guest's UseItem throws it.
            if matches!(self.guest_use, Some(super::GuestUse::OnBlock(_))) {
                return Ok(false);
            }
            // EnderEyeItem.use: startUsingItem (getUseDuration 0, so the use
            // lasts until the button lets go), then the eye is thrown towards
            // the nearest stronghold if there is one (CONSUME without).
            self.start_using_item();
            self.throw_ender_eye();
            return Ok(true);
        };
        if !use_on_block {
            // EnderEyeItem.use passes on a frame; only useOn can insert an eye.
            return Ok(false);
        }
        if state.properties.get("eye").map(String::as_str) == Some("true") {
            // Both useOn and use pass on a filled frame.
            return Ok(true);
        }
        let mut filled = state;
        filled.properties.insert("eye".into(), "true".into());
        let id = registry.resolve(&filled)?;
        self.level_set(hit.pos, id)?;
        // Block.pushEntitiesUp: the eye raises the frame's top from 13/16 to a full block.
        if let Some(p) = self.player.as_mut() {
            let top = f64::from(hit.pos[1]) + 1.0;
            let w = f64::from(p.dimensions.width) / 2.0;
            let over = |c: f64, b: i32| c + w > f64::from(b) && c - w < f64::from(b) + 1.0;
            if over(p.position.x, hit.pos[0])
                && over(p.position.z, hit.pos[2])
                && p.position.y >= top - 0.25
                && p.position.y < top
            {
                p.position.y = top;
            }
        }
        self.award_stat("used", "minecraft:ender_eye", 1);
        self.consume_held();
        self.swing_arm();
        // EnderEyeItem.useOn emits this event for all nearby players.
        self.level_event_at(1503, hit.pos, 0);
        if let Some(c) = self.portal_ring_center(hit.pos) {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let p = [c[0] + dx, c[1], c[2] + dz];
                    let at = BlockPos::new(p[0], p[1], p[2]);
                    self.with_level(|b, level, ticks| {
                        b.destroy_block(level, ticks, at, true);
                    });
                    self.set_state_named(p, "minecraft:end_portal", &[])?;
                }
            }
            self.process_level_events()?;
            // EnderEyeItem.useOn: global level event 1038 (END_PORTAL_SPAWN),
            // heard by every player (vanilla places it at the frame's corner
            // + (1, 0, 1); only its direction matters).
            self.global_level_event(1038, c, 0);
        }
        Ok(true)
    }

    /// EndPortalFrameBlock.getOrCreatePortalShape matched around `frame`: a 5×5
    /// ring (corners free) of filled frames, all facing the centre or all
    /// facing away from it. Returns the centre of the 3×3 opening.
    fn portal_ring_center(&self, frame: [i32; 3]) -> Option<[i32; 3]> {
        let registry = self.pack.assets.registry;
        // (offset from the centre, facing when the frames face the centre)
        let mut ring = Vec::with_capacity(12);
        for i in -1..=1 {
            ring.push(([i, -2], "south"));
            ring.push(([i, 2], "north"));
            ring.push(([-2, i], "east"));
            ring.push(([2, i], "west"));
        }
        let opposite = |f: &str| match f {
            "south" => "north",
            "north" => "south",
            "east" => "west",
            _ => "east",
        };
        for &([ox, oz], _) in &ring {
            let c = [frame[0] - ox, frame[1], frame[2] - oz];
            for inward in [true, false] {
                let ok = ring.iter().all(|&([dx, dz], facing)| {
                    let id = self.world.block(c[0] + dx, c[1], c[2] + dz);
                    let Some(s) = registry.state(id) else {
                        return false;
                    };
                    let want = if inward { facing } else { opposite(facing) };
                    s.name == "minecraft:end_portal_frame"
                        && s.properties.get("eye").map(String::as_str) == Some("true")
                        && s.properties.get("facing").map(String::as_str) == Some(want)
                });
                if ok {
                    return Some(c);
                }
            }
        }
        None
    }

    /// End portals, End gateways and the exit portal, once per tick.
    pub(super) fn tick_end(&mut self) -> Result<()> {
        self.end.gateway_cooldown = self.end.gateway_cooldown.saturating_sub(1);
        self.tick_fight();
        if self.dimension_type().has_ender_dragon_fight {
            // The breath clouds tick in the ECS; the flame ends when its phase is left.
            self.ecs_with(|ecs, mw| ecs.end_flames(mw));
        }
        // The spikes' crystals (placed above) join before they light a fire:
        // a mod that cancels one keeps its fire out too.
        self.native_joins_flush(false, 0);
        self.ecs_end_fire();
        self.tick_eyes();
        if self.dimension_type().has_ender_dragon_fight {
            self.ensure_exit_portal()?;
        }
        Ok(())
    }

    /// Whether a `width` by `height` box at `position` is inside an End
    /// portal (its entity-inside shape), and the End gateway it touches.
    pub(super) fn inside_end_portals(
        &self,
        position: Vec3,
        width: f64,
        height: f64,
    ) -> (bool, Option<[i32; 3]>) {
        let w = width / 2.0;
        let lo = [position.x - w, position.y, position.z - w];
        let hi = [position.x + w, position.y + height, position.z + w];
        let mut portal = false;
        let mut gateway = None;
        for x in lo[0].floor() as i32..=hi[0].floor() as i32 {
            for y in lo[1].floor() as i32..=hi[1].floor() as i32 {
                for z in lo[2].floor() as i32..=hi[2].floor() as i32 {
                    let name = self.block_name([x, y, z]);
                    // EndPortalBlock entity-inside shape: y 6/16..12/16.
                    if &*name == "minecraft:end_portal"
                        && lo[1] < f64::from(y) + 0.75
                        && hi[1] > f64::from(y) + 0.375
                    {
                        portal = true;
                    } else if &*name == "minecraft:end_gateway" && self.dim == Dim::End {
                        gateway = Some([x, y, z]);
                    }
                }
            }
        }
        (portal, gateway)
    }

    /// EndPortalBlock.getPortalDestination for the pending enter_dimension.
    pub(super) fn end_destination(&self, target: Dim) -> Option<Vec3> {
        if !self.end_portal_travel {
            return None;
        }
        if target == Dim::End {
            // Vec3.atBottomCenterOf(END_SPAWN_POINT), one block lower for players.
            return Some(Vec3::new(
                f64::from(END_SPAWN[0]) + 0.5,
                f64::from(END_SPAWN[1] - 1),
                f64::from(END_SPAWN[2]) + 0.5,
            ));
        }
        // Leaving the End: the respawn position, a bed's found once it has
        // loaded (findRespawnPositionAndUseSpawnBlock, not using it up).
        if let Some(c) = self.respawn_config.filter(|c| c.dim == target) {
            return Some(Vec3::new(
                f64::from(c.pos[0]) + 0.5,
                f64::from(c.pos[1]),
                f64::from(c.pos[2]) + 0.5,
            ));
        }
        Some(fallback_spawn(self.spawn, self.level.world_spawn))
    }

    /// Where an End portal's traveller lands once `target` has loaded, and
    /// the yaw it faces there: the platform below the End's spawn point,
    /// facing west, or the top of the Overworld's spawn column.
    /// None while a candidate of the world spawn's search loads (`finder`
    /// keeps it, see `search_spawn`).
    pub(super) fn end_arrival(
        &mut self,
        target: Vec3,
        finder: &mut Option<PlayerSpawnFinder>,
    ) -> Result<Option<(Vec3, Option<f32>)>> {
        if self.dim == Dim::End {
            // EndPlatformFeature.createEndPlatform below the spawn point.
            let origin = [END_SPAWN[0], END_SPAWN[1] - 1, END_SPAWN[2]];
            for dz in -2..=2 {
                for dx in -2..=2 {
                    for dy in -1..3 {
                        let p = [origin[0] + dx, origin[1] + dy, origin[2] + dz];
                        let name = if dy == -1 {
                            "minecraft:obsidian"
                        } else {
                            "minecraft:air"
                        };
                        if &*self.block_name(p) != name {
                            // Portal arrivals rebuild the platform with drops enabled.
                            self.with_level(|b, level, ticks| {
                                b.destroy_block(
                                    level,
                                    ticks,
                                    BlockPos::new(p[0], p[1], p[2]),
                                    true,
                                );
                            });
                            self.set_state_named(p, name, &[])?;
                        }
                    }
                }
            }
            self.native_portal_create("minecraft:the_end", origin);
            // Direction.WEST.toYRot().
            return Ok(Some((target, Some(90.0))));
        }
        // ServerPlayer.findRespawnPositionAndUseSpawnBlock: where the bed or
        // anchor puts the player, facing it; else onto the spawn column.
        let mut target = target;
        if let Some(c) = self.respawn_config.filter(|c| c.dim == self.dim) {
            // This existing owner transition adapter carries yaw only; its pitch path remains open.
            if let Some((at, yaw, _pitch)) = self.find_respawn_and_use(c, false)? {
                return Ok(Some((at, Some(yaw))));
            }
            target = fallback_spawn(self.spawn, self.level.world_spawn);
        }
        let (x, z) = (target.x.floor() as i32, target.z.floor() as i32);
        // TeleportTransition.missingRespawnBlock: PlayerSpawnFinder around
        // the world spawn.
        if let Some(world) = self.level.world_spawn.filter(|w| w[0] == x && w[2] == z) {
            return Ok(match self.search_spawn(world, finder) {
                Ok(at) => Some((at, None)),
                Err(chunk) => {
                    self.spawn_wait.push(chunk);
                    None
                }
            });
        }
        let y = self.land_column(x, z).map_or(target.y, f64::from);
        Ok(Some((Vec3::new(target.x, y, target.z), None)))
    }

    /// EnderDragonFight without a dragon: once the centre of the main island
    /// is loaded, place the active exit portal (EndPodiumFeature) and the
    /// first gateway (spawnNewGateway) if the portal is not there yet. Like
    /// scanState and spawnExitPortal, only while the fight has no exit portal
    /// location: a podium that is later broken stays broken.
    fn ensure_exit_portal(&mut self) -> Result<()> {
        if !(-1..=0).all(|cz| (-1..=0).all(|cx| self.streaming.is_lit(cx, cz)))
            || !self.fight_ready()
        {
            return Ok(());
        }
        let won = self.ecs.fight_view().previously_killed;
        let gateway = self.first_gateway();
        if self.exit_portal().is_none() {
            // findExitPortal remembers the location of a podium that is already there.
            if let Some(level) = self.podium_level() {
                self.set_exit_portal([0, level, 0]);
            }
        }
        if self.exit_portal().is_some() {
            // The gateway may have been out of range when it was placed.
            if won
                && self.world.loaded(gateway[0], gateway[2])
                && &*self.block_name(gateway) != "minecraft:end_gateway"
                && &*self.block_name([gateway[0], gateway[1] + 2, gateway[2]])
                    != "minecraft:bedrock"
            {
                self.place_gateway(gateway)?;
            }
            return Ok(());
        }
        // getHeightmapPos(MOTION_BLOCKING_NO_LEAVES, podium location).below()
        let mut y = (0..256)
            .rev()
            .find(|&y| {
                let f = self.res.facts().get(self.world.block(0, y, 0));
                (f.full_collision || f.fluid != myriade_engine::blocks::Fluid::None)
                    && !f.name.ends_with("_leaves")
            })
            .unwrap_or(0);
        while &*self.block_name([0, y, 0]) == "minecraft:bedrock" && y > 63 {
            y -= 1;
        }
        let origin = [0, y.max(1), 0];
        self.set_exit_portal(origin);
        self.place_podium(origin, won)?;
        if won && self.world.loaded(gateway[0], gateway[2]) {
            self.place_gateway(gateway)?;
        }
        Ok(())
    }
    /// The portal level of the podium around the centre column, read from the
    /// bedrock that EndPodiumFeature places (rim at the portal level, disc
    /// below it, the torch column) so a broken column does not hide it.
    fn podium_level(&self) -> Option<i32> {
        let mut offsets: Vec<[i32; 3]> = (0..4).map(|dy| [0, dy, 0]).collect();
        for z in -4..=4i32 {
            for x in -4..=4i32 {
                let flat = f64::from(x * x + z * z);
                if (6.25..12.25).contains(&flat) {
                    offsets.push([x, 0, z]);
                } else if flat + 1.0 < 6.25 {
                    offsets.push([x, -1, z]);
                }
            }
        }
        (1..253)
            .map(|y| {
                let score = offsets
                    .iter()
                    .filter(|o| &*self.block_name([o[0], y + o[1], o[2]]) == "minecraft:bedrock")
                    .count();
                (score, y)
            })
            .filter(|&(score, _)| score >= 3)
            .max()
            .map(|(_, y)| y)
    }
    /// `spawnExitPortal(true)` after the dragon dies.
    pub(super) fn activate_exit_portal(&mut self) -> Result<()> {
        let origin = match self.exit_portal() {
            Some(o) => o,
            None => {
                // The rim's level, found without assuming the centre column is intact.
                [0, self.podium_level().unwrap_or(61), 0]
            }
        };
        self.place_podium(origin, true)
    }
    pub(super) fn place_gateway_at(&mut self, pos: [i32; 3]) -> Result<()> {
        self.place_gateway(pos)
    }

    /// The first gateway of the shuffled ring (EnderDragonFight constructor
    /// and spawnNewGateway).
    fn first_gateway(&self) -> [i32; 3] {
        let mut gateways: Vec<i32> = (0..20).collect();
        let mut random = LegacyRandom::new(self.level.seed);
        for i in (2..=gateways.len()).rev() {
            let j = random.next_int_bound(i as i32).unwrap_or(0) as usize;
            gateways.swap(i - 1, j);
        }
        let index = f64::from(gateways[gateways.len() - 1]);
        let angle = 2.0 * (-std::f64::consts::PI + std::f64::consts::PI / 20.0 * index);
        [
            (GATEWAY_RADIUS * angle.cos()).floor() as i32,
            GATEWAY_Y,
            (GATEWAY_RADIUS * angle.sin()).floor() as i32,
        ]
    }

    /// EndPodiumFeature(active).place at `origin` (the portal level).
    pub(super) fn place_podium(&mut self, origin: [i32; 3], active: bool) -> Result<()> {
        for y in origin[1] - 1..=origin[1] + 32 {
            for z in origin[2] - 4..=origin[2] + 4 {
                for x in origin[0] - 4..=origin[0] + 4 {
                    let d =
                        (x - origin[0]).pow(2) + (y - origin[1]).pow(2) + (z - origin[2]).pow(2);
                    let d = f64::from(d);
                    let inside_rim = d < 2.5 * 2.5;
                    if !(inside_rim || d < 3.5 * 3.5) {
                        continue;
                    }
                    let name = if y < origin[1] {
                        if inside_rim {
                            "minecraft:bedrock"
                        } else {
                            "minecraft:end_stone"
                        }
                    } else if y > origin[1] {
                        "minecraft:air"
                    } else if !inside_rim {
                        "minecraft:bedrock"
                    } else if active {
                        "minecraft:end_portal"
                    } else {
                        "minecraft:air"
                    };
                    self.set_state_named([x, y, z], name, &[])?;
                }
            }
        }
        for dy in 0..4 {
            self.set_state_named(
                [origin[0], origin[1] + dy, origin[2]],
                "minecraft:bedrock",
                &[],
            )?;
        }
        let registry = self.pack.assets.registry;
        for (facing, d) in [
            ("north", [0, -1]),
            ("south", [0, 1]),
            ("west", [-1, 0]),
            ("east", [1, 0]),
        ] {
            let Some(default) = registry.default_state("minecraft:wall_torch") else {
                continue;
            };
            let mut state = registry.state(default).cloned().ok_or("wall torch")?;
            state.properties.insert("facing".into(), facing.into());
            let id = registry.resolve(&state)?;
            self.level_set([origin[0] + d[0], origin[1] + 2, origin[2] + d[1]], id)?;
        }
        Ok(())
    }

    /// EndGatewayFeature.place: the gateway block in a bedrock cross.
    fn place_gateway(&mut self, origin: [i32; 3]) -> Result<()> {
        for y in -2i32..=2 {
            for z in -1..=1 {
                for x in -1..=1 {
                    let (same_x, same_y, same_z) = (x == 0, y == 0, z == 0);
                    let end = y.abs() == 2;
                    let name = if same_x && same_y && same_z {
                        "minecraft:end_gateway"
                    } else if same_y {
                        "minecraft:air"
                    } else if end && same_x && same_z || (same_x || same_z) && !end {
                        "minecraft:bedrock"
                    } else {
                        "minecraft:air"
                    };
                    self.set_state_named([origin[0] + x, origin[1] + y, origin[2] + z], name, &[])?;
                }
            }
        }
        self.world
            .block_entities
            .get_or_insert_with((origin[0], origin[1], origin[2]), || {
                BlockEntity::EndGateway(Default::default())
            });
        Ok(())
    }

    /// `TheEndGatewayBlockEntity.setExitPosition`: the block entity alone owns the exit.
    fn set_gateway_exit(&mut self, gateway: [i32; 3], exit: [i32; 3], exact: bool) -> Result<()> {
        let chunk = (gateway[0] >> 4, gateway[2] >> 4);
        if !self.world.chunks.contains_key(&chunk) {
            return Err("gateway source chunk is not loaded".into());
        }
        // A gateway removed while an entity waits for the destination stays removed.
        if &*self.block_name(gateway) != "minecraft:end_gateway" {
            return Ok(());
        }
        if let BlockEntity::EndGateway(g) = self
            .world
            .block_entities
            .get_or_insert_with((gateway[0], gateway[1], gateway[2]), || {
                BlockEntity::EndGateway(Default::default())
            })
        {
            g.exit_portal = Some(exit);
            g.exact_teleport = exact;
            self.world.modified.insert(chunk);
        }
        Ok(())
    }

    fn known_gateway_exit(&self, gateway: [i32; 3]) -> Option<([i32; 3], bool)> {
        match self
            .world
            .block_entities
            .get(&(gateway[0], gateway[1], gateway[2]))
        {
            Some(BlockEntity::EndGateway(g)) => g.exit_portal.map(|e| (e, g.exact_teleport)),
            _ => None,
        }
    }

    /// Where `gateway` leads (TheEndGatewayBlockEntity.getPortalPosition):
    /// its exit, or the search for one, and the spot to load before either
    /// is resolved ([`Game::resolve_gateway`]).
    pub(super) fn gateway_job(&mut self, gateway: [i32; 3]) -> (GatewayJob, Vec3) {
        let job = GatewayJob::for_gateway(
            gateway,
            self.world
                .block_entities
                .get(&(gateway[0], gateway[1], gateway[2])),
        );
        if let Some(c) = job.source_chunk()
            && !self.end.areas.contains(&c)
        {
            self.end.areas.push(c);
            self.streaming.extra.push((c, 1));
        }
        let destination = match job {
            GatewayJob::Known { exit, .. } => Vec3::new(
                f64::from(exit[0]) + 0.5,
                f64::from(exit[1]),
                f64::from(exit[2]) + 0.5,
            ),
            GatewayJob::Find { gateway, .. } => {
                let d = glam::DVec2::new(f64::from(gateway[0]), f64::from(gateway[2]))
                    .normalize_or_zero()
                    * 1024.0;
                Vec3::new(d.x, 75.0, d.y)
            }
        };
        (job, destination)
    }

    /// `TheEndGatewayBlockEntity.isChunkEmpty` for the exit search. Vanilla's
    /// `getChunk` loads the candidate on the spot; here the search asks for it
    /// ([`EndState::search`]) and waits, so that which chunks happen to be
    /// loaded cannot change where it ends. Every chunk tested stays asked for.
    fn search_chunk_empty(&mut self, gateway: [i32; 3], x: f64, z: f64) -> Option<bool> {
        let key = ((x / 16.0).floor() as i32, (z / 16.0).floor() as i32);
        self.ask_search_chunk(gateway, key);
        let chunk = self.world.chunks.get(&key)?;
        Some(chunk.sections.iter().all(|s| s.is_uniform(0)))
    }

    /// Asks for `key` as one of `gateway`'s search chunks ([`EndState::search`]),
    /// kept until no traveller waits for that gateway.
    fn ask_search_chunk(&mut self, gateway: [i32; 3], key: (i32, i32)) {
        let corridor = match self.end.search.iter().position(|(g, _)| *g == gateway) {
            Some(i) => &mut self.end.search[i].1,
            None => {
                self.end.search.push((gateway, Vec::new()));
                &mut self.end.search.last_mut().expect("just pushed").1
            }
        };
        if !corridor.contains(&key) {
            corridor.push(key);
        }
    }

    /// Whether every chunk with a block within `reach` of `around` (in x and z)
    /// is loaded. Vanilla's `Level.getBlockState` and `setBlock` load the chunk
    /// they touch (`getChunk(x, z)`), so a lookup that would read or write an
    /// unloaded chunk waits for it here instead of seeing it as air.
    fn chunks_loaded_around(&self, around: [i32; 3], reach: i32) -> bool {
        chunks_around(around, reach).all(|c| self.world.chunks.contains_key(&c))
    }

    /// [`Self::chunks_loaded_around`] for `gateway`'s exit search, which asks
    /// for each of those chunks as it goes ([`Self::ask_search_chunk`]).
    fn search_chunks_loaded(&mut self, gateway: [i32; 3], around: [i32; 3], reach: i32) -> bool {
        for c in chunks_around(around, reach) {
            self.ask_search_chunk(gateway, c);
        }
        self.chunks_loaded_around(around, reach)
    }

    /// TheEndGatewayBlockEntity.findTallestBlock.
    fn tallest_block(&self, around: [i32; 3], dist: i32, allow_bedrock: bool) -> [i32; 3] {
        let mut tallest: Option<[i32; 3]> = None;
        for xd in -dist..=dist {
            for zd in -dist..=dist {
                if xd == 0 && zd == 0 && !allow_bedrock {
                    continue;
                }
                let floor = tallest.map_or(0, |t| t[1]);
                let mut y = 255;
                while y > floor {
                    let p = [around[0] + xd, y, around[2] + zd];
                    let f = self.res.facts().get(self.world.block(p[0], p[1], p[2]));
                    if f.full_collision && (allow_bedrock || &*f.name != "minecraft:bedrock") {
                        tallest = Some(p);
                        break;
                    }
                    y -= 1;
                }
            }
        }
        tallest.unwrap_or(around)
    }

    /// The teleport position of a gateway job, creating the exit gateway (and
    /// an island when the chunk has no landing spot) for new destinations.
    /// `None` while the exit search waits for a candidate chunk to load.
    pub(super) fn resolve_gateway(
        &mut self,
        job: GatewayJob,
        target: Vec3,
    ) -> Result<Option<Vec3>> {
        let bottom_center = myriade_engine::end_gateway::bottom_center;
        let find_exit = |game: &Self, exit: [i32; 3]| {
            let p = game.tallest_block([exit[0], exit[1] + 2, exit[2]], FIND_EXIT_REACH, false);
            [p[0], p[1] + 1, p[2]]
        };
        match job {
            GatewayJob::Known { exit, exact } => {
                // findExitPosition reads 5 blocks around the exit.
                if !exact && !self.chunks_loaded_around(exit, FIND_EXIT_REACH) {
                    return Ok(None);
                }
                Ok(Some(bottom_center(exit_target(exit, exact, |e| {
                    find_exit(self, e)
                }))))
            }
            GatewayJob::Find { gateway, exact } => {
                if !self
                    .world
                    .chunks
                    .contains_key(&(gateway[0] >> 4, gateway[2] >> 4))
                {
                    return Err("gateway source chunk is not loaded".into());
                }
                // The player and the entities may resolve the same gateway: the
                // first one to arrive finds the exit, the others use it.
                let known = self.known_gateway_exit(gateway);
                if let Some((exit, exact)) = known {
                    if !exact && !self.search_chunks_loaded(gateway, exit, FIND_EXIT_REACH) {
                        return Ok(None);
                    }
                    return Ok(Some(bottom_center(exit_target(exit, exact, |e| {
                        find_exit(self, e)
                    }))));
                }
                // findExitPortalXZPosTentative: back while the candidate's chunk is
                // not empty, then forward while it is, 16 steps each way.
                let dir = glam::DVec2::new(f64::from(gateway[0]), f64::from(gateway[2]))
                    .normalize_or_zero();
                let mut pos = glam::DVec2::new(target.x, target.z);
                let mut limit = 16;
                while limit > 0 {
                    match self.search_chunk_empty(gateway, pos.x, pos.y) {
                        Some(false) => {
                            limit -= 1;
                            pos -= dir * 16.0;
                        }
                        Some(true) => break,
                        None => return Ok(None),
                    }
                }
                let mut limit = 16;
                while limit > 0 {
                    match self.search_chunk_empty(gateway, pos.x, pos.y) {
                        Some(true) => {
                            limit -= 1;
                            pos += dir * 16.0;
                        }
                        Some(false) => break,
                        None => return Ok(None),
                    }
                }
                let (cx, cz) = ((pos.x / 16.0).floor() as i32, (pos.y / 16.0).floor() as i32);
                // findValidSpawnInChunk: end stone with two non-full blocks above, closest to 0,0,0.
                // BlockPos.betweenClosed walks x fastest, then y, then z: the first
                // of two spots at the same distance wins.
                let mut closest: Option<([i32; 3], f64)> = None;
                for z in cz * 16..cz * 16 + 16 {
                    for y in 30..256 {
                        for x in cx * 16..cx * 16 + 16 {
                            if &*self.block_name([x, y, z]) != "minecraft:end_stone"
                                || self
                                    .res
                                    .facts()
                                    .get(self.world.block(x, y + 1, z))
                                    .full_collision
                                || self
                                    .res
                                    .facts()
                                    .get(self.world.block(x, y + 2, z))
                                    .full_collision
                            {
                                continue;
                            }
                            let d = (f64::from(x) + 0.5).powi(2)
                                + (f64::from(y) + 0.5).powi(2)
                                + (f64::from(z) + 0.5).powi(2);
                            if closest.is_none_or(|c| d < c.1) {
                                closest = Some(([x, y, z], d));
                            }
                        }
                    }
                }
                let island = closest.is_none();
                let spot = match closest {
                    Some((p, _)) => p,
                    None => [
                        (pos.x + 0.5).floor() as i32,
                        75,
                        (pos.y + 0.5).floor() as i32,
                    ],
                };
                // Everything from here reads and writes through the level, which
                // loads each chunk it touches: the island (within 6 blocks of the
                // spot), findTallestBlock (16), then around the exit it finds, the
                // exit gateway (1) and findExitPosition (5). Nothing is written
                // until all of them are loaded, so a retry starts from the same blocks.
                let reach = TALLEST_REACH + if exact { 1 } else { FIND_EXIT_REACH };
                if !self.search_chunks_loaded(gateway, spot, reach) {
                    return Ok(None);
                }
                if island {
                    self.place_end_island(spot)?;
                }
                let tallest = self.tallest_block(spot, TALLEST_REACH, true);
                let exit = [tallest[0], tallest[1] + 10, tallest[2]];
                self.place_gateway(exit)?;
                self.set_gateway_exit(exit, gateway, false)?;
                // setExitPosition keeps the gateway's own exactTeleport.
                self.set_gateway_exit(gateway, exit, exact)?;
                Ok(Some(bottom_center(exit_target(exit, exact, |e| {
                    find_exit(self, e)
                }))))
            }
        }
    }

    /// EndIslandFeature with RandomSource.create(pos.asLong()).
    fn place_end_island(&mut self, origin: [i32; 3]) -> Result<()> {
        let packed = ((i64::from(origin[0]) & 0x3FF_FFFF) << 38)
            | ((i64::from(origin[2]) & 0x3FF_FFFF) << 12)
            | (i64::from(origin[1]) & 0xFFF);
        let mut random = LegacyRandom::new(packed);
        let mut size = random.next_int_bound(3).unwrap_or(0) as f32 + 4.0;
        let mut y = 0;
        while size > 0.5 {
            for x in (-size).floor() as i32..=size.ceil() as i32 {
                for z in (-size).floor() as i32..=size.ceil() as i32 {
                    if (x * x + z * z) as f32 <= (size + 1.0) * (size + 1.0) {
                        self.set_state_named(
                            [origin[0] + x, origin[1] + y, origin[2] + z],
                            "minecraft:end_stone",
                            &[],
                        )?;
                    }
                }
            }
            size -= random.next_int_bound(2).unwrap_or(0) as f32 + 0.5;
            y -= 1;
        }
        Ok(())
    }

    /// EnderEyeItem.use: findNearestMapStructure over the stronghold rings,
    /// then an EyeOfEnder signalled towards it. Without a stronghold (other
    /// dimensions) nothing is thrown.
    fn throw_ender_eye(&mut self) {
        let rings = self
            .end
            .locator
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|t| t.ring_positions("minecraft:strongholds"));
        let (Some(rings), Some(p)) = (rings, self.player.as_ref()) else {
            return;
        };
        let block = [
            p.position.x.floor(),
            p.position.y.floor(),
            p.position.z.floor(),
        ];
        // ChunkGenerator.getNearestGeneratedStructure: the ring chunk closest to
        // its (8, 32, 8) block, located at the chunk's corner.
        let Some(nearest) = rings.iter().min_by(|a, b| {
            let d = |c: &horizon_core::position::ChunkPos| {
                (f64::from(c.x * 16 + 8) - block[0]).powi(2)
                    + (32.0 - block[1]).powi(2)
                    + (f64::from(c.z * 16 + 8) - block[2]).powi(2)
            };
            d(a).total_cmp(&d(b))
        }) else {
            return;
        };
        let target = glam::DVec3::new(f64::from(nearest.x * 16), 0.0, f64::from(nearest.z * 16));
        let start = glam::DVec3::new(
            p.position.x,
            p.position.y + f64::from(p.dimensions.height) * 0.5,
            p.position.z,
        );
        // EyeOfEnder.signalTo.
        let delta = target - start;
        let horizontal = delta.x.hypot(delta.z);
        let target = if horizontal > 12.0 {
            start
                + glam::DVec3::new(
                    delta.x / horizontal * 12.0,
                    8.0,
                    delta.z / horizontal * 12.0,
                )
        } else {
            target
        };
        let survive = self.world.random.next_int_bound(5).unwrap_or(0) > 0;
        let at = [p.position.x, p.position.y, p.position.z];
        let ctx = self.player_context(None);
        self.post_game_event(
            "minecraft:projectile_shoot",
            [start.x, start.y, start.z],
            ctx,
        );
        let id = self.take_entity_id();
        self.end.eyes.push(EyeOfEnder {
            id,
            position: start,
            previous: start,
            velocity: glam::DVec3::ZERO,
            target,
            life: 0,
            survive,
        });
        let r = self.world.random.next_float();
        let pitch = 0.33 + r * (0.5 - 0.33);
        self.audio
            .play("minecraft:entity.ender_eye.launch", Some(at), 1.0, pitch);
        // CriteriaTriggers.USED_ENDER_EYE, against the feature's block.
        let (dx, dz) = (
            at[0] - f64::from(nearest.x * 16),
            at[2] - f64::from(nearest.z * 16),
        );
        self.trigger_used_ender_eye(dx * dx + dz * dz);
        self.consume_held();
        self.award_stat("used", "minecraft:ender_eye", 1);
        self.swing_arm();
    }

    /// EyeOfEnder.tick: fly, then drop (4 in 5) or shatter after 80 ticks.
    fn tick_eyes(&mut self) {
        let _save_lease = myriade_engine::join_seq::visibility::PassLease::enter();
        let mut eyes = std::mem::take(&mut self.end.eyes);
        let mut shattered = Vec::new();
        eyes.retain_mut(|eye| {
            eye.previous = eye.position;
            let next = eye.position + eye.velocity;
            eye.velocity = EyeOfEnder::update_delta(eye.velocity, next, eye.target);
            eye.position = next;
            // Its move is filed at once (told with the dragon's step), also for one about to go.
            self.entity_section_step(eye.id, eye.previous.to_array(), next.to_array());
            eye.life += 1;
            if eye.life <= 80 {
                return true;
            }
            let at = eye.position.to_array();
            self.audio
                .play("minecraft:entity.ender_eye.death", Some(at), 1.0, 1.0);
            if eye.survive {
                // new ItemEntity(level, x, y, z, item): random spread, 0.2 up.
                let vx = self.world.random.next_double() * 0.2 - 0.1;
                let vz = self.world.random.next_double() * 0.2 - 0.1;
                let stack = ItemStack {
                    item: "minecraft:ender_eye".into(),
                    count: 1,
                    damage: 0,
                    extra: None,
                };
                self.spawn_item(stack, at, [vx, 0.2, vz], 0);
            } else {
                shattered.push(at.map(|v| v.floor() as i32));
            }
            false
        });
        self.end.eyes = eyes;
        for pos in shattered {
            self.level_event_at(2003, pos, 0);
        }
    }

    // ---- End gateway travel for the entities ----

    /// `EndGatewayBlock.entityInside` for the items, mobs and thrown ender
    /// pearls in the End: the gateway blocks their box swept through. A
    /// gateway that is cooling down (`TheEndGatewayBlockEntity.triggerCooldown`)
    /// takes nobody in. `entityInside` comes from the entity's own tick
    /// (`applyEffectsFromBlocks`), and `ServerLevel.tick` ticks only the
    /// entities in entity-ticking chunks: one past the simulation distance
    /// enters none (and costs nothing here, however many lie there).
    pub(super) fn find_end_gateway_portals(&mut self) {
        if self.dim != Dim::End {
            return;
        }
        let is_gateway = |game: &Self, p: BlockPos| {
            &*game.res.facts().get(game.world.block(p.x, p.y, p.z)).name == "minecraft:end_gateway"
        };
        let ticking = self.entity_ticking_area();
        let ticks = |at: Vec3| ticking(at.x.floor() as i32, at.z.floor() as i32);
        let items: Vec<(u32, Vec3, f32, f32)> = self
            .ecs.item_views()
            .iter()
            .filter(|e| ticks(e.position))
            .map(|e| (e.id, e.position, 0.25, 0.25))
            .collect();
        let mut mobs: Vec<(u32, Vec3, f32, f32)> = Vec::new();
        // The ECS creatures, items and carts that moved (`portal_entries`): the legacy lists above are empty while the ECS is active.
        if self.ecs.is_active() {
            let entries = self.ecs_with(|ecs, mw| ecs.portal_entries(mw));
            for (id, can_sweep, at) in entries {
                let at = Vec3::new(at[0], at[1], at[2]);
                if can_sweep && ticks(at) {
                    if let Some((width, height)) = self.ecs.portal_size(id) {
                        mobs.push((id, at, width, height));
                    }
                }
            }
        }
        let mut inside: Vec<(u32, BlockPos)> = Vec::new();
        for (id, to, width, height) in items.into_iter().chain(mobs) {
            let from = self.portals.before.get(&id).copied().unwrap_or(to);
            let found = myriade_engine::end_gateway::swept_gateway(from, to, width, height, |p| {
                is_gateway(self, p)
            });
            if let Some(pos) = found {
                inside.push((id, pos));
            }
        }
        for (id, pos) in inside {
            // One entity at a time: the gateway cools down for 40 ticks.
            if !self.gateway_enter([pos.x, pos.y, pos.z]) {
                continue;
            }
            // An entity that just came through any portal stays out of this one.
            if self.restart_portal_cooldown(id) {
                continue;
            }
            let delay = self.dimension_changing_delay(id);
            self.end
                .gateways
                .entry(id)
                .or_default()
                .enter_with_delay(pos, delay);
        }
        // ThrowableProjectile applies the gateway's block effect before Entity.tick.
        self.handle_end_gateway_portals_phase(true);
    }

    /// `ThrownEnderpearl.onInsideBlock` in an End gateway: its owner's
    /// `ServerPlayer.onInsideBlock`, the enter_block trigger, as if they had
    /// walked in. A guest's pearl triggers theirs; one whose guest has left
    /// has no owner.
    fn pearl_owner_in_gateway(&mut self, guest: Option<u32>, pos: BlockPos) {
        let state = self.world.block(pos.x, pos.y, pos.z);
        let Some(conn) = guest else {
            self.trigger_enter_block(state);
            return;
        };
        self.as_guest(conn, |g| g.trigger_enter_block(state));
    }

    /// `Entity.getPortalCooldown` of an ender pearl: only an End gateway gives
    /// it one (zero when it has none).
    pub(super) fn pearl_portal_cooldown(&self, id: u32) -> i32 {
        self.end.gateways.get(&id).map_or(0, |t| t.cooldown())
    }

    /// `Entity.handlePortal` for the entities in an End gateway: they go
    /// through to the gateway's exit, where they wait for its chunks.
    pub(super) fn handle_end_gateway_portals(&mut self) {
        self.handle_end_gateway_portals_phase(false);
    }

    fn handle_end_gateway_portals_phase(&mut self, after_move: bool) {
        // Nobody in a gateway: nothing to look up.
        if self.dim != Dim::End || self.end.gateways.is_empty() {
            return;
        }
        // `handlePortal` runs in the entity's tick: one past the simulation
        // distance waits in its gateway, its state kept as it is.
        let ticking = self.entity_ticking_area();
        let ticks = |at: Vec3| ticking(at.x.floor() as i32, at.z.floor() as i32);
        let mut ids: Vec<(u32, bool, bool)> = self
            .ecs.item_views()
            .iter()
            .map(|e| (e.id, true, ticks(e.position)))
            .collect();
        if self.ecs.is_active() {
            // Only the entities in a gateway have a state: the ECS rows are asked for those alone.
            let held: Vec<u32> = self.end.gateways.keys().copied().collect();
            for id in held {
                if let Some((at, _, _)) = self.ecs_body(id) {
                    ids.push((id, self.ecs.can_use_portal(id), ticks(at)));
                }
            }
        }
        let mut present: std::collections::HashSet<u32> =
            ids.iter().map(|(id, _, _)| *id).collect();
        // A pearl waiting for its exit keeps its portal cooldown.
        present.extend(self.end.arrivals.iter().map(|a| match &a.traveller {
            Arriving::Pearl(p) => p.id,
            Arriving::Live(id) => *id,
        }));
        self.end.gateways.retain(|id, _| present.contains(id));
        let mut leaving: Vec<(u32, BlockPos)> = Vec::new();
        for (id, allowed, ticking) in ids {
            if !ticking {
                continue;
            }
            // Only an entity in a gateway has a state: skip the others before
            // the per-id lookups below, which walk the entity lists.
            if !self.end.gateways.contains_key(&id) {
                continue;
            }
            if after_move {
                continue;
            }
            let delay = self.dimension_changing_delay(id);
            let Some(state) = self.end.gateways.get_mut(&id) else {
                continue;
            };
            // The End gateway has no transition time (Portal.getPortalTransitionTime).
            if let Some(entry) = state.travel_with_delay(allowed, delay) {
                leaving.push((id, entry));
            }
            if state.is_done() {
                self.end.gateways.remove(&id);
            }
        }
        for (id, entry) in leaving {
            self.send_through_gateway(id, entry);
        }
    }

    /// `Entity.teleportToPortalDestination` for an entity that goes through an
    /// End gateway: an ordinary same-level entity stays live while the exit loads.
    fn send_through_gateway(&mut self, id: u32, gateway: BlockPos) {
        if self.ecs.entity_of(id).is_some()
            || matches!(
            self.level_entity(id as i32),
            Some(
                super::commands::Target::Item(_)
                    | super::commands::Target::Mob(_)
                    | super::commands::Target::Boat(_)
                    | super::commands::Target::Cart(_)
            )
        ) {
            let (job, target) = self.gateway_job([gateway.x, gateway.y, gateway.z]);
            let arrival = GatewayArrival {
                traveller: Arriving::Live(id),
                job,
                target,
            };
            if let Some(waiting) = self.arrive_through_gateway(arrival) {
                self.end.arrivals.push(waiting);
            }
            return;
        }
    }

    /// The entities that reached a gateway's exit: they land where the
    /// gateway leads, once its chunks have loaded.
    pub(super) fn resolve_end_gateway_arrivals(&mut self) -> Result<()> {
        if self.dim != Dim::End {
            return Ok(());
        }
        for arrival in std::mem::take(&mut self.end.arrivals) {
            if let Some(waiting) = self.arrive_through_gateway(arrival) {
                self.end.arrivals.push(waiting);
            }
        }
        Ok(())
    }

    /// `Entity.teleport` of a gateway traveller once its exit's chunk has
    /// loaded: the ordinary entity moves live and a pearl flies on from there.
    /// The traveller comes back while it still waits.
    fn arrive_through_gateway(&mut self, arrival: GatewayArrival) -> Option<GatewayArrival> {
        let chunk = |at: Vec3| ((at.x.floor() as i32) >> 4, (at.z.floor() as i32) >> 4);
        if !self.world.chunks.contains_key(&chunk(arrival.target))
            || arrival
                .job
                .source_chunk()
                .is_some_and(|c| !self.world.chunks.contains_key(&c))
        {
            return Some(arrival);
        }
        let at = match self.resolve_gateway(arrival.job, arrival.target) {
            Ok(Some(at)) => at,
            // The exit search waits for a candidate chunk to load.
            Ok(None) => return Some(arrival),
            Err(e) => {
                // The traveller waits for the exit: the failure is retried.
                log::warn!("End gateway: {e}");
                return Some(arrival);
            }
        };
        let key = chunk(at);
        match arrival.traveller {
            Arriving::Live(id) => {
                if self.ecs.entity_of(id).is_some() {
                    // `Entity.teleportSameDimension` for an ECS entity: the place and the portal cooldown.
                    // The riders move in the same operation, each by the root's shift (`ecs.teleport` moves one entity).
                    let root_from = self.ecs.position(id);
                    let moves: Vec<(bevy_ecs::entity::Entity, [f64; 3])> = self
                        .ecs_tree(id)
                        .into_iter()
                        .filter_map(|g| {
                            let (ge, p) = (self.ecs.entity_of(g)?, self.ecs.position(g)?);
                            let to = match root_from {
                                Some(r) if g != id => [p[0] - r[0] + at.x, p[1] - r[1] + at.y, p[2] - r[2] + at.z],
                                _ => [at.x, at.y, at.z],
                            };
                            Some((ge, to))
                        })
                        .collect();
                    self.ecs_with_mw(|ecs, mw| {
                        for (ge, to) in moves {
                            ecs.teleport(mw, ge, to);
                        }
                    });
                    self.set_portal_cooldown(id);
                    self.place_portal_ticket(key);
                } else if let Some(target) = self.level_entity(id as i32) {
                    // Entity.teleportSameDimension keeps the live entity and moves every
                    // passenger in the same operation. The gateway's transition has
                    // `Relative.DELTA`: the motion `/tp`'s rule in there stops is put back.
                    let motion = self.transition_motion(target);
                    self.teleport_to_in_level(target, [at.x, at.y, at.z], None, [false; 3], false);
                    if let Some(motion) = motion {
                        self.restore_transition_motion(target, motion);
                    }
                    self.set_portal_cooldown(id);
                    self.place_portal_ticket(key);
                }
            }
            Arriving::Pearl(mut pearl) => {
                myriade_engine::end_gateway::place_pearl(&mut pearl, at);
                // ThrownEnderpearl.teleport: placePortalTicket where it came out.
                self.end.tickets.retain(|(c, _)| *c != key);
                self.end.tickets.push((key, PORTAL_TICKET));
                self.add_projectile(*pearl);
            }
        }
        None
    }

    /// The chunks the entities waiting to arrive need, kept loaded like the
    /// areas a server keeps around its players.
    pub(super) fn keep_gateway_destinations(&mut self) {
        let in_end = self.dim == Dim::End;
        let areas = &self.end.areas;
        // The exit searches' chunks are End chunks, asked for in the End's ticks.
        let searched =
            |c: &(i32, i32)| in_end && self.end.search.iter().any(|(_, s)| s.contains(c));
        self.streaming
            .extra
            .retain(|&(c, r)| !(r == 1 && (areas.contains(&c) || searched(&c))));
        if in_end {
            // A search's chunks are released once no traveller waits for its gateway.
            let waiting: Vec<[i32; 3]> = self
                .end
                .arrivals
                .iter()
                .map(|a| a.job)
                .chain(self.gateway_trip_jobs())
                .filter_map(|job| match job {
                    GatewayJob::Find { gateway, .. } => Some(gateway),
                    GatewayJob::Known { .. } => None,
                })
                .collect();
            self.end.search.retain(|(g, _)| waiting.contains(g));
        }
        // The pearls' portal tickets hold End chunks, and run out there
        // (TicketType.PORTAL's timeout).
        if in_end {
            self.end.tickets.retain_mut(|(_, ticks)| {
                *ticks = ticks.saturating_sub(1);
                *ticks > 0
            });
        }
        let mut areas = Vec::new();
        let arrivals = self.end.arrivals.iter().map(|a| {
            (
                (a.target.x.floor() as i32) >> 4,
                (a.target.z.floor() as i32) >> 4,
            )
        });
        let tickets = self.end.tickets.iter().filter(|_| in_end);
        let sources = self
            .end
            .arrivals
            .iter()
            .filter_map(|a| a.job.source_chunk());
        let search = self
            .end
            .search
            .iter()
            .filter(|_| in_end)
            .flat_map(|(_, s)| s.iter().copied());
        for c in arrivals
            .chain(tickets.map(|&(c, _)| c))
            .chain(sources)
            .chain(self.gateway_source_chunks())
            .chain(search)
        {
            if !areas.contains(&c) {
                areas.push(c);
                self.streaming.extra.push((c, 1));
            }
        }
        self.end.areas = areas;
    }
}

#[cfg(test)]
#[path = "end/end_gateway_ticking_tests.rs"]
mod ticking_tests;

#[cfg(test)]
#[path = "tests/end_gateway_exit_search_tests.rs"]
mod exit_search_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use myriade_engine::containers::EndGateway;

    #[test]
    fn a_new_exit_keeps_the_gateways_exact_teleport() {
        let exact = BlockEntity::EndGateway(EndGateway {
            exact_teleport: true,
            ..Default::default()
        });
        let plain = BlockEntity::EndGateway(EndGateway::default());
        assert!(exact_teleport(Some(&exact)));
        assert!(!exact_teleport(Some(&plain)));
        assert!(!exact_teleport(None));
        assert!(!exact_teleport(Some(&BlockEntity::DaylightDetector)));
        let exit = [8, 100, 0];
        // Exact: the exit itself, nothing is searched for.
        assert_eq!(exit_target(exit, true, |_| panic!("searched")), exit);
        assert_eq!(
            exit_target(exit, false, |e| [e[0], e[1] + 11, e[2]]),
            [8, 111, 0]
        );
    }

    #[test]
    #[ignore = "daily"]
    fn a_pearl_reads_its_end_gateway_cooldown_and_refuses_the_write() {
        use super::super::native_entity_refs::tests::with_game;

        with_game(|game| {
            let id = game.add_projectile(Projectile::new(
                ProjectileKind::EnderPearl,
                Vec3::ZERO,
                None,
            ));
            let pearl = game.entity(id).unwrap();
            assert_eq!(pearl.portal_cooldown(game), Some(0));
            // The write refuses a pearl by name: its cooldown is the End
            // gateway traveller's, not the level's portal state.
            assert!(!pearl.set_portal_cooldown(game, 40));
            assert_eq!(pearl.portal_cooldown(game), Some(0));
            let traveller = game.end.gateways.entry(id).or_default();
            traveller.enter(BlockPos::new(0, 64, 0));
            assert!(traveller.travel(true).is_some());
            assert_eq!(pearl.portal_cooldown(game), Some(300));
            assert!(!pearl.set_portal_cooldown(game, 40));
            assert_eq!(pearl.portal_cooldown(game), Some(300));
        });
    }

    #[test]
    #[ignore = "daily"]
    fn an_entity_keeps_its_motion_through_a_gateway() {
        use super::super::native_entity_refs::tests::with_game;

        with_game(|game| {
            // The exit's chunk is loaded, so the arrival does not wait.
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(myriade_engine::chunk::Chunk::empty(0, 0)),
            );
            game.spawn_item_own(
                myriade_engine::items::ItemStack::new("minecraft:stick", 1),
                [2.5, 100.0, 2.5],
                0,
            );
            let id = game.ecs.item_ids().last().copied().unwrap();
            let target = Vec3::new(12.5, 100.0, 12.5);
            let motion = Vec3::new(0.117, 0.25, -0.05);
            let e = game.ecs.entity_of(id).unwrap();
            game.ecs.set_velocity(e, motion);
            let arrival = GatewayArrival {
                traveller: Arriving::Live(id),
                job: GatewayJob::Known {
                    exit: [12, 100, 12],
                    exact: true,
                },
                target,
            };
            assert!(game.arrive_through_gateway(arrival).is_none());
            // Entity.teleportSameDimension with the gateway's Relative.DELTA: the
            // entity is at the exit and its motion is kept.
            let item = game.ecs.item(id).unwrap();
            let (at, to) = (item.position, target);
            for (a, t) in [(at.x, to.x), (at.y, to.y), (at.z, to.z)] {
                assert!((a - t).abs() < 1e-3, "{at:?}");
            }
            assert_eq!(item.velocity, motion);
        });
    }

    #[test]
    fn leaving_the_end_without_a_point_of_its_own_goes_to_the_world_spawn() {
        let world = Some([220, 63, -41]);
        assert_eq!(fallback_spawn(None, world), Vec3::new(220.5, 63.0, -40.5));
        assert_eq!(
            fallback_spawn(Some(Vec3::new(10.5, 70.0, 12.5)), world),
            Vec3::new(10.5, 70.0, 12.5)
        );
        assert_eq!(fallback_spawn(None, None), Vec3::new(0.5, 64.0, 0.5));
    }

    #[test]
    fn end_gateway_job_reads_the_block_entity_and_keeps_new_sources_loaded() {
        let gateway = [702, 100, 0];
        let exit = [8, 100, 0];
        for exact in [true, false] {
            let entity = BlockEntity::EndGateway(EndGateway {
                exit_portal: Some(exit),
                exact_teleport: exact,
                ..Default::default()
            });
            assert_eq!(
                GatewayJob::for_gateway(gateway, Some(&entity)),
                GatewayJob::Known { exit, exact },
            );
        }
        let empty = BlockEntity::EndGateway(EndGateway::default());
        for entity in [None, Some(&empty)] {
            assert_eq!(
                GatewayJob::for_gateway(gateway, entity),
                GatewayJob::Known {
                    exit: END_SPAWN,
                    exact: true
                }
            );
            let nearby = [96, 75, 0];
            let exact = BlockEntity::EndGateway(EndGateway {
                exact_teleport: true,
                ..Default::default()
            });
            assert_eq!(
                GatewayJob::for_gateway(nearby, Some(&exact)),
                GatewayJob::Find {
                    gateway: nearby,
                    exact: true
                }
            );
            assert_eq!(
                GatewayJob::for_gateway(nearby, entity),
                GatewayJob::Find {
                    gateway: nearby,
                    exact: false
                }
            );
        }
    }
}

#[cfg(test)]
#[path = "end/gateway_cooldown_tests.rs"]
mod cooldown_tests;
