//! Wind charges: the item a player throws (`WindChargeItem`), the charges
//! breezes shoot, and the burst they make (`ServerExplosion` with
//! `TRIGGER_BLOCK`: no damage, only knockback and the blocks that react).
use super::Game;
use super::entity::{Blast, Entity as _, for_each_entity_mut};
use myriade_engine::advancements::EntityView;
use myriade_engine::join_seq::Joined as _;
use myriade_engine::{
    Result,
    items::ItemStack,
    mobs::Who,
    projectiles::{Projectile, ProjectileKind},
};
use horizon_core::{
    geometry::{Direction, Vec3},
    position::BlockPos,
    random::RandomSource,
};
use horizon_world::level::PlayerContext;
use std::collections::BTreeSet;

/// A burst `wind_burst` puts off until a guest's turn is over.
pub(super) struct Burst {
    center: [f64; 3],
    radius: f32,
    multiplier: f32,
    src: Source,
}
/// The burst's sound: the breeze's own for its charges and for a Wind Charged
/// mob's death, the player's charge's for a wind charge and a mace's Wind Burst.
const BREEZE_BURST: &str = "minecraft:entity.breeze.wind_burst";
const WIND_CHARGE_BURST: &str = "minecraft:entity.wind_charge.wind_burst";

/// What set a burst off, as much of `ServerExplosion`'s source as the burst
/// reads: the sound `ServerLevel.explode` was given, whether the source is a
/// breeze's charge (`canTriggerBlocks` needs mob griefing for that one alone),
/// whether it is a wind charge (`ServerPlayer.onExplosionHit`'s fall), whether
/// a mob is the indirect source (`VehicleEntity.ignoreExplosion`), and whether
/// the source is silent (`ServerLevel.explode`'s `playSound`).
#[derive(Clone, Copy)]
pub(super) struct Source {
    sound: &'static str,
    breeze: bool,
    wind_charge: bool,
    mob: bool,
    silent: bool,
    pub(super) cause: BurstCause,
}
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct BurstCause {
    pub source: Option<u32>,
    pub owner: Option<u32>,
}

impl Source {
    pub(super) fn with_cause(self, cause: BurstCause) -> Self {
        Self { cause, ..self }
    }
    /// `WindChargedMobEffect.onMobRemoved`: the dying mob is the source, so it
    /// makes the breeze's sound while its blocks react whatever mob griefing
    /// says and vehicles stay put when it is off; a `Silent` mob's is mute.
    pub(super) fn wind_charged(player: bool, silent: bool) -> Self {
        Self {
            sound: BREEZE_BURST,
            breeze: false,
            wind_charge: false,
            mob: !player,
            silent,
            cause: BurstCause {
                source: None,
                owner: None,
            },
        }
    }
    /// The `minecraft:explode` of the Wind Burst enchantment: the attacker is
    /// the source, so it makes the wind charge's sound and its blocks react.
    pub(super) const WIND_BURST: Self = Self {
        sound: WIND_CHARGE_BURST,
        breeze: false,
        wind_charge: false,
        mob: false,
        silent: false,
        cause: BurstCause {
            source: None,
            owner: None,
        },
    };
}
/// Whether a wind push must wait for the entity's own tick: it joined before
/// the charge (`id` below the charge's), so vanilla has ticked it already this
/// game tick, and its kind ticks after the projectiles here.
fn push_waits_for_tick<T: PartialOrd>(
    ticks_after_projectiles: bool,
    joined: T,
    charge: Option<T>,
) -> bool {
    ticks_after_projectiles && charge.is_some_and(|c| joined < c)
}
/// Whether a block reacts to a wind charge's burst by being used.
fn triggers(name: &str) -> bool {
    let n = name.trim_start_matches("minecraft:");
    // Doors and trapdoors open unless iron; gates, buttons, levers and bells react.
    (n.ends_with("_door") || n.ends_with("_trapdoor")) && !n.starts_with("iron_")
        || n.ends_with("_fence_gate")
        || n.ends_with("_button")
        || n == "lever"
        || n == "bell"
}
impl Game<'_> {
    /// `Player.itemAttackInteraction` applies trigger-only held-item effects
    /// after an EnderDragonPart is successfully hit.
    pub(super) fn post_attack_dragon_burst(&mut self) {
        if let Some(held) = self.held().cloned() {
            self.post_attack_bursts(&held, "minecraft:ender_dragon");
        }
    }

    /// `ServerExplosion.hurtEntities` knockback for a trigger-only blast such
    /// as Wind Burst. The dragon is not in the normal entity lists, but its
    /// body still receives the blast impulse (without taking damage).
    pub(super) fn wind_burst_knockback(
        &mut self,
        center: [f64; 3],
        radius: f32,
        multiplier: f32,
        blocks: &[[i32; 3]],
    ) {
        let Some((id, _)) = self.dragon_views().into_iter().next() else {
            return;
        };
        let whole = self.ecs.dragon_boxes(id).into_iter().map(|(_, b)| b).reduce(|a, b| a.union_bounds(b));
        let Some(whole) = whole else {
            return;
        };
        let (lo, hi) = (whole.min_position(), whole.max_position());
        let exposure = self.seen_percent(center, [lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
        let Some(mut push) = self.ecs_with(|ecs, mw| ecs.dragon_push(mw, id, center, radius, &|_| exposure)) else {
            return;
        };
        for component in &mut push {
            *component *= f64::from(multiplier);
        }
        self.ecs.push(id, push);
    }

    /// `WindChargeItem.use`.
    pub(super) fn throw_wind_charge(&mut self, stack: &ItemStack) -> Result<bool> {
        let (Some(eye), Some(p)) = (self.eye_position(), self.player.as_ref()) else {
            return Ok(false);
        };
        let (on_ground, pos) = (p.ground.on_ground, p.position);
        let velocity = self.known_player_movement();
        let start = Vec3::new(pos.x, eye[1], pos.z);
        let mut proj = Projectile::new(ProjectileKind::WindCharge, start, Some(Who::Player));
        proj.shoot_from_rotation(self.pitch, self.yaw, 0.0, 1.5, 1.0, velocity, on_ground);
        self.spawn_projectile(proj, stack);
        let pitch = 0.4 / (self.world.random.next_float() * 0.4 + 0.8);
        self.audio
            .play("minecraft:entity.wind_charge.throw", Some(eye), 0.5, pitch);
        self.award_stat("used", "minecraft:wind_charge", 1);
        self.swing_arm();
        if !self.creative
            && let Some(s) = self.held_mut().as_mut()
        {
            s.count -= 1;
            if s.count == 0 {
                *self.held_mut() = None;
            }
        }
        Ok(true)
    }
    /// The exploded positions: rays through everything but the blocks that
    /// stop wind (`blocks_wind_charge_explosions`).
    fn wind_positions(&mut self, center: [f64; 3], radius: f32) -> Vec<[i32; 3]> {
        let stops = self
            .res
            .items()
            .block_tags
            .get("minecraft:blocks_wind_charge_explosions")
            .cloned()
            .unwrap_or_default();
        let (min_y, max_y) = (self.world.bottom_y(), self.world.top_y());
        let mut set: BTreeSet<[i32; 3]> = BTreeSet::new();
        for xx in 0..16 {
            for yy in 0..16 {
                for zz in 0..16 {
                    if !(xx == 0 || xx == 15 || yy == 0 || yy == 15 || zz == 0 || zz == 15) {
                        continue;
                    }
                    let mut d = [
                        f64::from(xx as f32 / 15.0 * 2.0 - 1.0),
                        f64::from(yy as f32 / 15.0 * 2.0 - 1.0),
                        f64::from(zz as f32 / 15.0 * 2.0 - 1.0),
                    ];
                    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
                    for c in &mut d {
                        *c /= len;
                    }
                    let mut power = radius * (0.7 + self.world.random.next_float() * 0.6);
                    let mut at = center;
                    while power > 0.0 {
                        let pos = [
                            at[0].floor() as i32,
                            at[1].floor() as i32,
                            at[2].floor() as i32,
                        ];
                        if pos[1] < min_y || pos[1] >= max_y {
                            break;
                        }
                        let name = &self
                            .res
                            .facts()
                            .get(self.world.block(pos[0], pos[1], pos[2]))
                            .name;
                        if stops.contains(&**name) {
                            power -= (3_600_000.0_f32 + 0.3) * 0.3;
                        }
                        if power > 0.0 {
                            set.insert(pos);
                        }
                        for i in 0..3 {
                            at[i] += d[i] * f64::from(0.3_f32);
                        }
                        power -= 0.225_000_01;
                    }
                }
            }
        }
        set.into_iter().collect()
    }
    /// `ServerExplosion.hurtEntities` for a calculator that damages nothing:
    /// knockback scaled by distance, exposure and the multiplier. Returns the
    /// guests it pushed and by how much (`hitPlayers`), which their clients
    /// apply from the burst's packet.
    fn wind_knockback(
        &mut self,
        center: [f64; 3],
        radius: f32,
        multiplier: f32,
        src: Source,
        blocks: &[[i32; 3]],
    ) -> Vec<(u32, [f64; 3])> {
        // ServerExplosion.shouldAffectBlocklikeEntities
        let blocklike = !src.breeze
            && !src.wind_charge
            && self
                .session
                .rules
                .bool_at(myriade_engine::gamerules::vanilla::MOB_GRIEFING);
        let diameter = f64::from(radius * 2.0);
        let distance = |p: [f64; 3]| {
            ((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2) + (p[2] - center[2]).powi(2))
                .sqrt()
        };
        let direction = |origin: [f64; 3]| {
            let d = [
                origin[0] - center[0],
                origin[1] - center[1],
                origin[2] - center[2],
            ];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if l < 1.0e-5 {
                [0.0; 3]
            } else {
                [d[0] / l, d[1] / l, d[2] / l]
            }
        };
        // The player, then every other player in the level (LAN guests),
        // each swapped in as the player. In a guest's turn (a mace's wind
        // burst) the player is that guest.
        let own = self.wind_push_player(center, radius, multiplier, src, blocks);
        let mut pushed: Vec<_> = self.lan_acting.zip(own).into_iter().collect();
        if let Some(host) = self.lan.as_ref() {
            let guests: Vec<u32> = host
                .hub
                .guests
                .iter()
                .filter(|g| host.hub.here(g) && g.loaded && !g.dead)
                .map(|g| g.conn)
                .collect();
            for conn in guests {
                let push = self.as_guest(conn, |g| {
                    g.wind_push_player(center, radius, multiplier, src, blocks)
                });
                if let Some(Some(push)) = push {
                    pushed.push((conn, push));
                }
            }
        }
        let multiplier = f64::from(multiplier);
        // Every other entity takes `Entity.pushFromExplosion` alike: its
        // distance is from its feet, its direction from its eyes. A kind sits
        // out where its `ignoreExplosion` says (items and visible stands for a
        // charge, vehicles for a mob's burst with mob griefing off).
        let burst = Blast {
            vehicles_ignore: src.mob
                && !self
                    .session
                    .rules
                    .bool_at(myriade_engine::gamerules::vanilla::MOB_GRIEFING),
            items_ignore: !blocklike,
            spared: None,
        };
        let source = self.creatures.wind_source;
        let mut late: Vec<(u32, crate::game::Vec3)> = Vec::new();
        let mut entities = self.take_entities();
        for_each_entity_mut!(entities, entity => {
            if entity.ignores_explosion(&burst) || !entity.pushed_by_wind() {
                return;
            }
            let feet = entity.position();
            let feet = [feet.x, feet.y, feet.z];
            let dist = distance(feet) / diameter;
            if dist > 1.0 {
                return;
            }
            let b = entity.explosion_box();
            let (lo, hi) = (b.min_position(), b.max_position());
            let exposure = self.seen_percent(center, [lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
            let power = (1.0 - dist) * f64::from(exposure) * multiplier;
            let dir = direction([feet[0], feet[1] + entity.explosion_eye_height(), feet[2]]);
            let push = Vec3::new(dir[0] * power, dir[1] * power, dir[2] * power);
            // ServerLevel ticks entities in the order they joined. One that
            // joined before the charge has ticked by now in vanilla, so the
            // push is for its next tick; here its kind ticks after the
            // projectiles, so the push waits for that tick to run.
            let key = entity.join_seq();
            if push_waits_for_tick(entity.ticks_after_projectiles(), key, source) {
                late.push((entity.id(), push));
            } else {
                entity.push_from_explosion(push);
            }
        });
        self.restore_entities(entities);
        self.creatures.late_wind_pushes.extend(late);
        pushed
    }
    /// The pushes `wind_knockback` held for entities that tick after the
    /// projectiles, applied once those entities have ticked.
    pub(super) fn apply_late_wind_pushes(&mut self) {
        let late = std::mem::take(&mut self.creatures.late_wind_pushes);
        if late.is_empty() {
            return;
        }
        let mut entities = self.take_entities();
        for_each_entity_mut!(entities, entity => {
            // Every burst of the tick pushed it, one after the other.
            let id = entity.id();
            for (_, push) in late.iter().filter(|(pushed, _)| *pushed == id) {
                entity.push_from_explosion(*push);
            }
        });
        self.restore_entities(entities);
    }
    /// `ServerExplosion.hurtEntities` for the player: the push, less
    /// explosion knockback resistance, and `ServerPlayer.onExplosionHit`.
    /// Returns the push for players eligible for the explosion packet.
    fn wind_push_player(
        &mut self,
        center: [f64; 3],
        radius: f32,
        multiplier: f32,
        src: Source,
        blocks: &[[i32; 3]],
    ) -> Option<[f64; 3]> {
        if self.spectator() {
            return None;
        }
        let resistance = self
            .player
            .as_mut()
            .and_then(|p| p.attribute("minecraft:explosion_knockback_resistance").ok())
            .unwrap_or(0.0);
        let creative = self.creative;
        let p = self.player.as_ref()?;
        let feet = [p.position.x, p.position.y, p.position.z];
        let dist = ((feet[0] - center[0]).powi(2)
            + (feet[1] - center[1]).powi(2)
            + (feet[2] - center[2]).powi(2))
        .sqrt()
            / f64::from(radius * 2.0);
        let flying = creative && p.flying;
        if dist > 1.0 {
            return None;
        }
        let w = f64::from(p.dimensions.width) / 2.0;
        let h = f64::from(p.dimensions.height);
        let eye = [
            feet[0] - center[0],
            feet[1] + f64::from(p.dimensions.eye_height) - center[1],
            feet[2] - center[2],
        ];
        let exposure = self.seen_percent(
            center,
            [feet[0] - w, feet[1], feet[2] - w],
            [feet[0] + w, feet[1] + h, feet[2] + w],
        );
        let power = (1.0 - dist) * f64::from(exposure) * f64::from(multiplier) * (1.0 - resistance);
        let l = (eye[0] * eye[0] + eye[1] * eye[1] + eye[2] * eye[2]).sqrt();
        let push = if l < 1.0e-5 {
            [0.0; 3]
        } else {
            eye.map(|d| d / l * power)
        };
        let p = self.player.as_mut()?;
        p.velocity = Vec3::new(
            p.velocity.x + push[0],
            p.velocity.y + push[1],
            p.velocity.z + push[2],
        );
        // ServerPlayer.onExplosionHit: a wind charge's burst cancels the
        // fall; any other burst ends the grace of the last impulse.
        let at = p.position;
        p.impulse.set_ignore_fall_damage(src.wind_charge, Some(at));
        // The cause is the charge that burst (`ServerExplosion.source`); a mace's
        // burst has none.
        let cause = if src.wind_charge {
            Some("minecraft:wind_charge")
        } else if src.breeze {
            Some("minecraft:breeze_wind_charge")
        } else {
            None
        }
        .map(|kind| EntityView {
            kind: kind.into(),
            position: center,
            ..EntityView::default()
        });
        self.adv.marks.blast = Some(([at.x, at.y, at.z], cause));
        (!flying).then_some(push)
    }
    /// The burst: knockback, the blocks that react, the gust and the sound.
    pub(super) fn wind_burst(
        &mut self,
        center: [f64; 3],
        radius: f32,
        multiplier: f32,
        src: Source,
    ) -> Result<()> {
        // In a guest's turn (their charge landing, their mace's burst) the
        // host holds the other players (`as_guest`): the burst waits for the
        // turn to end so that it reaches every player, the guest too.
        if self.lan_acting.is_some() && self.lan.is_none() {
            self.creatures.wind_bursts.push(Burst {
                center,
                radius,
                multiplier,
                src,
            });
            return Ok(());
        }
        // ServerExplosion.explode: the blast is heard first.
        let ctx = myriade_engine::sculk::Context::default();
        self.post_game_event("minecraft:explode", center, ctx);
        let mut targets = self.wind_positions(center, radius);
        let pushed = self.wind_knockback(center, radius, multiplier, src, &targets);
        self.wind_burst_knockback(center, radius, multiplier, &targets);
        let block_count = targets.len();
        // Explosion.canTriggerBlocks: a breeze's charge needs mob griefing.
        if !src.breeze
            || self
                .session
                .rules
                .bool_at(myriade_engine::gamerules::vanilla::MOB_GRIEFING)
        {
            // Util.shuffle(list, level.random)
            for i in (2..=targets.len()).rev() {
                let swap = self.world.random.next_int_bound(i as i32).unwrap_or(0) as usize;
                targets.swap(i - 1, swap);
            }
            for pos in targets {
                self.wind_trigger_block(pos)?;
            }
        }
        // `ServerLevel.explode`'s `playSound`: a silent source's burst is mute,
        // and the pitch's two client draws only happen when it is heard.
        if !src.silent {
            let pitch = (1.0
                + (super::explosion::client_float() - super::explosion::client_float()) * 0.2)
                * 0.7;
            self.audio.play(src.sound, Some(center), 4.0, pitch);
        }
        self.lan_wind_burst(center, radius, src.sound, block_count, &pushed);
        Ok(())
    }
    /// The bursts guests' turns put off.
    pub(super) fn flush_wind_bursts(&mut self) -> Result<()> {
        self.flush_smash_pushes();
        for b in std::mem::take(&mut self.creatures.wind_bursts) {
            self.wind_burst(b.center, b.radius, b.multiplier, b.src)?;
        }
        Ok(())
    }
    /// `ServerExplosion.interactWithBlocks` for a `TRIGGER_BLOCK` blast that
    /// is not a wind charge's burst (a mod's `ExplosionInteraction::Trigger`):
    /// the blocks, shuffled, react and none breaks. `canTriggerBlocks`: with a
    /// breeze's charge as the source they react only under mob griefing.
    pub(super) fn explosion_trigger_blocks(&mut self, targets: &mut [[i32; 3]]) -> Result<()> {
        // Util.shuffle(list, level.random)
        for i in (2..=targets.len()).rev() {
            let swap = self.world.random.next_int_bound(i as i32).unwrap_or(0) as usize;
            targets.swap(i - 1, swap);
        }
        let breeze = self.combat.explosion_source.is_some_and(|id| {
            self.ecs.kind(id) == Some("minecraft:breeze_wind_charge")
        });
        if breeze
            && !self
                .session
                .rules
                .bool_at(myriade_engine::gamerules::vanilla::MOB_GRIEFING)
        {
            return Ok(());
        }
        for &pos in targets.iter() {
            self.wind_trigger_block(pos)?;
        }
        Ok(())
    }
    /// `BlockBehaviour.onExplosionHit` for `TRIGGER_BLOCK`: doors, gates,
    /// buttons, levers and bells react as if used; candles go out.
    fn wind_trigger_block(&mut self, pos: [i32; 3]) -> Result<()> {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let name = self.res.facts().get(state).name.clone();
        let prop = |game: &Self, key: &str| {
            game.pack
                .assets
                .registry
                .state(state)
                .and_then(|s| s.properties.get(key).cloned())
        };
        if name.ends_with("candle") || name.ends_with("candle_cake") {
            // AbstractCandleBlock.onExplosionHit: extinguish(null, ...), the
            // candle's own sound and a block-change game event (the smoke is a
            // client-level particle, which a server level never draws).
            self.with_level(|b, level, ticks| {
                b.extinguish_candle(level, ticks, BlockPos::new(pos[0], pos[1], pos[2]))
            });
            self.process_level_events()?;
            return Ok(());
        }
        if !triggers(&name) {
            return Ok(());
        }
        // BellBlock.onExplosionHit: attemptToRing with no direction, so the bell
        // rings toward its facing, not the face the burst came from,
        // powered or not.
        if &*name == "minecraft:bell" {
            self.with_level(|b, level, ticks| {
                b.bell_attempt_to_ring(level, ticks, BlockPos::new(pos[0], pos[1], pos[2]))
            });
            self.process_level_events()?;
            return Ok(());
        }
        // Doors react in their lower half only; powered ones and pressed
        // buttons stay as they are.
        if name.ends_with("_door") && prop(self, "half").as_deref() != Some("lower") {
            return Ok(());
        }
        if prop(self, "powered").as_deref() == Some("true") && &*name != "minecraft:lever" {
            return Ok(());
        }
        // FenceGateBlock.onExplosionHit toggles the gate without turning it
        // towards anyone, as a click would.
        if name.ends_with("_fence_gate") {
            self.with_level(|b, level, ticks| {
                b.fence_gate_exploded(level, ticks, BlockPos::new(pos[0], pos[1], pos[2]))
            });
            self.process_level_events()?;
            return Ok(());
        }
        let at = Vec3::new(
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 0.5,
            f64::from(pos[2]) + 0.5,
        );
        let player = PlayerContext {
            yaw: self.yaw,
            pitch: self.pitch,
            may_build: true,
            can_eat: false,
            game_master: false,
            respawn_point: None,
        };
        self.with_level(|b, level, ticks| {
            b.use_without_item(
                level,
                ticks,
                state,
                horizon_core::geometry::BlockHit {
                    block: BlockPos::new(pos[0], pos[1], pos[2]),
                    direction: Direction::Up,
                    location: at,
                    inside: false,
                },
                player,
            )
        });
        self.process_level_events()?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/wind_dragon_tests.rs"]
mod wind_dragon_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_opens_wooden_doors_but_not_iron_ones() {
        assert!(triggers("minecraft:oak_door"));
        assert!(triggers("minecraft:copper_trapdoor"));
        assert!(!triggers("minecraft:iron_door"));
        assert!(!triggers("minecraft:iron_trapdoor"));
        assert!(triggers("minecraft:lever"));
        assert!(triggers("minecraft:stone_button"));
        assert!(triggers("minecraft:bell"));
        assert!(!triggers("minecraft:stone"));
    }
    /// ServerLevel ticks entities in join order: an orb, falling block or TNT that
    /// joined before the charge has moved by the time the charge bursts, so the
    /// push belongs to its next tick; one that joined after has not.
    #[test]
    fn a_push_waits_only_for_late_ticking_kinds_that_joined_before_the_charge() {
        use crate::game::entity::Entity;
        assert!(push_waits_for_tick(true, 3, Some(7)));
        assert!(!push_waits_for_tick(true, 9, Some(7)), "joined after");
        assert!(!push_waits_for_tick(false, 3, Some(7)), "ticks before it");
        assert!(!push_waits_for_tick(true, 3, None), "not a charge's burst");
        let tnt = crate::game::tnt::PrimedTnt {
            rotation: [0.0, 0.0],
            id: 1,
            position: Vec3::ZERO,
            previous: Vec3::ZERO,
            velocity: Vec3::ZERO,
            fuse: 80,
            on_ground: false,
            fall_distance: 0.0,
            stuck: Vec3::ZERO,
            in_water: false,
            owner: None,
            guest: None,
            no_gravity: false,
            power: 4.0,
            used_portal: false,
            block_state: None,
            join: Default::default(),
        };
        assert!(Entity::ticks_after_projectiles(&tnt));
    }
}
