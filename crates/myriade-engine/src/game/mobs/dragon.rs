//! The ender dragon fight on the host. The ECS holds the state (`EndFight`) and
//! the flight, phases, parts, crystals and respawn; the host keeps the save file,
//! the arena's load, the players' side of each hit and the reactions to the
//! fight's events (`on_fight_event`).

#[path = "dragon/view.rs"]
pub(in crate::game) mod view;
#[cfg(test)]
#[path = "dragon/thorns_tests.rs"]
mod thorns_tests;

use super::commands::Target;
use super::{Game, creatures::{Finalized, SpawnSite}};
use myriade_engine::health::HurtSource;
use myriade_engine::mobs::{Damage, DamageKind};
use myriade_engine::random_source;

/// `#minecraft:is_fire` by the damage kind (the dragon's parts and crystals refuse it).
fn is_fire(kind: DamageKind) -> bool {
    Damage { kind, attacker: None, from: None, attacker_species: None }.is_fire()
}
/// The ECS's `Cause` of a damage kind (`bridge/host.rs` maps the same causes from a `HurtSource`); a kind with no cause of its own is magic.
fn cause_of(kind: DamageKind) -> Cause {
    match kind {
        DamageKind::Fall => Cause::Fall,
        DamageKind::Explosion => Cause::Blast,
        DamageKind::InFire => Cause::InFire,
        DamageKind::OnFire => Cause::OnFire,
        DamageKind::Lava => Cause::Lava,
        DamageKind::Drown => Cause::Drown,
        DamageKind::Campfire => Cause::Campfire,
        DamageKind::PlayerAttack | DamageKind::MobAttack | DamageKind::MaceSmash | DamageKind::Spear | DamageKind::MobAttackNoAggro | DamageKind::Arrow | DamageKind::Thrown | DamageKind::Fireball => Cause::Entity,
        _ => Cause::Magic,
    }
}
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::random::RandomSource;
use horizon_core::nbt::Tag;
use horizon_ecs_proto::cow::Cause;
use horizon_ecs_proto::dragon::fight::{FightSave, RespawnStage};
use horizon_ecs_proto::dragon::hurt::{Blow, Hurt};
use horizon_ecs_proto::host_out::FightEvent;
use horizon_ecs_proto::rules::dragon_data::{BODY, MAX_HEALTH};
use horizon_ecs_proto::targeting::Who;

/// The respawn stage's name in the save file (vanilla's `respawn_stage`).
fn respawn_name(r: RespawnStage) -> &'static str {
    match r {
        RespawnStage::Start => "start",
        RespawnStage::Preparing => "preparing_to_summon_pillars",
        RespawnStage::Pillars => "summoning_pillars",
        RespawnStage::Summoning => "summoning_dragon",
    }
}
fn respawn_from_name(name: &str) -> Option<RespawnStage> {
    [RespawnStage::Start, RespawnStage::Preparing, RespawnStage::Pillars, RespawnStage::Summoning]
        .into_iter()
        .find(|r| respawn_name(*r) == name)
}

const FIGHT_FILE: &str = "data/minecraft/ender_dragon_fight.dat";

impl Game<'_> {
    fn fight_path(&self) -> std::path::PathBuf {
        myriade_engine::save::dimension_dir(&self.world_root, "minecraft:the_end").join(FIGHT_FILE)
    }
    /// The saved fight, read once. Without a file, a world with an active portal
    /// already won the fight.
    fn load_fight(&mut self) {
        if self.ecs.fight_view().loaded {
            return;
        }
        let tag = myriade_engine::save::read_dat(&self.fight_path());
        let data = tag.as_ref().and_then(|t| t.get("data"));
        let flag = |k: &str| matches!(data.and_then(|d| d.get(k)), Some(Tag::Byte(v)) if *v != 0);
        let won = tag.is_none() && self.end_portal_active();
        let saved = FightSave {
            killed: won || flag("dragon_killed"),
            previously_killed: won || flag("previously_killed"),
            crystals_placed: won || flag("horizon_crystals_placed"),
            needs_scan: match data.and_then(|d| d.get("needs_state_scanning")) {
                Some(Tag::Byte(v)) => *v != 0,
                _ => true,
            },
            dragon_uuid: match data.and_then(|d| d.get("dragon_uuid")) {
                Some(Tag::IntArray(v)) if v.len() == 4 => {
                    Some(v.iter().fold(0u128, |a, &x| (a << 32) | u128::from(x as u32)))
                }
                _ => None,
            },
            exit_portal: match data.and_then(|d| d.get("exit_portal_location")) {
                Some(Tag::IntArray(v)) if v.len() == 3 => Some([v[0], v[1], v[2]]),
                _ => None,
            },
            gateways: match data.and_then(|d| d.get("gateways")) {
                Some(Tag::IntArray(v)) => v.clone(),
                _ => Vec::new(),
            },
            respawn: match data.and_then(|d| d.get("respawn_stage")) {
                Some(Tag::String(name)) => respawn_from_name(&name.to_string().unwrap_or_default()).map(|r| {
                    let time = match data.and_then(|d| d.get("respawn_time")) {
                        Some(Tag::Int(t)) => *t as u32,
                        _ => 0,
                    };
                    (r, time)
                }),
                _ => None,
            },
            respawn_crystals: match data.and_then(|d| d.get("horizon_respawn_crystals")) {
                Some(Tag::IntArray(v)) => v
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|b| [f64::from(b[0]) + 0.5, f64::from(b[1]), f64::from(b[2]) + 0.5])
                    .collect(),
                _ => Vec::new(),
            },
        };
        let seed = self.level.seed;
        self.ecs_with(|ecs, mw| ecs.fight_load(mw, seed, saved));
    }
    fn end_portal_active(&self) -> bool {
        (0..256).any(|y| {
            &*self.res.facts().get(self.world.block(0, y, 0)).name == "minecraft:bedrock"
                && (-3..=3).any(|d| {
                    &*self.res.facts().get(self.world.block(d, y, 0)).name == "minecraft:end_portal"
                })
        })
    }
    /// Writes the fight's state as the vanilla file, from the ECS.
    pub(super) fn save_fight(&mut self) {
        if !self.ecs.fight_view().loaded {
            return;
        }
        let f = self.ecs.ecs().map(|w| w.fight_save()).unwrap_or_default();
        let b = |v: bool| Tag::Byte(i8::from(v));
        let mut data = vec![
            ("needs_state_scanning", b(f.needs_scan)),
            ("dragon_killed", b(f.killed)),
            ("previously_killed", b(f.previously_killed)),
            ("respawn_time", Tag::Int(f.respawn.map_or(0, |r| r.1 as i32))),
            ("gateways", Tag::IntArray(f.gateways.clone())),
            ("horizon_crystals_placed", b(f.crystals_placed)),
        ];
        if let Some(p) = f.exit_portal {
            data.push(("exit_portal_location", Tag::IntArray(p.to_vec())));
        }
        if let Some(u) = f.dragon_uuid {
            data.push(("dragon_uuid", Tag::IntArray((0..4).rev().map(|i| (u >> (32 * i)) as u32 as i32).collect())));
        }
        if let Some((stage, _)) = f.respawn {
            data.push(("respawn_stage", Tag::String(respawn_name(stage).into())));
            data.push((
                "horizon_respawn_crystals",
                Tag::IntArray(
                    f.respawn_crystals
                        .iter()
                        .flat_map(|p| [p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32])
                        .collect(),
                ),
            ));
        }
        let tag = Tag::compound([
            ("data", Tag::compound(data)),
            ("DataVersion", Tag::Int(horizon_world::DATA_VERSION)),
        ]);
        let _ = myriade_engine::save::write_dat(&self.fight_path(), tag);
    }
    /// The island's centre chunks are lit (the arena is "loaded").
    fn arena_loaded(&self) -> bool {
        (-2..=1).all(|cz| (-2..=1).all(|cx| self.streaming.is_lit(cx, cz)))
    }
    /// The player acting now: the guest whose turn it is, else the host.
    pub(super) fn acting_player(&self) -> Target {
        self.lan_acting.map_or(Target::Player, Target::Guest)
    }
    /// `EnderDragonFight.tick` while the End is ticked; the ECS runs the fight
    /// from its arena's state (`arena_ready`).
    pub(super) fn tick_fight(&mut self) {
        if !self.dimension_type().has_ender_dragon_fight {
            return;
        }
        self.load_fight();
        let arena_ready = self.arena_loaded();
        self.ecs_with(|ecs, mw| {
            if let Some(w) = ecs.ecs_mut() {
                w.fight_tick(mw, arena_ready);
            }
        });
    }
    /// `EndCrystal.tick`'s fire is the ECS's, set on the End's fight by `ecs_end_fire`.
    pub(super) fn ecs_end_fire(&mut self) {
        let on = self.dimension_type().has_ender_dragon_fight;
        self.ecs.set_end_fire(on);
    }
    /// The player behind a command's damage source (`source.getEntity() instanceof Player`).
    fn command_player(&self, cause: Option<Target>) -> Option<Target> {
        match cause {
            Some(Target::Player) => Some(self.acting_player()),
            Some(Target::Guest(conn)) => Some(Target::Guest(conn)),
            _ => None,
        }
    }
    /// `DamageCommand` on the dragon: the mod hooks decide first, then `EnderDragon.hurtServer`
    /// through the ECS (`hurt_dragon`, a quarter of the damage plus at most one, on the body).
    pub(in crate::game) fn command_damage_dragon(
        &mut self,
        id: u32,
        amount: f32,
        ty: &str,
        kind: DamageKind,
        (by, cause, from): (Option<Target>, Option<Target>, Option<Vec3>),
    ) -> bool {
        let player = self.command_player(cause);
        let creative = self.creative_source_of(cause.or(by));
        let saved = self.saved_refuses_damage(id, ty, creative);
        let vanilla = saved || is_fire(kind);
        let by = player.map(|p| self.player_entity_id(p));
        let causing = by.map_or(0, |id| id as i32);
        let source = myriade_engine::health::HurtSource {
            damage_type: ty,
            causing,
            direct: causing,
            position: from.map(|p| [p.x, p.y, p.z]),
        };
        let level = super::entity::damage_level_of!(self, scoped);
        if level.invulnerability_check(myriade_engine::health::Hurting::Dragon(id as i32, source), vanilla) {
            return false;
        }
        let blow = horizon_ecs_proto::dragon::hurt::Blow {
            part: 2,
            damage: amount,
            cause: cause_of(kind),
            by: by.map(Who::Player),
            arrow: false,
            from: from.map(|p| (p.x, p.z)),
        };
        self.ecs_with(|ecs, mw| ecs.hurt_dragon(mw, id, blow)) != Hurt::Refused
    }
    /// `DamageCommand` on an End crystal: the mod hooks, then `hit_crystal`.
    pub(super) fn command_damage_crystal(
        &mut self,
        id: u32,
        ty: &str,
        kind: DamageKind,
        direct: Option<Target>,
        cause: Option<Target>,
        position: Option<[f64; 3]>,
    ) -> bool {
        if is_fire(kind) {
            return false;
        }
        if matches!(cause, Some(Target::Dragon(_))) {
            return false;
        }
        let by = self.command_player(cause).map(|p| self.player_entity_id(p));
        self.ecs_with(|ecs, mw| ecs.destroy_crystal(mw, id, by));
        true
    }
    /// The network id of a player behind a command or a melee hit (`Target::Player` is the host's own).
    fn player_entity_id(&self, target: Target) -> u32 {
        match target {
            Target::Player => self.own_entity_id() as u32,
            Target::Guest(conn) => self.command_guest(conn).map_or(0, |g| g.entity_id as u32),
            _ => 0,
        }
    }
    /// `Player.attack` on a dragon part or crystal in reach: a crystal takes `hit_crystal`, a part of the dragon `hurt_dragon` on
    /// that part (part `i` is the id `dragon + 1 + i`). True when the blow landed.
    pub(in crate::game) fn melee_special(&mut self, id: u32, amount: f32, kind: DamageKind) -> bool {
        let by = self.player_entity_id(self.acting_player());
        let fire = is_fire(kind);
        let creative = self.creative;
        if self.ecs.crystal_views().iter().any(|(c, _)| *c == id) {
            return self.ecs_with(|ecs, mw| ecs.hit_crystal(mw, id, Some(by), fire, creative));
        }
        let Some((dragon, part)) = self.dragon_views().into_iter().find_map(|(d, _)| {
            let part = id.checked_sub(d + 1).filter(|p| *p < 8)?;
            Some((d, part as usize))
        }) else {
            return false;
        };
        let blow = Blow {
            part,
            damage: amount,
            cause: cause_of(kind),
            by: Some(Who::Player(by)),
            arrow: false,
            from: None,
        };
        matches!(self.ecs_with(|ecs, mw| ecs.hurt_dragon(mw, dragon, blow)), Hurt::Landed { .. })
    }
    /// The dragon part under the crosshair within reach: its id and distance.
    pub(super) fn dragon_hit(&self) -> Option<(u32, f64)> {
        let (eye, dir) = self.aim()?;
        let reach: f64 = self.entity_reach;
        let mut best: Option<(u32, f64)> = None;
        for (id, b) in self.special_targets() {
            let (lo, hi) = (b.min_position(), b.max_position());
            let (lo, hi) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
            let (mut t0, mut t1) = (0.0f64, reach);
            let mut hit = true;
            for a in 0..3 {
                if dir[a].abs() < 1e-9 {
                    hit &= eye[a] >= lo[a] && eye[a] <= hi[a];
                    continue;
                }
                let (mut ta, mut tb) = ((lo[a] - eye[a]) / dir[a], (hi[a] - eye[a]) / dir[a]);
                if ta > tb {
                    std::mem::swap(&mut ta, &mut tb);
                }
                t0 = t0.max(ta);
                t1 = t1.min(tb);
            }
            if hit && t0 <= t1 && best.is_none_or(|b| t0 < b.1) {
                best = Some((id, t0));
            }
        }
        best
    }
    /// The targets for projectiles and melee: the dragon's boxes, then the crystals' (`crystal_boxes`).
    pub(super) fn special_targets(&self) -> Vec<(u32, Aabb)> {
        let mut out = Vec::new();
        for (id, _) in self.dragon_views() {
            for (part, b) in self.ecs.dragon_boxes(id) {
                out.push((id + 1 + part as u32, b));
            }
        }
        out
    }
    /// The box of a special the fight tracks by network id: a dragon's part, else an end crystal (2 by 2 by 2 from its feet).
    pub(in crate::game) fn tracked_special_box(&self, id: i32) -> Option<Aabb> {
        let id = u32::try_from(id).ok().filter(|&id| id != 0)?;
        if let Some((_, b)) = self.special_targets().into_iter().find(|(s, _)| *s == id) {
            return Some(b);
        }
        let (_, c) = self.ecs.crystal_views().into_iter().find(|(cid, _)| *cid == id)?;
        let [x, y, z] = c.position;
        Some(Aabb::new(Vec3::new(x - 1.0, y, z - 1.0), Vec3::new(x + 1.0, y + 2.0, z + 1.0)))
    }
    /// `EnderDragonFight.dragonEvent`'s progress: the fight's dragon health over its max, for a viewer within 192 blocks of the
    /// island's centre while the fight runs (`dragon_boss_progress`).
    pub(in crate::game) fn dragon_boss_progress(&self, at: [f64; 3], alive: bool) -> Option<f32> {
        let view = self.ecs.fight_view();
        if !self.dimension_type().has_ender_dragon_fight || !alive || !view.loaded || view.killed {
            return None;
        }
        let near = at[0].powi(2) + (at[1] - 128.0).powi(2) + at[2].powi(2) <= 192.0 * 192.0;
        if !near {
            return None;
        }
        let health = self.dragon_views().into_iter().find(|(_, v)| v.fight).map_or(MAX_HEALTH, |(_, v)| v.health);
        Some((health / MAX_HEALTH).clamp(0.0, 1.0))
    }
    /// `EnderDragonFight.dragonEvent`'s progress is the boss bar's (`HostEvent::BossBar`).
    pub(super) fn fight_ready(&mut self) -> bool {
        self.load_fight();
        // The players' scan (`fight_view().players`) is not in the FightView yet: loaded is the condition here.
        self.ecs.fight_view().loaded
    }
    pub(super) fn exit_portal(&self) -> Option<[i32; 3]> {
        self.ecs.fight_view().exit_portal
    }
    pub(super) fn set_exit_portal(&mut self, origin: [i32; 3]) {
        self.ecs_with(|ecs, _| ecs.set_exit_portal(origin));
    }
    /// `explosion_hits_special` is the ECS's `blast_dragon` for the dragon and
    /// `destroy_crystal` for each crystal in reach.
    pub(super) fn explosion_hits_special(&mut self, center: [f64; 3], radius: f32, blocks: &[[i32; 3]], id: u32) {
        // The exposures are read before the ECS is lent out: the closure cannot borrow `self`.
        let exposures: Vec<(Aabb, f32)> = self
            .ecs
            .dragon_boxes(id)
            .into_iter()
            .map(|(_, b)| {
                let (lo, hi) = (b.min_position(), b.max_position());
                (b, self.seen_percent(center, [lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]))
            })
            .collect();
        let exposure = |b: Aabb| exposures.iter().find(|(x, _)| *x == b).map_or(0.0, |(_, e)| *e);
        self.ecs_with(|ecs, mw| ecs.blast_dragon(mw, id, center, radius, &exposure, None));
    }
    /// `EndCrystalItem.useOn`'s checks: obsidian or bedrock clicked, air above it.
    /// The entity check of the 1x2x1 room is still missing (no ECS query of the boxes yet).
    fn end_crystal_fits(&self, hit: &super::Hit) -> bool {
        let name = &self.res.facts().get(self.world.block(hit.pos[0], hit.pos[1], hit.pos[2])).name;
        if !matches!(&**name, "minecraft:obsidian" | "minecraft:bedrock") {
            return false;
        }
        let above = [hit.pos[0], hit.pos[1] + 1, hit.pos[2]];
        self.res.facts().is_air(self.world.block(above[0], above[1], above[2]))
    }
    /// `EndCrystalItem.useOn`: on obsidian or bedrock with room above, the ECS places it.
    pub(super) fn place_end_crystal(&mut self, hit: &super::Hit, stack: &myriade_engine::items::ItemStack) -> bool {
        if &*stack.item != "minecraft:end_crystal" || !self.end_crystal_fits(hit) {
            return false;
        }
        let above = [hit.pos[0], hit.pos[1] + 1, hit.pos[2]];
        let at = [f64::from(above[0]) + 0.5, f64::from(above[1]), f64::from(above[2]) + 0.5];
        let show_bottom = !self.dimension_type().has_ender_dragon_fight;
        let id = self.take_entity_id();
        self.ecs_with(|ecs, mw| ecs.place_crystal(mw, at, show_bottom, &mut || id));
        self.post_block_game_event("minecraft:entity_place", above, self.sculk_actor(), None);
        self.world.modified.insert((above[0] >> 4, above[2] >> 4));
        if !self.creative {
            self.consume_held();
        }
        self.award_stat("used", &stack.item, 1);
        self.swing_arm();
        true
    }
    /// `EndCrystal.kill` (`/kill`): removed without a blast; the ECS sees it destroyed by no one.
    pub(super) fn kill_crystal(&mut self, id: u32) {
        self.ecs_with(|ecs, mw| ecs.destroy_crystal(mw, id, None));
        self.entity_killed_event([0.0, 0.0, 0.0], false);
    }
    /// The host's side of the fight's events, drained from `HostEvent::Fight`.
    pub(super) fn on_fight_event(&mut self, e: FightEvent) {
        match e {
            // The old host gave a crystal's destruction no stat or advancement: the strafe
            // it reset is the ECS's now, so nothing is left here.
            FightEvent::CrystalDestroyed { .. } => {}
            // The arena's ticket is read each tick from the fight's `players` (game.rs, `dragon:`).
            FightEvent::PlayersChanged { .. } => {}
            FightEvent::Killed { by: _, last_blow: _, silent } => {
                if !silent {
                    self.entity_killed_event([0.0, 128.0, 0.0], false);
                }
                self.catalyst_takes_experience([0.0, 128.0, 0.0], |_| 12000);
            }
            FightEvent::Credit { by, last_blow: _, by_player } => {
                self.award_dragon_kill(by, by_player);
            }
            FightEvent::BarGone => self.bosses.remove_bar("minecraft:ender_dragon"),
            FightEvent::Dirty => self.save_fight(),
        }
    }
    /// `ServerPlayer.awardKillScore` for the dragon, on the killer's state (`credit_dragon_kill`'s
    /// stat, score and advancement; the blow's view is the ECS's `Cause`, not rebuilt here).
    fn award_dragon_kill(&mut self, by: u32, by_player: bool) {
        let _ = by_player;
        // The dragon's position is not in the Credit event: the fight's centre, as its other events.
        let dragon = myriade_engine::advancements::EntityView {
            kind: "minecraft:ender_dragon".into(),
            position: [0.0, 128.0, 0.0],
            ..Default::default()
        };
        self.ecs_award_kill(by, &dragon, None);
    }
    /// The victims of a breath cloud this tick (`HostEvent::CloudHit`).
    pub(in crate::game) fn ecs_cloud_hit(&mut self, at: [f64; 3], amount: f32, amplifier: u32, players: Vec<u32>, mobs: Vec<u32>) {
        for p in players {
            let guest = self.lan.as_ref().and_then(|h| h.hub.guests.iter().find(|g| g.entity_id as u32 == p).map(|g| g.conn));
            let who = match guest {
                Some(conn) => Target::Guest(conn),
                None if self.player.as_ref().is_some_and(|me| me.entity_id as u32 == p) => Target::Player,
                None => continue,
            };
            match who {
                Target::Guest(conn) => {
                    self.as_guest(conn, |g| g.hurt_player_by("minecraft:indirect_magic", amount));
                }
                _ => self.hurt_player_by("minecraft:indirect_magic", amount),
            }
        }
        let _ = (at, amplifier, mobs);
    }
    /// Spawns a dragon as `EntityType.create` or the `/summon` command does, through the ECS.
    pub(super) fn spawn_dragon_spawned(&mut self, at: [f64; 3], site: SpawnSite) -> Option<u32> {
        if !self.dragon_views().is_empty() {
            return None;
        }
        let yaw = match site {
            SpawnSite::Finalizing(myriade_engine::mobs::spawn_rules::Reason::Command) => 0.0,
            _ => myriade_types::wrap_degrees(self.world.random.next_float() * 360.0),
        };
        let sheet = horizon_ecs_proto::spawn_sheet::SpawnSheet::default();
        let finalized = match site {
            SpawnSite::Finalizing(_) => self.finalize_spawn_at(at).0,
            SpawnSite::Plain | SpawnSite::Raw => Finalized::Done,
        };
        match finalized {
            Finalized::Cancelled => return None,
            Finalized::Skipped => {}
            Finalized::Done => {
                let _ = self.world.random.triangle(0.0, 0.114_850_000_000_000_01);
                let _ = self.world.random.next_float();
            }
        }
        let seed = random_source::unique_seed();
        let id = self.take_entity_id();
        self.next_entity += 8;
        let summoned = self.ecs_with(|ecs, mw| {
            ecs.summon_dragon(mw, at, yaw, seed, horizon_ecs_proto::species::ender_dragon::START_PHASE, &sheet, &mut || id)
        });
        Some(summoned)
    }
    /// `EnderDragon.hurtServer`: Thorns hits the body part, so damage is quartered plus at
    /// most one, with the player as its causing entity. The roll is the Thorns' own.
    pub(super) fn dragon_thorns(&mut self, dragon: u32, min: f32, max: f32) {
        let damage = random_source::entity().next_float() * (max - min) + min;
        let by = self.own_entity_id() as u32;
        let blow = Blow { part: BODY, damage, cause: Cause::Entity, by: Some(Who::Player(by)), arrow: false, from: None };
        self.ecs_with(|ecs, mw| ecs.hurt_dragon(mw, dragon, blow));
    }
    /// `Mob.playAmbientSound` when EntityType.spawn places a dragon.
    pub(super) fn dragon_spawn_ambient(&mut self, id: u32) -> bool {
        let Some(at) = self.dragon_views().into_iter().find(|(d, _)| *d == id).map(|(_, v)| v.position) else {
            return false;
        };
        let pitch = (self.world.random.next_float() - self.world.random.next_float()) * 0.2 + 1.0;
        self.audio.play("minecraft:entity.ender_dragon.ambient", Some(at), 5.0, pitch);
        true
    }
}

impl Game<'_> {
    /// The dragon fireball's burst (level event 2006): its explosion sound when `data` is 1. The particles are the clients' own.
    pub(in crate::game) fn dragon_fireball_burst(&mut self, pos: horizon_core::position::BlockPos, data: i32) {
        if data == 1 {
            let at = [pos.x, pos.y, pos.z].map(|v| f64::from(v) + 0.5);
            let pitch = self.world.random.next_float() * 0.1 + 0.9;
            self.audio.play("minecraft:entity.dragon_fireball.explode", Some(at), 1.0, pitch);
        }
    }
}
