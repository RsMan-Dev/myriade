//! Pillager patrols (`PatrolSpawner`), the raid omen, and raids (`Raids`):
//! their waves, boss bar, victory and the raiders' bookkeeping. The rules are
//! `horizon_client::patrol` and `horizon_client::raid`; this is the game's
//! side of them.
use super::{Dim, Game};
use horizon_client::{
    effects::{EffectInstance, OmenTick},
    mobs::{Species, raider, spawn_rules},
    patrol::GameplayAttributes,
    raid::{self, Action, BarName, RaidLevel, RaidPlayer, RaiderInfo, RaiderType, Raids, Spawn},
    random_source,
};
use horizon_core::nbt::Tag;
use horizon_core::geometry::Vec3;
use horizon_gameplay::village_siege::{SiegeLevel, VillageSiege};
use std::collections::{HashMap, HashSet};

/// Patrol and raid state owned by the game.
#[derive(Default)]
pub(super) struct Raiding {
    siege: VillageSiege,
    /// The raids of the dimension the player is in, and that dimension.
    pub(super) raids: Raids,
    loaded: Option<Dim>,
    /// `ServerPlayer.raidOmenPosition` (`raid_omen_position`), read from the
    /// player data on first use.
    omen_position: Option<[i32; 3]>,
    omen_loaded: bool,
    /// Raids read before their first raid tick, whose randoms that tick seeds
    /// (their ids and positions in the file).
    unseeded: Option<Vec<(i32, usize)>>,
    attributes: Option<std::sync::Arc<GameplayAttributes>>,
}

impl Raiding {
    pub(super) fn raids(&self, dimension: &str) -> Option<&Raids> {
        self.loaded
            .filter(|dim| dim.id() == dimension)
            .map(|_| &self.raids)
    }
    pub(super) fn raids_mut(&mut self, dimension: &str) -> Option<&mut Raids> {
        self.loaded
            .filter(|dim| dim.id() == dimension)
            .map(|_| &mut self.raids)
    }
    /// Exchange the level-held parts (`ServerLevel.getRaids`) with another
    /// level's (game/world/levels.rs).
    pub(super) fn swap_level(&mut self, other: &mut Self) {
        std::mem::swap(&mut self.raids, &mut other.raids);
        std::mem::swap(&mut self.loaded, &mut other.loaded);
        std::mem::swap(&mut self.unseeded, &mut other.unseeded);
    }
}

pub(super) use horizon_gameplay::custom_spawner::{section, sections_to_village};

fn block_of(p: Vec3) -> [i32; 3] {
    [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
}

/// The level as a raid sees it, with this tick's village data.
struct Lvl<'a, 'g> {
    game: &'a Game<'g>,
    difficulty: i32,
    centers: &'a HashSet<(i32, i32, i32)>,
}
impl RaidLevel for Lvl<'_, '_> {
    fn difficulty(&self) -> i32 {
        self.difficulty
    }
    fn has_chunk_at(&self, p: [i32; 3]) -> bool {
        self.game.world.chunks.contains_key(&(p[0] >> 4, p[2] >> 4))
    }
    fn has_chunks_at(&self, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
        (x0 >> 4..=x1 >> 4)
            .all(|cx| (z0 >> 4..=z1 >> 4).all(|cz| self.game.world.chunks.contains_key(&(cx, cz))))
    }
    fn sections_to_village(&self, p: [i32; 3]) -> i32 {
        sections_to_village(self.centers, section(p))
    }
    fn surface_height(&self, x: i32, z: i32) -> i32 {
        self.game.surface_height(x, z)
    }
    /// `ServerLevel.isPositionEntityTicking`: a loaded chunk whose entities
    /// tick (`canPositionTick`) in entity ticking range, not merely loaded.
    fn is_position_entity_ticking(&self, p: [i32; 3]) -> bool {
        self.has_chunk_at(p) && self.game.tickets.levels().entity_ticking_at(p[0], p[2])
    }
    fn ravager_can_spawn(&self, [x, y, z]: [i32; 3]) -> bool {
        let g = self.game;
        let w = g.mob_world_base(&[]);
        spawn_rules::is_spawn_position_ok("minecraft:ravager", Species::Ravager, &w, x, y, z)
            || w.block_name(x, y - 1, z) == "minecraft:snow"
                && g.res.facts().is_air(g.world.block(x, y, z))
    }
    fn players(&self) -> Vec<RaidPlayer> {
        // The players of the level in place: a server has none of its own.
        self.game
            .level_players()
            .into_iter()
            .map(|p| RaidPlayer {
                uuid: p.uuid,
                position: [p.position.x, p.position.y, p.position.z],
                block: block_of(p.position),
                alive: p.alive,
            })
            .collect()
    }
}

impl Game<'_> {
    /// `Heightmap.Types.WORLD_SURFACE`: the first free block above the
    /// highest non-air one.
    pub(super) fn surface_height(&self, x: i32, z: i32) -> i32 {
        let top = if self.dim == Dim::Nether { 127 } else { 319 };
        (self.world.bottom_y()..=top)
            .rev()
            .find(|&y| !self.res.facts().is_air(self.world.block(x, y, z)))
            .map_or(self.world.bottom_y(), |y| y + 1)
    }
    /// Every claimed village point of interest (job sites, beds, bells).
    fn village_pois(&self) -> Vec<[i32; 3]> {
        self.ecs.village_points()
    }
    pub(super) fn difficulty_id(&self) -> i32 {
        use horizon_world::food::Difficulty;
        match self.world.health_rules.difficulty {
            Difficulty::Peaceful => 0,
            Difficulty::Easy => 1,
            Difficulty::Normal => 2,
            Difficulty::Hard => 3,
        }
    }
    fn player_block(&self) -> Option<[i32; 3]> {
        self.player.as_ref().map(|p| block_of(p.position))
    }
    /// `dimensions/<dimension>/data/minecraft/raids.dat`: each dimension
    /// keeps its own raids.
    fn raids_path(&self, dim: Dim) -> std::path::PathBuf {
        horizon_client::save::dimension_dir(&self.world_root, dim.id())
            .join("data/minecraft/raids.dat")
    }
    /// The raids of the player's dimension, seeded (the previous dimension's
    /// saved first).
    fn load_raids(&mut self) {
        self.read_raids();
        if let Some(order) = self.raiding.unseeded.take() {
            let seed = random_source::unique_seed();
            self.raiding.raids.seed_loaded(seed, &order);
        }
    }
    /// Reads the raids of the player's dimension from its file (the previous
    /// dimension's saved first), as a level opens (`ServerLevel`'s
    /// constructor). Their own randoms are seeded by `load_raids` on the first
    /// raid tick without drawing from the level's stream.
    pub(super) fn read_raids(&mut self) {
        if self.raiding.loaded == Some(self.dim) {
            return;
        }
        if self.raiding.loaded.is_some() {
            self.save_raids();
        }
        self.raiding.loaded = Some(self.dim);
        let (raids, order) = match horizon_client::save::read_dat(&self.raids_path(self.dim))
            .and_then(|tag| tag.get("data").cloned())
        {
            Some(data) => Raids::from_nbt_unseeded(&data),
            None => (Raids::default(), Vec::new()),
        };
        self.raiding.raids = raids;
        self.raiding.unseeded = Some(order);
    }
    /// `Raids` as its dimension's `raids.dat`, when it changed.
    pub(super) fn save_raids(&mut self) {
        let Some(dim) = self.raiding.loaded else {
            return;
        };
        if !self.raiding.raids.dirty {
            return;
        }
        let tag = horizon_core::nbt::Tag::compound([
            ("data", self.raiding.raids.to_nbt()),
            (
                "DataVersion",
                horizon_core::nbt::Tag::Int(horizon_world::DATA_VERSION),
            ),
        ]);
        if horizon_client::save::write_dat(&self.raids_path(dim), tag).is_ok() {
            self.raiding.raids.dirty = false;
        }
        self.save_raid_omen_position();
    }
    /// `ServerPlayer.raidOmenPosition` from the player data.
    fn load_raid_omen_position(&mut self) {
        if self.raiding.omen_loaded {
            return;
        }
        self.raiding.omen_loaded = true;
        self.raiding.omen_position = match self
            .level
            .player_extra
            .as_ref()
            .and_then(|p| p.get("raid_omen_position"))
        {
            Some(horizon_core::nbt::Tag::IntArray(v)) if v.len() == 3 => Some([v[0], v[1], v[2]]),
            _ => None,
        };
    }
    /// Writes `raid_omen_position` into the player data being saved.
    fn save_raid_omen_position(&mut self) {
        if !self.raiding.omen_loaded {
            return;
        }
        let extra = self
            .level
            .player_extra
            .get_or_insert_with(|| horizon_core::nbt::Tag::Compound(Default::default()));
        if let horizon_core::nbt::Tag::Compound(map) = extra {
            match self.raiding.omen_position {
                Some(p) => {
                    map.insert(
                        "raid_omen_position".into(),
                        horizon_core::nbt::Tag::IntArray(p.to_vec()),
                    );
                }
                None => {
                    map.remove(&"raid_omen_position".into());
                }
            }
        }
    }
    fn gameplay_attributes(&mut self) -> std::sync::Arc<GameplayAttributes> {
        if self.raiding.attributes.is_none() {
            self.raiding.attributes = Some(std::sync::Arc::new(GameplayAttributes::load(
                &self.options.resources,
            )));
        }
        self.raiding.attributes.clone().unwrap_or_default()
    }
    /// A boolean gameplay attribute at a position (dimension, biome, then
    /// the timelines at the overworld clock).
    fn gameplay_attribute(&mut self, key: &str, at: [i32; 3]) -> bool {
        let attributes = self.gameplay_attributes();
        let biome = self
            .world
            .chunks
            .get(&(at[0] >> 4, at[2] >> 4))
            .map(|c| {
                self.res
                    .biomes()
                    .get(c.biome(at[0], at[1], at[2]))
                    .name
                    .clone()
            })
            .unwrap_or_default();
        attributes.value(key, &self.dimension_type().id, &biome, self.world.day_time)
    }

    /// `BadOmenMobEffect` and `RaidOmenMobEffect.applyEffectTick` for the
    /// player's effect tick: Bad Omen inside a village becomes Raid Omen,
    /// whose last tick starts or extends a raid where it began.
    pub(super) fn omen_tick(&mut self, e: &EffectInstance) -> OmenTick {
        if self.spectator() || self.player.is_none() {
            return OmenTick::Continue;
        }
        let Some(pos) = self.player_block() else {
            return OmenTick::Continue;
        };
        self.load_raids();
        self.load_raid_omen_position();
        match e.id {
            "bad_omen" => {
                if self.difficulty_id() != 0
                    && sections_to_village(&self.village_centers(), section(pos)) <= 1
                    && self
                        .raiding
                        .raids
                        .nearby(pos)
                        .is_none_or(|(_, r)| r.omen_level < raid::MAX_OMEN_LEVEL)
                {
                    self.raiding.omen_position = Some(pos);
                    return OmenTick::Add(EffectInstance::new("raid_omen", 600, e.amplifier));
                }
                OmenTick::Continue
            }
            "raid_omen" => match self.raiding.omen_position.take() {
                Some(at) => {
                    self.create_or_extend_raid(at, Some(e.amplifier));
                    OmenTick::Remove
                }
                None => OmenTick::Continue,
            },
            _ => OmenTick::Continue,
        }
    }
    /// The Raid Omen sound when the effect is first added
    /// (`MobEffect.onEffectAdded`: at the player's feet).
    pub(super) fn raid_omen_added(&mut self) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let at = [p.position.x, p.position.y, p.position.z];
        self.audio
            .play("minecraft:event.mob_effect.raid_omen", Some(at), 1.0, 1.0);
    }
    /// `Raids.createOrExtendRaid`: not for spectators, with the `raids` rule
    /// and where raids may start; the raid centres on the occupied village
    /// points of interest within 64 blocks.
    pub(super) fn create_or_extend_raid(
        &mut self,
        at: [i32; 3],
        amplifier: Option<i32>,
    ) -> Option<i32> {
        if self.spectator()
            || !self
                .session
                .rules
                .bool_at(horizon_client::gamerules::vanilla::RAIDS)
            || !self.gameplay_attribute(horizon_client::patrol::CAN_START_RAID, at)
        {
            return None;
        }
        self.load_raids();
        let center = raid::raid_center(at, &self.village_pois());
        // The new raid's own random: `RandomSource.create()`, not the level's.
        let seed = random_source::unique_seed();
        let difficulty = self.difficulty_id();
        let (id, triggered) = self
            .raiding
            .raids
            .create_or_extend(center, difficulty, amplifier, seed);
        if triggered {
            self.award_custom_stat("raid_trigger", 1);
            self.trigger_player("minecraft:voluntary_exile");
            if let Some(level) = self.level(self.dim.id()) {
                self.native_raid_state_change(
                    mod_api::RaidRef::new(level, id),
                    center,
                    "triggered",
                );
            }
        }
        Some(id)
    }
    /// The raids of the level in place, once they are read (`read_raids`).
    pub(super) fn raids_in_place(&self) -> Option<&Raids> {
        self.raiding.raids(self.dim.id())
    }
    /// `ServerLevel.getRaidAt`.
    pub(super) fn raiding_nearby(&self, at: [i32; 3]) -> Option<i32> {
        self.raiding.raids.nearby(at).map(|(id, _)| id)
    }
    /// `/raid start`: a raid in the local village at the given omen level.
    /// `RaidCommand.start` reaches it through `Raids.createOrExtendRaid`, so
    /// the player's Raid Omen is absorbed (`Raid.absorbRaidOmen`) before the
    /// command sets the level.
    pub(super) fn raid_command_create(&mut self, at: [i32; 3], omen_level: i32) -> bool {
        self.load_raids();
        let amplifier = self.potions.effects.amplifier("raid_omen");
        let Some(id) = self.create_or_extend_raid(at, amplifier) else {
            return false;
        };
        if let Some(r) = self.raiding.raids.get_mut(id) {
            r.omen_level = omen_level;
        }
        self.raiding.raids.dirty = true;
        true
    }
    pub(super) fn raid_command_stop(&mut self, id: i32) {
        if let Some(center) = self.raiding.raids.stop(id)
            && let Some(level) = self.level(self.dim.id())
        {
            self.native_raid_state_change(mod_api::RaidRef::new(level, id), center, "stopped");
        }
    }
    pub(super) fn raid_set_omen(&mut self, id: i32, level: i32) -> i32 {
        self.raiding.raids.set_omen_level(id, level).unwrap_or(0)
    }
    /// The `/raid check` summary line.
    pub(super) fn raid_status_text(&self, id: i32) -> String {
        let Some(r) = self.raiding.raids.get(id) else {
            return String::new();
        };
        let members = self.ecs.raid_members(id);
        let health: f32 = members.iter().fold(0.0, |a, m| a + m.2);
        // Java's string concatenation of the floats: `20.0`, not `20`.
        format!(
            "Num groups spawned: {} Raid omen level: {} Num mobs: {} Raid health: {} / {}",
            r.groups_spawned,
            r.omen_level,
            members.len(),
            horizon_core::java_number::float_to_string(health),
            horizon_core::java_number::float_to_string(r.total_health)
        )
    }
    /// `getHealthOfLivingRaiders`, leaving out one raider.
    fn remaining_raider_health(&self, id: i32, except: Option<u32>) -> f32 {
        self.ecs.raiders_of(id).iter().filter(|m| Some(m.id) != except).map(|m| m.health).sum()
    }
    /// The members of every raid, for `Raids.tick`.
    fn raider_infos(&self) -> HashMap<i32, Vec<RaiderInfo>> {
        let mut out: HashMap<i32, Vec<RaiderInfo>> = HashMap::new();
        for id in self.raiding.raids.ids() {
            let infos: Vec<RaiderInfo> = self
                .ecs
                .raiders_of(id)
                .into_iter()
                .map(|r| RaiderInfo {
                    id: r.id,
                    wave: r.wave,
                    health: r.health,
                    position: r.position,
                    in_level: true,
                    patrol_leader: r.leader,
                    tick_count: r.tick_count,
                    no_action_time: r.no_action_time,
                    ticks_outside: r.ticks_outside,
                    ..Default::default()
                })
                .collect();
            if !infos.is_empty() {
                out.insert(id, infos);
            }
        }
        out
    }
    /// `ServerLevel.tick` for the raids and their raiders.
    pub(super) fn tick_raids(&mut self) {
        self.load_raids();
        let centers = self.village_centers();
        let infos = self.raider_infos();
        let old_states: HashMap<_, _> = self
            .raiding
            .raids
            .ids()
            .into_iter()
            .filter_map(|id| {
                self.raiding
                    .raids
                    .get(id)
                    .map(|r| (id, (r.status, r.center, r.active, r.groups_spawned)))
            })
            .collect();
        let mut raids = std::mem::take(&mut self.raiding.raids);
        let actions = {
            let lvl = Lvl {
                game: self,
                difficulty: self.difficulty_id(),
                centers: &centers,
            };
            let members = |id: i32| infos.get(&id).cloned().unwrap_or_default();
            raids.tick(
                &lvl,
                self.session
                    .rules
                    .bool_at(horizon_client::gamerules::vanilla::RAIDS),
                &members,
            )
        };
        self.raiding.raids = raids;
        // The ECS reads only a view of each raid (centre, active, over, lost), written when the raid appears, changes phase or goes.
        let mut views: Vec<(i32, Option<horizon_ecs_proto::raids::RaidView>)> = Vec::new();
        for id in self.raiding.raids.ids() {
            if let Some(r) = self.raiding.raids.get(id)
                && old_states.get(&id).is_none_or(|(s, c, a, g)| *s != r.status || *c != r.center || *a != r.active || *g != r.groups_spawned)
            {
                views.push((
                    id,
                    Some(horizon_ecs_proto::raids::RaidView {
                        center: r.center,
                        active: r.active,
                        over: r.is_over(),
                        loss: r.is_loss(),
                        wave: r.groups_spawned,
                    }),
                ));
            }
        }
        views.extend(old_states.keys().filter(|id| self.raiding.raids.get(**id).is_none()).map(|id| (*id, None)));
        for (id, view) in views {
            self.ecs.set_raid_view(id, view);
        }
        for (id, (old, ..)) in old_states {
            if let Some(r) = self.raiding.raids.get(id)
                && r.status != old
                && let Some(level) = self.level(self.dim.id())
            {
                self.native_raid_state_change(
                    mod_api::RaidRef::new(level, id),
                    r.center,
                    r.status.name(),
                );
            }
        }
        for (id, action) in actions {
            self.apply_raid_action(id, action);
        }
        let infos = self.raider_infos();
        self.sync_villager_raids(&infos);
    }
    fn apply_raid_action(&mut self, id: i32, action: Action) {
        match action {
            Action::Spawn { pos, wave, spawns } => self.spawn_wave(id, pos, wave, &spawns),
            Action::Horn { pos, seed } => self.raid_horn(id, pos, seed),
            Action::Remove { id: mob } => {
                self.ecs.leave_raid(mob);
            }
            Action::SetOutside { id: mob, ticks } => {
                self.ecs.set_ticks_outside_raid(mob, ticks);
            }
            // Raid.tick on a victory: each hero who is still alive.
            Action::Victory { heroes, amplifier } => self.for_each_player(|g| {
                if heroes.contains(&g.acting_uuid()) && !g.spectator() {
                    let mut effect =
                        EffectInstance::new("hero_of_the_village", raid::HERO_DURATION, amplifier);
                    effect.visible = false;
                    g.potions.effects.add(effect);
                    g.award_custom_stat("raid_win", 1);
                    g.trigger_player("minecraft:hero_of_the_village");
                }
            }),
        }
    }
    /// `Raid.playSound`: the horn, 13 blocks from each player toward the wave,
    /// for everyone within 64 blocks or in the raid. One seed for every packet
    /// (`SoundSource.NEUTRAL`, volume 64, pitch 1).
    fn raid_horn(&mut self, id: i32, pos: [i32; 3], seed: i64) {
        let in_raid = self.raiding.raids.get(id).is_some_and(|r| r.bar.player);
        let (rx, rz) = (f64::from(pos[0]) + 0.5, f64::from(pos[2]) + 0.5);
        // The raid tracks one participant (`bar.player`): the first player in
        // the level. Everyone else is heard by distance alone.
        let local_first = self.player.is_some();
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let dim = host.hub.config.dimension.clone();
        let spotted: Vec<(u32, [f64; 3], bool)> = host
            .hub
            .guests
            .iter()
            .map(|g| (g.conn, g.position, g.placed && g.dimension == dim))
            .collect();
        let neutral = horizon_client_core::sound_source::SOUND_SOURCES
            .iter()
            .position(|s| *s == "neutral")
            .unwrap_or(0) as i32;
        let mut first_guest = !local_first;
        for (conn, position, here) in spotted {
            if !here {
                continue;
            }
            let participating = std::mem::replace(&mut first_guest, false) && in_raid;
            let Some(at) = raid::horn_at(position, rx, rz, participating) else {
                continue;
            };
            let packet =
                host.hub
                    .sound_packet("minecraft:event.raid.horn", at, 64.0, 1.0, neutral, seed);
            host.hub.send_to(conn, &packet);
        }
    }
    /// `Raid.spawnGroup`: a wave of raiders at `pos`, the leader wearing the
    /// banner, ravagers carrying their riders.
    fn spawn_wave(&mut self, raid_id: i32, pos: [i32; 3], wave: i32, spawns: &[Spawn]) {
        let odds = self
            .raiding
            .raids
            .get(raid_id)
            .map_or(0.0, raid::Raid::enchant_odds);
        for sp in spawns {
            let Some(id) = self.spawn_raider(sp.kind, pos, raid_id, wave, sp.leader, odds) else {
                continue;
            };
            if sp.leader
                && let Some(r) = self.raiding.raids.get_mut(raid_id)
            {
                r.set_leader(wave, id);
            }
            if let Some(kind) = sp.rider
                && let Some(rider) = self.spawn_raider(kind, pos, raid_id, wave, false, odds)
            {
                self.mount_mob(rider, id);
            }
        }
        self.native_raid_spawn_wave(raid_id, pos, wave, spawns.len());
    }
    /// `Raid.joinRaid` for a new raider: placed at the block's centre one
    /// above, finalized for an event, its raid buffs applied.
    fn spawn_raider(
        &mut self,
        kind: RaiderType,
        pos: [i32; 3],
        raid_id: i32,
        wave: i32,
        leader: bool,
        odds: f32,
    ) -> Option<u32> {
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 1.0,
            f64::from(pos[2]) + 0.5,
        ];
        let site = super::creatures::SpawnSite::Finalizing(horizon_client::mobs::spawn_rules::Reason::Event);
        let id = self.ecs_spawn_mob(kind.entity(), at, leader, None, site)?;
        self.ecs.enlist_raider(id, raid_id, wave, leader);
        let buff = match kind {
            RaiderType::Pillager => Some(horizon_ecs_proto::raid_buffs::RaidBuff::Crossbow),
            RaiderType::Vindicator => Some(horizon_ecs_proto::raid_buffs::RaidBuff::Axe),
            _ => None,
        };
        if let Some(buff) = buff {
            let rules = horizon_ecs_proto::raid_buffs::RaidBuffRules {
                odds,
                easy_groups: raid::num_groups(1),
                normal_groups: raid::num_groups(2),
            };
            self.ecs.apply_raid_buffs(id, buff, wave, rules);
        }
        Some(id)
    }
    /// `EntityType.create` then `finalizeSpawn` for a patrol or a raid: the
    /// mob keeps the default rotation; a leader is set before finalizing so
    /// it wears the banner.
    pub(super) fn spawn_event_mob(
        &mut self,
        kind: &str,
        at: [f64; 3],
        leader: bool,
        reason: raider::Reason,
    ) -> Option<u32> {
        let spawn_reason = if reason == raider::Reason::Patrol {
            horizon_client::mobs::spawn_rules::Reason::Patrol
        } else {
            horizon_client::mobs::spawn_rules::Reason::Event
        };
        // The finalize can be cancelled by a mod (`ecs_with` refuses the kind): the ECS tells it as a missing entity.
        self.ecs_spawn_patrol_member(kind, at, leader, spawn_reason)
    }

    /// `Raider.die`: the wave loses its leader, the killing player becomes
    /// a hero of the village, and the raider leaves the raid (its health
    /// stays in the bar's total).
    pub(in crate::game) fn raider_died(
        &mut self,
        raid: i32,
        id: u32,
        by: Option<horizon_ecs_proto::targeting::Who>,
        wave: i32,
        leader: bool,
        health: f32,
    ) {
        let living = self.remaining_raider_health(raid, Some(id));
        let hero = match by {
            Some(horizon_ecs_proto::targeting::Who::Player(p)) => {
                self.with_player(p, |g| horizon_client::lan::playerdata::ints_uuid(g.acting_player_uuid()))
            }
            _ => None,
        };
        if let Some(r) = self.raiding.raids.get_mut(raid) {
            if leader {
                r.remove_leader(wave);
            }
            if let Some(uuid) = hero {
                r.add_hero(horizon_client::lan::playerdata::uuid_ints(uuid));
            }
            r.remove_from_raid(health, false, living);
            self.raiding.raids.dirty = true;
        }
    }
    /// `HostEvent::RaiderJoined`: a creature the ECS already holds joins raid `raid` (`Raid.joinRaid`): it counts toward the bar.
    /// `loaded`: it came back from a save (`readAdditionalSaveData`: no health recount, the wave's leader kept); a raid the host no longer
    /// holds drops it. A leader is set on its wave; the raid's `groups_spawned` was the wave it took.
    pub(in crate::game) fn ecs_raider_joined(&mut self, raid: i32, id: u32, wave: i32, leader: bool, loaded: bool) {
        let Some(health) = self.ecs.raiders_of(raid).iter().find(|r| r.id == id).map(|r| r.health) else {
            return;
        };
        let living = self.remaining_raider_health(raid, None);
        let Some(r) = self.raiding.raids.get_mut(raid) else {
            self.ecs.leave_raid(id);
            return;
        };
        r.add_wave_mob(health, !loaded, living);
        if leader {
            r.set_leader(wave, id);
        }
        self.raiding.raids.dirty = true;
    }
    /// `HostEvent::RaiderLeads`: a raider picked up the ominous banner (`Raider.pickUpItem`): `Raid.setLeader(wave, raider)`.
    pub(in crate::game) fn ecs_raider_leads(&mut self, raid: i32, id: u32, wave: i32) {
        if let Some(r) = self.raiding.raids.get_mut(raid) {
            r.set_leader(wave, id);
            self.raiding.raids.dirty = true;
        }
    }
    /// `HostEvent::RaiderHurt`: `Raider.hurtServer` recomputes the raid's bar before the damage, with the raider's health `before`.
    pub(in crate::game) fn ecs_raider_hurt(&mut self, raid: i32, id: u32, before: f32) {
        let living = self.remaining_raider_health(raid, Some(id)) + before;
        if let Some(r) = self.raiding.raids.get_mut(raid) {
            r.update_bossbar(living);
        }
    }

    /// What `getRaidAt` tells each villager where it stands (its brain's
    /// `SetRaidStatus`, `ResetRaidStatus`, `ReactToBell` and raid packages
    /// read it).
    /// RESTE-SANS-ECS: the villagers' raid status and the splash (entity event
    /// 42) have no ECS hook yet.
    fn sync_villager_raids(&mut self, _infos: &HashMap<i32, Vec<RaiderInfo>>) {}

    /// The raid bars a player `uuid` is in (`Raid.getPlayers`): red, notched
    /// in tens. The set is the one `updatePlayers` cached on the last 20-tick
    /// beat, so a viewer joins or leaves it up to 19 ticks late, alive or not
    /// (`validPlayer`'s `isAlive` is only read on a beat). The name is the
    /// translate component the client draws.
    pub(super) fn raid_bars_at(&self, uuid: u128) -> Vec<(i32, Tag, f32)> {
        if self.dim != Dim::Overworld {
            return Vec::new();
        }
        self.raiding
            .raids
            .ids()
            .into_iter()
            .filter_map(|id| self.raiding.raids.get(id).map(|raid| (id, raid)))
            .filter(|(_, raid)| {
                raid.bar.players.contains(&uuid) && raid.bar.visible && !raid.is_stopped()
            })
            .map(|(id, raid)| (id, raid_name_tag(raid.bar.name), raid.bar.progress))
            .collect()
    }
    /// A firework a villager launches to celebrate (`CelebrateVillagersSurvivedRaid`).
    pub(super) fn villager_firework(&mut self, at: [f64; 3], color: usize, flight: u8) {
        let mut stack = horizon_client::items::ItemStack::new("minecraft:firework_rocket", 1);
        stack.data_mut().fireworks = Some((
            flight,
            vec![horizon_client::particles::FireworkExplosion {
                shape: horizon_client::particles::FireworkShape::Burst,
                colors: vec![horizon_client::particles::DYE_FIREWORK_COLORS[color & 15].1],
                fade: Vec::new(),
                trail: false,
                twinkle: false,
            }],
        ));
        let rocket = self.new_rocket(Vec3::new(at[0], at[1], at[2]), &stack, false);
        self.push_rocket(rocket, &stack);
    }
    /// `BehaviorUtils.throwItem`: a hero's gift tossed toward the player.
    pub(super) fn throw_gift(&mut self, table: &str, from: [f64; 3], to: [f64; 3]) {
        let drops = self
            .res
            .items()
            .loot_table(table, false, None, &mut self.world.random);
        let d = Vec3::new(to[0] - from[0], to[1] - from[1], to[2] - from[2]).normalize();
        for stack in drops {
            self.spawn_item(stack, from, [d.x * 0.3, d.y * 0.3, d.z * 0.3], 10);
        }
    }
}

impl Game<'_> {
    /// Sections holding a claimed village point of interest (a job site,
    /// bed or bell a villager has taken): `PoiManager.isVillageCenter`.
    pub(super) fn village_centers(&self) -> HashSet<(i32, i32, i32)> {
        self.ecs.village_sections().into_iter().map(|[x, y, z]| (x, y, z)).collect()
    }
    /// `ServerLevel.isVillage`: within one section of a village.
    fn is_village(centers: &HashSet<(i32, i32, i32)>, pos: [i32; 3]) -> bool {
        horizon_gameplay::village_siege::is_village(centers, pos)
    }
    /// `VillageSiege.tryToSetupSiege`: the first player standing in a village
    /// picks a spot 32 blocks away (ten tries); true when one stood in a
    /// village, even if no spot was found.
    fn try_to_setup_siege(&mut self) -> bool {
        horizon_gameplay::village_siege::try_to_setup(self)
    }
    /// `VillageSiege.trySpawn`: one zombie, in a random yaw.
    fn try_spawn_siege_zombie(&mut self) {
        horizon_gameplay::village_siege::try_spawn(self);
    }
    /// `VillageSiege.findRandomSpawnPos`: ten tries within 8 blocks of the
    /// spot for a village column where a zombie may spawn.
    fn find_siege_spawn_pos(
        &mut self,
        centers: &HashSet<(i32, i32, i32)>,
        spot: [i32; 3],
    ) -> Option<[f64; 3]> {
        horizon_gameplay::village_siege::find_spawn_pos(self, centers, spot)
    }
    /// A raider's ominous banner (drop chance 2.0: always dropped) and a
    /// patrol captain's ominous bottle (`entities/pillager`).
    /// `OminousBottleAmplifier.onConsume`: Bad Omen for 100 minutes at the
    /// bottle's amplifier, then the dispose sound; the bottle is used up.
    pub(super) fn drink_ominous_bottle(
        &mut self,
        slot: usize,
        stack: &horizon_client::items::ItemStack,
    ) {
        let amplifier = stack.data().and_then(|d| d.ominous_amplifier).unwrap_or(0);
        let mut effect =
            horizon_client::effects::EffectInstance::new("bad_omen", 120_000, i32::from(amplifier));
        effect.visible = false;
        let new = self.potions.effects.get("bad_omen").is_none();
        self.potions.effects.add(effect);
        let eye = self.eye_position().unwrap_or_default();
        // MobEffect.onEffectAdded: Bad Omen's sound when it is new, at the
        // player's feet.
        if new && let Some(p) = self.player.as_ref() {
            let at = [p.position.x, p.position.y, p.position.z];
            self.audio
                .play("minecraft:event.mob_effect.bad_omen", Some(at), 1.0, 1.0);
        }
        self.audio
            .play("minecraft:item.ominous_bottle.dispose", Some(eye), 1.0, 1.0);
        if !self.creative
            && let Some(s) = self.stack_in_mut(slot).as_mut()
        {
            s.count -= 1;
            if s.count == 0 {
                *self.stack_in_mut(slot) = None;
            }
        }
    }
}

impl Game<'_> {
    /// Read-only command-harness observation of the existing raid state and inputs.
    pub(in crate::game) fn raid_fixture_state(
        &self,
        op: &serde_json::Value,
    ) -> horizon_client::Result<serde_json::Value> {
        use serde_json::json;
        let id = op["id"].as_i64().ok_or("raid observation: missing id")? as i32;
        let mut state = json!({"id":id,"present":self.raiding.raids.get(id).is_some()});
        if let Some(r) = self.raiding.raids.get(id) {
            state["status"] = json!(r.status.name());
            state["started"] = json!(r.started);
            state["active"] = json!(r.active);
            state["groups_spawned"] = json!(r.groups_spawned);
            state["center"] = json!(r.center);
            // Observe the production answer; never substitute a corrected POI test here.
            state["village"] = json!(Self::is_village(&self.village_centers(), r.center));
            if op["members"].as_bool() == Some(true) {
                state["member_present"] = json!(!self.ecs.raiders_of(id).is_empty());
            }
        }
        // RESTE-SANS-ECS: the actors' and the villagers' NBT observations read Mob data.
        let pos: [i32; 3] =
            std::array::from_fn(|i| op["poi"][i].as_i64().unwrap_or(i64::MIN) as i32);
        let loaded = self.world.loaded(pos[0], pos[2]);
        let live_poi = loaded.then(|| {
            let w = self.mob_world_base(&[]);
            format!(
                "{:?}",
                horizon_client::mobs::villager_brain::poi_at(&w, pos[0], pos[1], pos[2])
            )
        });
        eprintln!(
            "raid input raw: {}",
            json!({"label":op["label"],"id":id,"loaded":loaded,
            "live_poi":live_poi,"claimed_pois":self.village_pois(),
            "raid_nbt":horizon_client::snbt::to_snbt(&self.raiding.raids.to_nbt()),
            "actors":[],"villagers":[]})
        );
        Ok(state)
    }
}

impl SiegeLevel for Game<'_> {
    fn is_village(centers: &HashSet<(i32, i32, i32)>, pos: [i32; 3]) -> bool {
        Game::is_village(centers, pos)
    }
    fn siege(&mut self) -> &mut VillageSiege {
        &mut self.raiding.siege
    }
    fn try_to_setup_siege(&mut self) -> bool {
        Game::try_to_setup_siege(self)
    }
    fn try_spawn_siege_zombie(&mut self) {
        Game::try_spawn_siege_zombie(self);
    }
    fn find_siege_spawn_pos(
        &mut self,
        centers: &HashSet<(i32, i32, i32)>,
        spot: [i32; 3],
    ) -> Option<[f64; 3]> {
        Game::find_siege_spawn_pos(self, centers, spot)
    }
}

/// `Raid.raidEvent`'s name: the raid title, the remaining-raiders line, or
/// the victory and defeat lines.
fn raid_name_tag(name: BarName) -> Tag {
    let key = |k: &str| Tag::compound([("translate", Tag::String(k.into()))]);
    match name {
        BarName::Raid => key("event.minecraft.raid"),
        BarName::Victory => key("event.minecraft.raid.victory.full"),
        BarName::Defeat => key("event.minecraft.raid.defeat.full"),
        BarName::Remaining(n) => Tag::compound([
            ("translate", Tag::String("event.minecraft.raid".into())),
            (
                "extra",
                Tag::List(vec![
                    Tag::String(" - ".into()),
                    Tag::compound([
                        (
                            "translate",
                            Tag::String("event.minecraft.raid.raiders_remaining".into()),
                        ),
                        (
                            "with",
                            Tag::List(vec![Tag::String(n.to_string().as_str().into())]),
                        ),
                    ]),
                ]),
            ),
        ]),
    }
}



#[cfg(test)]
mod entity_ticking_tests {
    use super::{Lvl, RaidLevel};
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_client::{chunk::Chunk, ticket_levels::Sources};
    use std::{collections::HashSet, sync::Arc};

    #[test]
    #[ignore = "daily"]
    fn a_wave_spawns_only_where_entities_tick() {
        with_game(|game| {
            game.world
                .chunks
                .insert((0, 0), Arc::new(Chunk::empty(0, 0)));
            game.world
                .chunks
                .insert((6, 0), Arc::new(Chunk::empty(6, 0)));
            // A player in chunk (0, 0) with a simulation distance of 5: chunk
            // (6, 0) is loaded but its entities do not tick.
            game.tickets.update(Some(Sources {
                players: vec![(0, 0)],
                view_distance: 10,
                simulation_distance: 5,
                ..Sources::default()
            }));
            let centers = HashSet::new();
            let level = Lvl {
                game,
                difficulty: 2,
                centers: &centers,
            };
            assert!(level.is_position_entity_ticking([8, 64, 8]));
            assert!(!level.is_position_entity_ticking([6 * 16 + 8, 64, 8]));
            assert!(
                !level.is_position_entity_ticking([-40, 64, 8]),
                "not loaded"
            );
        });
    }
}
