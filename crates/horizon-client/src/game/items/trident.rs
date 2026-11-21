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
    mesher::Vertex,
    mobs::{Who, anim::Poses},
};
use horizon_core::{geometry::Vec3, random::RandomSource};
use horizon_ecs_proto::trident::Pickup;
use mod_api::SoundSource;

pub(super) use horizon_gameplay::trident::ThrownTrident;
use horizon_gameplay::trident::HALF;

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
        self.spawn_bolt(at, false);
        // The summon effect's owner is the thrower: LightningBolt.setCause.
        if let Some(bolt) = self.bolts.last_mut() {
            bolt.cause = Some(guest);
        }
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
        if self.remote.is_none() {
            self.award_stat("used", "minecraft:trident", 1);
        }
        if riptide > 0 {
            // `hurtWithoutBreaking` and the sound belong to the server; a server's
            // client only launches.
            let server_client = self.remote.is_some();
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
        if self.remote.is_some() {
            return;
        }
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
        let id = self.ecs_with(|ecs, mw| {
            ecs.throw_trident(
                mw,
                [start.x, start.y, start.z],
                [vel.x, vel.y, vel.z],
                Some(horizon_ecs_proto::targeting::Who::Player),
                loyalty,
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
    /// ThrownTridentRenderer.
    pub(super) fn trident_geometry(&self, camera: glam::DVec3, alpha: f64, out: &mut Vec<Vertex>) {
        // AbstractArrow.shouldRenderAtSqrDistance: ten times the default.
        let range = 10.0
            * super::entity_culling::default_range(
                glam::DVec3::splat(2.0 * HALF),
                self.view_scale(),
            );
        let live = self
            .tridents
            .iter()
            .filter_map(|t| {
                // EntityRenderer.shouldRender; ThrownTridentRenderer grows the box by 1.5. One
                // only the ray scene's entity layer reaches is its alone (`ray_only`).
                let sight = self.standing_entity_sight(
                    camera,
                    super::entity_culling::at(t.previous),
                    super::entity_culling::at(t.position),
                    alpha,
                    2.0 * HALF,
                    2.0 * HALF,
                    range,
                    1.5,
                )?;
                Some((t, sight.camera))
            })
            .map(|(t, seen)| {
                let at = [
                    t.previous.x + (t.position.x - t.previous.x) * alpha,
                    t.previous.y + (t.position.y - t.previous.y) * alpha,
                    t.previous.z + (t.position.z - t.previous.z) * alpha,
                ];
                (t, at, seen)
            });
        self.draw_tridents(camera, live, out);
    }

    pub(super) fn pickup_trident_geometry(
        &self,
        camera: glam::DVec3,
        alpha: f64,
        out: &mut Vec<Vertex>,
    ) {
        // ItemPickupParticle: a picked-up trident flying to its collector,
        // drawn with the render state extracted at partial tick 1.
        let taken = self.pickups.live.iter().filter_map(|p| match &p.taken {
            super::pickups::Taken::Trident(t) => {
                Some((&**t, super::pickups::position(p, alpha), true))
            }
            _ => None,
        });
        self.draw_tridents(camera, taken, out);
    }

    fn draw_tridents<'a>(
        &self,
        camera: glam::DVec3,
        tridents: impl Iterator<Item = (&'a ThrownTrident, [f64; 3], bool)>,
        out: &mut Vec<Vertex>,
    ) {
        let Some(model) = &self.trident_model else {
            return;
        };
        let Some(uv) = self
            .gui
            .atlas
            .sprites
            .get("entity/trident/trident")
            .map(|r| [r.u0, r.v0, r.u1, r.v1])
        else {
            return;
        };
        // The texture's OptiFine emissive overlay, drawn again full bright.
        let glow = self
            .res
            .mob_data()
            .emissive
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .overlay("entity/trident/trident")
            .and_then(|e| self.gui.atlas.sprites.get(e))
            .map(|r| [r.u0, r.v0, r.u1, r.v1]);
        for (t, at, seen) in tridents {
            // Frame generation and temporal upscaling follow its own flight and spin.
            self.mark_motion(out.len(), Self::MOTION_OBJECT | u64::from(t.id));
            let p = glam::DVec3::from_array(at) - camera;
            let base = glam::Mat4::from_translation(p.as_vec3())
                * glam::Mat4::from_rotation_y((t.yaw - 90.0).to_radians())
                * glam::Mat4::from_rotation_z((t.pitch + 90.0).to_radians());
            let l = self.world.light(
                t.position.x.floor() as i32,
                t.position.y.floor() as i32,
                t.position.z.floor() as i32,
            );
            if t.fire_ticks > 0 && seen {
                // The flames face the camera, turned round in the front view.
                let fire_yaw =
                    if self.camera_type == horizon_client::camera::CameraType::ThirdPersonFront {
                        self.yaw + 180.0
                    } else {
                        self.yaw
                    };
                horizon_client::fire_render::flames(
                    at,
                    0.5,
                    0.5,
                    camera,
                    fire_yaw,
                    &self.gui.atlas.sprites,
                    l,
                    out,
                );
            }
            let packed = (u32::from(l >> 4) * 16) | ((u32::from(l & 15) * 16) << 8);
            let poses = Poses::new(model);
            let start = out.len();
            horizon_client::mobs::render::draw(
                model,
                base,
                &poses,
                uv,
                0xFFFF_FFFF,
                packed,
                0,
                out,
            );
            if let Some(glow) = glow {
                let full = horizon_client::mobs::render::FULL_BRIGHT;
                horizon_client::mobs::render::draw(
                    model,
                    base,
                    &poses,
                    glow,
                    0xFFFF_FFFF,
                    full,
                    0,
                    out,
                );
            }
            if seen {
                self.lit_entity(start..out.len());
            } else {
                self.ray_only(out, start);
            }
        }
        self.mark_motion(out.len(), 0);
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
            false,
            crate::game::execution_role::ExecutionRole::IntegratedServer,
            &Progress::new(),
            &mut |_, _| unreachable!(),
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
            loaded.gui.unwrap(),
            options,
            settings,
            Audio::silent(&horizon_assets::resource_source::ResourceSource::directory(
                root.to_path_buf(),
            )),
            None,
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
        assert_eq!(game.tridents[0].pickup, super::Pickup::Allowed);
        assert_eq!(game.tridents[0].owner_uuid, game.level.player_uuid);
        assert!(!game.tridents[0].has_been_shot);
        assert!(!game.tridents[0].left_owner);

        game.creative = true;
        game.using = None;
        let stack = super::ItemStack::new("minecraft:trident", 1);
        *game.held_mut() = Some(stack.clone());
        assert!(game.use_trident(&stack));
        game.release_trident(10);
        assert_eq!(game.tridents[1].pickup, super::Pickup::CreativeOnly);
        assert!(game.held().is_some());
    }
}
