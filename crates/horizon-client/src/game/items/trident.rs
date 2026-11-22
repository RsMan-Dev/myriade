//! Tridents: `TridentItem.releaseUsing` (throw after 10 ticks, or Riptide in
//! water or rain) and `ThrownTrident` (arrow flight with 0.99 water inertia,
//! 8 damage plus Impaling, the reversed bounce off a hit, sticking, Loyalty's
//! return to the owner, pickup) with `ThrownTridentRenderer`, and Channeling's
//! lightning on a hit (`channeling.json`).
use super::level_sounds::LevelSound;
use super::{Game, view_vector};
use horizon_client::{
    collisions::SceneCollisions,
    items::ItemStack,
    mobs::Who,
};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::trident::Pickup;
use mod_api::SoundSource;

pub(super) use horizon_gameplay::trident::ThrownTrident;

impl Game<'_> {
    /// The trident stack's maximum damage component, else its item's default.
    pub(super) fn trident_max_damage(&self, stack: &ItemStack) -> u32 {
        stack
            .data()
            .and_then(|data| match data.other("minecraft:max_damage") {
                Some(horizon_core::nbt::Tag::Int(value)) => u32::try_from(*value).ok(),
                _ => None,
            })
            .unwrap_or_else(|| self.items().get(&stack.item).map_or(250, |i| i.max_damage))
    }
    /// `TridentItem.use`: start the throw unless one more use would break it.
    /// Returns false for `InteractionResult.FAIL` (about to break, or Riptide
    /// while dry) so callers neither dip the hand nor consume the click.
    pub(super) fn use_trident(&mut self, stack: &ItemStack) -> bool {
        if &*stack.item != "minecraft:trident" {
            return false;
        }
        let max = self.trident_max_damage(stack);
        let riptide = stack.enchantment_level("minecraft:riptide") > 0;
        if stack.damage >= max.saturating_sub(1) || (riptide && !self.in_water_or_rain()) {
            return false;
        }
        self.start_using_item();
        true
    }
    pub(in crate::game) fn in_water_or_rain(&mut self) -> bool {
        let Some(p) = &self.player else { return false };
        let (at, top) = (p.position, p.bounds().max_position().y);
        p.fluids.was_touching_water || self.in_rain(at, top)
    }
    /// The `summon_entity` and `play_sound` effects of `channeling.json`: in a
    /// thunderstorm (`weather_check`) a spot that sees the sky (`can_see_sky`)
    /// gets a lightning bolt and the thunder sound.
    pub(in crate::game) fn channel_lightning(&mut self, at: [f64; 3], guest: Option<u32>) {
        // Level.isThundering: the thunder level, in dimensions with weather.
        if !self.can_have_weather() || self.weather_levels(1.0).1 <= 0.9 {
            return;
        }
        // BlockAndLightGetter.canSeeSky: full sky light.
        let (x, y, z) = (
            at[0].floor() as i32,
            at[1].floor() as i32,
            at[2].floor() as i32,
        );
        if self.world.light(x, y, z) >> 4 != 15 {
            return;
        }
        if !self.lightning_strike_allowed(self.lan_acting_uuid, [x, y, z], "trident") {
            return;
        }
        self.ecs.set_next_bolt_cause(Some(guest.unwrap_or(0)));
        self.spawn_bolt(at, false);
        self.audio
            .play("minecraft:item.trident.thunder", Some(at), 5.0, 1.0);
    }
    /// `TridentItem.releaseUsing`.
    pub(super) fn release_trident(&mut self, ticks: u32) {
        if ticks < 10 {
            return;
        }
        let Some(stack) = self.held().cloned() else {
            return;
        };
        let Some(eye) = self.eye_position() else {
            return;
        };
        let riptide = stack.enchantment_level("minecraft:riptide");
        // `releaseUsing` checks the wet/passenger conditions, then nextDamageWillBreak again.
        if riptide > 0 && (!self.in_water_or_rain() || self.riding.is_some()) {
            return;
        }
        if stack.damage >= self.trident_max_damage(&stack).saturating_sub(1) {
            return;
        }
        let at = self
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        // Counted (on the server) before the throw or the spin.
        {
            self.award_stat("used", "minecraft:trident", 1);
        }
        if riptide > 0 {
            // `hurtWithoutBreaking` and the sound belong to the server; a server's
            // client only launches.
            let server_client = false;
            if !server_client {
                self.hurt_held_without_breaking(1);
            }
            // trident_spin_attack_strength: 1.5 + 0.75 per level above the first.
            let spin_stack = self.held().cloned();
            let strength = 1.5 + 0.75 * (riptide as f32 - 1.0);
            let d = view_vector(self.yaw, self.pitch);
            if let Some(p) = self.player.as_mut() {
                p.velocity = Vec3::new(
                    p.velocity.x + d[0] * f64::from(strength),
                    p.velocity.y + d[1] * f64::from(strength),
                    p.velocity.z + d[2] * f64::from(strength),
                );
                // Player.startAutoSpinAttack(20, 8, stack).
                p.auto_spin_attack_ticks = 20;
                p.auto_spin_attack_dmg = 8.0;
                p.auto_spin_attack_item = spin_stack;
            }
            // `player.move(SELF, (0, 1.1999999, 0))` when on the ground: the lift
            // stops at a ceiling. A self move through the move gate (arismoko/horizon#2414);
            // PR 1a records the movement unchanged.
            let lift = self
                .player
                .as_ref()
                .filter(|p| p.ground.on_ground)
                .map(|p| p.bounds());
            if let Some(bounds) = lift {
                let mut scene = SceneCollisions {
                    world: &self.world,
                    binding: self.collision,
                    subject: None,
                };
                let up = Vec3::new(0.0, f64::from(1.1999999f32), 0.0);
                let mut gate = horizon_client::move_gate::armed(
                    &self.loop_context,
                    self.player.as_ref().map_or(0, |p| p.entity_id as u32),
                    "minecraft:player",
                    horizon_world::entity_move::MoverType::SelfMovement,
                    up,
                );
                let moved = horizon_client::move_gate::scene_resolve(
                    &mut gate, &mut scene, bounds, up, 0.0, true,
                )
                .unwrap_or(Vec3::ZERO);
                if let Some(p) = self.player.as_mut() {
                    p.position.y += moved.y;
                }
            }
            if !server_client {
                let sound = format!("minecraft:item.trident.riptide_{}", riptide.min(3));
                // level.playSound(null, player, sound, PLAYERS, 1, 1).
                match (self.lan_acting_entity, at) {
                    (Some(player), Some(at)) => {
                        let sound =
                            LevelSound::new(&sound, at, 1.0, 1.0).in_source(SoundSource::Players);
                        self.play_bound_sound(sound, player, None);
                    }
                    _ => self.audio.play(&sound, at, 1.0, 1.0),
                }
                if let (Some(player), Some(name)) =
                    (self.lan_acting_uuid, self.lan_acting_name.clone())
                {
                    self.native_player_riptide(player, &name);
                }
            }
            return;
        }
        // The server throws the trident.
        // hurtWithoutBreaking (Unbreaking applies), then the thrown copy.
        self.hurt_held_without_breaking(1);
        let thrown = self.held().cloned().unwrap_or(stack);
        if !self.creative {
            *self.held_mut() = None;
        }
        let start = Vec3::new(eye[0], eye[1] - 0.1, eye[2]);
        let mut projectile = horizon_client::projectiles::Projectile::new(
            horizon_client::projectiles::ProjectileKind::Snowball,
            start,
            Some(Who::Player),
        );
        let (v, on_ground) = self.player.as_ref().map_or((Vec3::ZERO, true), |p| {
            (self.known_player_movement(), p.ground.on_ground)
        });
        projectile.shoot_from_rotation(self.pitch, self.yaw, 0.0, 2.5, 1.0, v, on_ground);
        // The ECS level flies it: its owner, Loyalty, Channeling and its pickup mode.
        let loyalty = thrown.enchantment_level("minecraft:loyalty");
        let channeling = thrown.enchantment_level("minecraft:channeling") > 0;
        let pickup = if self.creative { Pickup::CreativeOnly } else { Pickup::Allowed };
        let vel = projectile.velocity;
        let mut next = self.next_entity;
        let mut alloc = || {
            next += 1;
            next
        };
        let thrower = horizon_ecs_proto::targeting::Who::Player(self.own_entity_id() as u32);
        let id = self.ecs_with(|ecs, mw| {
            ecs.throw_trident(
                mw,
                [start.x, start.y, start.z],
                [vel.x, vel.y, vel.z],
                Some(thrower),
                loyalty as u8,
                channeling,
                pickup,
                thrown,
                &mut alloc,
            )
        });
        self.next_entity = next;
        self.player_projectile_shot();
        // level.playSound(null, trident, sound, PLAYERS, 1, 1).
        let throw = LevelSound::new(
            "minecraft:item.trident.throw",
            [start.x, start.y, start.z],
            1.0,
            1.0,
        )
        .in_source(SoundSource::Players);
        self.play_bound_sound(throw, id, None);
    }
    /// `settle_dealt_absorbed` for the player who threw a trident: a LAN guest's own
    /// statistics are only reachable inside `as_guest`, so the host is not credited.
    pub(in crate::game) fn thrower_dealt_absorbed(&mut self, guest: Option<u32>) {
        let Some(conn) = guest.filter(|c| self.lan_acting != Some(*c)) else {
            self.settle_dealt_absorbed(true);
            return;
        };
        let entered = self.as_guest(conn, |g| g.settle_dealt_absorbed(true));
        if entered.is_none() {
            self.settle_dealt_absorbed(false);
        }
    }
    /// The thrower's `PLAYER_HURT_ENTITY` for a trident hit: run as the
    /// guest that threw it, and not at all once they have left.
    pub(in crate::game) fn thrower_hit(
        &mut self,
        guest: Option<u32>,
        victim: &horizon_client::advancements::EntityView,
        direct: horizon_client::advancements::EntityView,
        damage: f32,
    ) {
        let Some(conn) = guest.filter(|c| self.lan_acting != Some(*c)) else {
            self.player_hit(victim, "minecraft:trident", Some(direct), damage);
            return;
        };
        self.as_guest(conn, |g| {
            g.player_hit(victim, "minecraft:trident", Some(direct), damage)
        });
    }
}

#[cfg(test)]
mod tests {
    /// A stack can reach its final durability point after use began. The release
    /// guard is independent of admission, and short charges still do nothing.
    #[test]
    #[ignore = "porté dans P : trident_hit"]
    fn trident_release_rechecks_stack_durability_after_charge() {
        use crate::game::{Audio, Options, PlayerMotion, Progress, Settings, load_resources};
        use horizon_core::nbt::Tag;
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            crate::game::execution_role::ExecutionRole::IntegratedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.game_dir = Some(
            std::path::PathBuf::from(std::env::var_os("HORIZON_HOME").unwrap())
                .join("trident-release"),
        );
        options.execution_role = crate::game::execution_role::ExecutionRole::IntegratedServer;
        let mut game = super::Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
            Audio::default(),
        )
        .unwrap();
        game.player =
            Some(PlayerMotion::new(game.motion, super::Vec3::new(4.5, 200.0, 4.5)).unwrap());

        // `Entity.isInWaterOrRain` follows the live fluid interaction. The
        // player's feet can be in air at the water surface while the body is
        // still touching water, so the block at the feet is not sufficient.
        let mut riptide = super::ItemStack::new("minecraft:trident", 1);
        riptide.enchant("minecraft:riptide", 3);
        game.player.as_mut().unwrap().fluids.was_touching_water = true;
        *game.held_mut() = Some(riptide.clone());
        assert!(game.use_trident(&riptide));
        game.using = None;

        let mut stack = super::ItemStack::new("minecraft:trident", 1);
        stack.damage = 8;
        stack
            .data_mut()
            .set_other("minecraft:max_damage", Some(Tag::Int(10)));
        *game.held_mut() = Some(stack.clone());
        assert!(game.use_trident(&stack));
        assert!(game.using.is_some());
        game.held_mut().as_mut().unwrap().damage = 9;
        game.release_trident(10);
        assert!(game.tridents.is_empty());
        assert_eq!(game.held().unwrap().damage, 9);
        game.using = None;

        stack.damage = 249;
        stack
            .data_mut()
            .set_other("minecraft:max_damage", Some(Tag::Int(500)));
        *game.held_mut() = Some(stack.clone());
        assert!(game.use_trident(&stack));
        assert!(game.using.is_some());
        game.release_trident(9);
        assert!(game.tridents.is_empty());
        assert_eq!(game.held().unwrap().damage, 249);
        game.release_trident(10);
        assert_eq!(game.tridents.len(), 1);
        assert_eq!(game.tridents[0].stack.damage, 250);
        assert!(game.held().is_none());
        assert_eq!(game.tridents[0].pickup, horizon_client::arrows::Pickup::Allowed);
        assert_eq!(game.tridents[0].owner_uuid, game.level.player_uuid);
        assert!(!game.tridents[0].has_been_shot);
        assert!(!game.tridents[0].left_owner);

        game.creative = true;
        game.using = None;
        let stack = super::ItemStack::new("minecraft:trident", 1);
        *game.held_mut() = Some(stack.clone());
        assert!(game.use_trident(&stack));
        game.release_trident(10);
        assert_eq!(game.tridents[1].pickup, horizon_client::arrows::Pickup::CreativeOnly);
        assert!(game.held().is_some());
    }
}
