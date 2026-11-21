//! Firework rockets (`FireworkRocketItem`, `FireworkRocketEntity`): launching
//! from a block face, boosting a gliding player, the 1.15 horizontal
//! acceleration and 0.04 lift, the flight-duration lifetime, explosion
//! damage with line of sight, and the client explosion (`createFireworks`).
use super::{Game, Hit, item_draw::owner_at, view_vector};
use horizon_assets::item_model::DisplayContext;
use horizon_client::{
    block_entity_render::vertex_light,
    items::ItemStack,
    mesher::Vertex,
    mobs::Who,
    particles::{ParticleOptions, ParticleType},
    random_source,
    text::Text,
};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::targeting;

pub(super) const FIREWORK_EXPLODE_EVENT: i8 = 17;

pub(super) use horizon_gameplay::fireworks::{Rocket, fireworks_of};
/// `damageAmount * (float)Math.sqrt((5.0 - this.distanceTo(target)) / 5.0)`
/// from the rocket at `from` to a target at `to`; `Entity.distanceTo` sums
/// the squared offsets in floats.
pub(super) fn blast_damage(damage: f32, from: Vec3, to: Vec3) -> f32 {
    let (x, y, z) = (
        (from.x - to.x) as f32,
        (from.y - to.y) as f32,
        (from.z - to.z) as f32,
    );
    let distance = f64::from(x * x + y * y + z * z).sqrt() as f32;
    damage * ((5.0 - f64::from(distance)) / 5.0).sqrt() as f32
}
impl Game<'_> {
    /// The rocket a launch carries: its stars are the item's; the level draws its motion, UUID and lifetime.
    pub(super) fn new_rocket(&mut self, at: Vec3, stack: &ItemStack, attached: bool) -> Rocket {
        let (_, explosions) = fireworks_of(stack);
        Rocket {
            attachment_lifetime: Default::default(),
            rotation: [0.0, 0.0],
            join: Default::default(),
            id: 0,
            position: at,
            previous: at,
            velocity: Vec3::ZERO,
            life: 0,
            lifetime: 0,
            explosions,
            attached,
            guest: None,
            owner: None,
            owner_uuid: None,
            left_owner: false,
            attached_to: None,
            angled: false,
            item: stack.clone(),
            random: horizon_core::random::EntityRandom::own(random_source::entity()),
            on_ground: false,
            has_been_shot: false,
            silent: false,
            fall_distance: 0.0,
            stuck: Vec3::ZERO,
            was_touching_water: false,
            first_tick: true,
        }
    }
    /// The bridge's `Who` of a host `Who`: the acting player by entity id, a mob by its ECS entity.
    fn bridge_who(&self, who: Option<Who>) -> Option<targeting::Who> {
        match who? {
            Who::Player => self.player.as_ref().map(|p| targeting::Who::Player(p.entity_id as u32)),
            Who::Mob(id) => self.ecs.entity_of(id).map(targeting::Who::Entity),
        }
    }
    /// Launches the rocket `r` in the level: its constructor and its flight are P's; a boosting one is held by its player.
    pub(in crate::game) fn push_rocket(&mut self, r: Rocket, stack: &ItemStack) {
        let (p, v) = (r.position, r.velocity);
        let flight = fireworks_of(stack).0;
        let owner = self.bridge_who(r.owner);
        let carrier = if r.attached { self.bridge_who(Some(Who::Player)) } else { None };
        let (angled, mut next) = (r.angled, self.next_entity);
        self.ecs_with(|ecs, mw| {
            ecs.launch_rocket(mw, [p.x, p.y, p.z], [v.x, v.y, v.z], angled, r.explosions, flight, owner, carrier, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
    }
    /// `FireworkRocketItem.useOn`: launch from the clicked point.
    pub(super) fn launch_firework(&mut self, hit: &Hit, stack: &ItemStack) -> bool {
        if &*stack.item != "minecraft:firework_rocket"
            || use_on_passes(self.player.as_ref().is_some_and(|p| p.fall_flying))
        {
            return false;
        }
        let at = Vec3::new(
            hit.point[0] + f64::from(hit.face[0]) * 0.15,
            hit.point[1] + f64::from(hit.face[1]) * 0.15,
            hit.point[2] + f64::from(hit.face[2]) * 0.15,
        );
        let mut rocket = self.new_rocket(at, stack, false);
        rocket.owner = Some(Who::Player);
        rocket.guest = self.lan_acting;
        self.push_rocket(rocket, stack);
        if !self.creative {
            self.consume_held();
        }
        // `ItemStack.useOn` awards ITEM_USED for `FireworkRocketItem.useOn`'s SUCCESS.
        self.award_stat("used", "minecraft:firework_rocket", 1);
        self.swing_arm();
        true
    }
    /// A rocket shot at an angle (`DATA_SHOT_AT_ANGLE`): out of a dispenser
    /// (`FireworkRocketItem.asProjectile`) or a crossbow.
    ///
    /// `shoot` gives the shot's velocity from the rocket's own random, after the
    /// constructor's draws (`asProjectile`, then `spawnProjectileUsingShoot`).
    pub(super) fn launch_angled_rocket(
        &mut self,
        at: Vec3,
        stack: &ItemStack,
        owner: Option<Who>,
        shoot: impl FnOnce(&mut horizon_core::random::EntityRandom) -> Vec3,
    ) {
        let mut own = horizon_core::random::EntityRandom::own(random_source::entity());
        let mut rocket = self.new_rocket(at, stack, false);
        rocket.velocity = shoot(&mut own);
        rocket.turn_to_shot();
        rocket.angled = true;
        rocket.owner = owner;
        rocket.guest = self.lan_acting.filter(|_| owner == Some(Who::Player));
        self.push_rocket(rocket, stack);
    }
    /// `Entity.getHandHoldingItemAngle(Items.FIREWORK_ROCKET)` for the local
    /// player: half a block out towards the hand holding the rocket while
    /// gliding, zero otherwise. A rocket only in the offhand is in the arm
    /// opposite the main one.
    fn rocket_hand_angle(&self) -> [f64; 3] {
        if !self.player.as_ref().is_some_and(|p| p.fall_flying) {
            return [0.0; 3];
        }
        let is_rocket =
            |s: Option<&ItemStack>| s.is_some_and(|s| &*s.item == "minecraft:firework_rocket");
        let main = self.stack_in(self.selected);
        let offhand_only = is_rocket(self.offhand.as_ref()) && !is_rocket(main);
        let right = offhand_only == self.settings.main_hand_left;
        let arm = if right { 80 } else { -80 };
        let v = view_vector(self.yaw + arm as f32, 0.0);
        [v[0] * 0.5, v[1] * 0.5, v[2] * 0.5]
    }
    /// `FireworkRocketEntity.tick` for a rocket attached to the local player:
    /// a glide is boosted towards the look, and the rocket sits at the rocket
    /// hand with the player's motion.
    pub(super) fn follow_own_player(&mut self, r: &mut Rocket) {
        let hand = self.rocket_hand_angle();
        // getLookAngle reads Mth's float sine table, as the server does; the
        // f64 sin/cos of super::view_vector drift the boost off the server's
        // path and trip the movement check while gliding under a rocket.
        let look = horizon_world::view_direction::view_vector(self.pitch, self.yaw);
        let Some(p) = self.player.as_mut() else {
            return;
        };
        if p.fall_flying {
            let m = p.velocity;
            p.velocity = Vec3::new(
                m.x + look.x * 0.1 + (look.x * 1.5 - m.x) * 0.5,
                m.y + look.y * 0.1 + (look.y * 1.5 - m.y) * 0.5,
                m.z + look.z * 0.1 + (look.z * 1.5 - m.z) * 0.5,
            );
        }
        r.position = p.position.add_vector(Vec3::new(hand[0], hand[1], hand[2]));
        r.velocity = p.velocity;
    }
    /// The trail (`FireworkRocketEntity.tick` on a client): a spark each tick
    /// at the rocket, drifting against its motion.
    pub(super) fn rocket_spark(&mut self, r: &Rocket) {
        let g = |g: &mut horizon_core::random::LegacyRandom| g.next_gaussian() * 0.05;
        let v = [
            g(&mut self.world.random),
            -r.velocity.y * 0.5,
            g(&mut self.world.random),
        ];
        let at = [r.position.x, r.position.y, r.position.z];
        self.particles
            .add(&ParticleOptions::simple(ParticleType::Firework), at, v);
    }
    /// `FireworkRocketItem.use` while gliding: drop the player's leads, then a
    /// rocket attached to the player.
    pub(super) fn boost_firework(&mut self, stack: &ItemStack) -> bool {
        if &*stack.item != "minecraft:firework_rocket"
            || !self.player.as_ref().is_some_and(|p| p.fall_flying)
        {
            return false;
        }
        // Server path of `use`: `dropAllLeashConnections(null)`, and when that
        // drops anything `playSound(null, player, LEAD_BREAK, NEUTRAL, 1, 1)`.
        // The entity overload follows the player and excludes nobody, including
        // the guest whose action is running while its hub is swapped out.
        if self.drop_all_leash_connections() {
            self.play_player_bound_sound("minecraft:item.lead.break", 6, 1.0, 1.0, false);
        }
        let at = self.player.as_ref().map_or(Vec3::ZERO, |p| p.position);
        let mut rocket = self.new_rocket(at, stack, true);
        rocket.owner = Some(Who::Player);
        rocket.guest = self.lan_acting;
        self.push_rocket(rocket, stack);
        if !self.creative {
            self.consume_held();
        }
        self.award_stat("used", "minecraft:firework_rocket", 1);
        self.swing_arm();
        if let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) {
            self.native_player_elytra_boost(player, &name);
        }
        true
    }
    /// `Projectile.getOwner` as a target's `Who`: a guest's `Who::Player`
    /// (kept with the connection that shot it) is their entity, not the host.
    pub(super) fn shooter_who(&self, owner: Option<Who>, guest: Option<u32>) -> Option<Who> {
        let owner = owner?;
        let Some(conn) = guest.filter(|_| owner == Who::Player) else {
            return Some(owner);
        };
        let host = self.lan.as_ref()?;
        let g = host.hub.guests.iter().find(|g| g.conn == conn)?;
        Some(Who::Mob(g.entity_id as u32))
    }
    /// FireworkRocketEntity.handleEntityEvent: use the synchronized item data.
    pub(super) fn remote_firework_explosion(&mut self, id: i32) {
        let camera = self.eye_position();
        if let Some(r) = self.rockets.iter().find(|r| r.id == id as u32) {
            let at = [r.position.x, r.position.y, r.position.z];
            let v = [r.velocity.x, r.velocity.y, r.velocity.z];
            self.particles
                .fireworks(at, v, r.explosions.clone(), camera.unwrap_or(at), !r.silent);
        }
    }
    /// `hurtServer` with `damageSources().fireworks` from the rocket at
    /// `from`: the rocket is the direct entity, so the player is knocked away
    /// from it and the death message names it; creative and spectator
    /// players are immune.
    pub(in crate::game) fn rocket_hurt_player(&mut self, from: Vec3, amount: f32) {
        // Ability, spectator and death refusals belong to hurtServer, after
        // its invulnerability observation (also for an attached target).
        // No mob ever owns a rocket.
        self.combat.projectile_owner = None;
        self.combat.rocket_hit = true;
        self.combat.projectile_direct =
            Some(Text::translate("entity.minecraft.firework_rocket", &[]));
        let at = [from.x, from.y, from.z];
        self.hurt_player_from(amount, Some(at), "minecraft:fireworks", Some(None), false);
    }
    /// `FireworkEntityRenderer.submit`: the item (GROUND) facing the camera,
    /// lit at the rocket's light probe.
    pub(super) fn rocket_geometry(&self, camera: glam::DVec3, alpha: f64, out: &mut Vec<Vertex>) {
        let orientation = glam::Mat4::from_quat(self.camera_orientation());
        // shouldRender: not while attached to an entity (`isAttachedToEntity`).
        for r in self
            .rockets
            .iter()
            .filter(|r| !r.attached && r.attached_to.is_none())
        {
            // FireworkRocketEntity.shouldRenderAtSqrDistance: within 64 blocks;
            // EntityRenderer.shouldRender: its 0.25 box in the view.
            if !self.standing_entity_in_view(
                camera,
                super::entity_culling::at(r.previous),
                super::entity_culling::at(r.position),
                alpha,
                0.25,
                0.25,
                64.0,
                0.0,
            ) {
                continue;
            }
            let p = glam::DVec3::new(
                r.previous.x + (r.position.x - r.previous.x) * alpha,
                r.previous.y + (r.position.y - r.previous.y) * alpha,
                r.previous.z + (r.position.z - r.previous.z) * alpha,
            );
            // `updateForNonLiving`: GROUND, owned by the rocket.
            let owner = owner_at(p.to_array(), 0.0, "minecraft:firework_rocket");
            let state = self.resolve_item(&r.item, DisplayContext::Ground, Some(&owner), 0);
            if state.is_empty() {
                continue;
            }
            let mut pose = glam::Mat4::from_translation((p - camera).as_vec3()) * orientation;
            if r.angled {
                pose *= glam::Mat4::from_rotation_z(std::f32::consts::PI)
                    * glam::Mat4::from_rotation_y(std::f32::consts::PI)
                    * glam::Mat4::from_rotation_x(std::f32::consts::FRAC_PI_2);
            }
            // getLightProbePosition: the eye, 0.85 of its 0.25 height.
            let light = self.world.light(
                p.x.floor() as i32,
                (p.y + 0.2125).floor() as i32,
                p.z.floor() as i32,
            );
            // Frame generation and temporal upscaling follow its own flight.
            self.mark_motion(out.len(), Self::MOTION_OBJECT | u64::from(r.id));
            self.submit_item_world(&state, pose, vertex_light(light), out);
        }
        self.mark_motion(out.len(), 0);
    }
}

/// `FireworkRocketItem.useOn` returns PASS while the player is fall flying,
/// leaving the boost to `use` (a guest's UseItemOn must not launch a rocket).
fn use_on_passes(fall_flying: bool) -> bool {
    fall_flying
}

#[cfg(test)]
#[path = "tests/own_rocket_spark_tests.rs"]
mod own_spark_tests;

#[cfg(test)]
mod tests {
    /// `Projectile.shoot` turns a shot rocket to its motion at once.
    #[test]
    fn a_shot_rocket_starts_turned_to_its_motion() {
        use horizon_core::geometry::Vec3;
        let mut rocket = super::Rocket::remote(1, Vec3::ZERO, Vec3::new(0.0, 0.5, 0.0));
        rocket.turn_to_shot();
        assert!((rocket.rotation[1] - 90.0).abs() < 1e-3);
        let mut rocket = super::Rocket::remote(1, Vec3::ZERO, Vec3::new(1.0, 0.0, 1.0));
        rocket.turn_to_shot();
        assert!((rocket.rotation[0] - 45.0).abs() < 1e-4);
        assert!(rocket.rotation[1].abs() < 1e-4);
    }


    /// A flying rocket's vertices carry its own id for frame generation's and a temporal
    /// upscaler's motion vectors (`Frame::entity_motion`), so the motion matched between
    /// two frames is its flight, not only the camera's.
    #[test]
    #[ignore = "daily"]
    fn a_flying_rocket_opens_a_motion_run_of_its_own() {
        use crate::game::{Audio, Options, Progress, Settings, load_resources};
        use horizon_core::geometry::Vec3;

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
        let res = loaded.resources;
        let mut game = super::Game::new(
            &res,
            loaded.pack,
            loaded.gui.unwrap(),
            options,
            settings,
            Audio::silent(&horizon_assets::resource_source::ResourceSource::directory(
                root.clone(),
            )),
            None,
        )
        .unwrap();
        let camera = glam::DVec3::new(0.5, 64.0, 0.5);
        let start = Vec3::new(0.5, 64.0, 4.5);
        game.rockets
            .push(super::Rocket::remote(77, start, Vec3::ZERO));
        let id = super::Game::MOTION_OBJECT | 77;
        // One frame's motion runs: the rocket's vertices open a run under its own id, and
        // a camera-only run (id 0) closes it so nothing drawn after takes its motion.
        game.entity_motion.borrow_mut().clear();
        let mut out = Vec::new();
        game.rocket_geometry(camera, 1.0, &mut out);
        let marks = game.entity_motion.borrow().clone();
        let at = marks
            .iter()
            .position(|m| m.1 == id)
            .expect("the rocket's run");
        assert_eq!(
            marks[at].0, 0,
            "the run starts at the rocket's first vertex"
        );
        assert_eq!(marks.last(), Some(&(out.len() as u32, 0)));
    }

    #[test]
    fn use_on_passes_only_while_gliding() {
        assert!(super::use_on_passes(true));
        assert!(!super::use_on_passes(false));
    }

    /// `FireworkRocketItem.use` on the server, while fall-flying: a lead on a
    /// mob within the 32-block box drops as an item and one
    /// `minecraft:item.lead.break` follows the player on NEUTRAL, heard by the
    /// acting player too. A lead outside that box, or held for another player,
    /// stays. Standing still does neither.
    #[test]
    #[ignore = "daily"]
    fn a_gliding_firework_drops_leads_and_plays_lead_break() {
        use crate::game::{Audio, Options, Progress, Settings, load_resources};
        use horizon_client::mobs::{Mob, leash::Holder};
        use horizon_core::geometry::Vec3;

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
        options.execution_role = crate::game::execution_role::ExecutionRole::IntegratedServer;
        let res = loaded.resources;
        let mut game = super::Game::new(
            &res,
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
        game.player = Some(
            horizon_client::motion::PlayerMotion::new(game.motion, Vec3::new(10.5, 64.0, -3.25))
                .unwrap(),
        );
        let feet = {
            let p = game.player.as_ref().expect("a player").position;
            [p.x, p.y, p.z]
        };
        let kind = game
            .res
            .mob_types()
            .get("minecraft:cow")
            .expect("cow")
            .clone();
        // A server action keeps the guest identity while its hub is swapped out.
        game.lan_acting = Some(11);
        game.lan_acting_entity = Some(37);
        let me = game.acting_uuid();
        let someone_else = [me[0].wrapping_add(1), me[1], me[2], me[3]];
        let mut leashed = |id, at: Vec3, owner: [i32; 4]| {
            let mut mob = Mob::new(id, kind.clone(), at, 0.0);
            mob.extra.leash = Some(Holder::Player);
            mob.extra.leash_player = Some(owner);
            game.mobs.push(mob);
        };
        let here = Vec3::new(feet[0] + 1.0, feet[1], feet[2]);
        leashed(1, here, me);
        leashed(2, Vec3::new(feet[0] + 40.0, feet[1], feet[2]), me);
        leashed(3, here, someone_else);
        game.inventory[game.selected] = Some(super::ItemStack::new("minecraft:firework_rocket", 1));
        let stack = game.inventory[game.selected].clone().unwrap();
        game.audio.capture = Some(Vec::new());

        assert!(!game.boost_firework(&stack));
        assert_eq!(game.mobs[0].extra.leash, Some(Holder::Player));
        assert!(game.dropped.is_empty());
        assert!(game.audio.capture.as_ref().unwrap().is_empty());

        game.player.as_mut().unwrap().fall_flying = true;
        assert!(game.boost_firework(&stack));
        assert!(
            game.mobs[0].extra.leash.is_none(),
            "the mob on the lead comes free"
        );
        assert_eq!(game.mobs[1].extra.leash, Some(Holder::Player));
        assert_eq!(game.mobs[2].extra.leash, Some(Holder::Player));
        let leads: Vec<_> = game
            .dropped
            .iter()
            .filter(|d| &*d.stack.item == "minecraft:lead")
            .collect();
        assert_eq!(leads.len(), 1);
        let heard = game.audio.capture.take().unwrap_or_default();
        let snaps: Vec<_> = heard
            .iter()
            .filter(|s| s.event == "minecraft:item.lead.break")
            .collect();
        assert_eq!(snaps.len(), 1);
        let snap = snaps[0];
        assert_eq!((snap.position, snap.volume, snap.pitch), (feet, 1.0, 1.0));
        assert_eq!(
            snap.entity,
            Some(37),
            "the snap must follow the acting guest"
        );
        assert_eq!(snap.source, 6, "SoundSource.NEUTRAL");
        assert_eq!(snap.except_entity, None, "vanilla excludes no player");
        let (packet, range) =
            horizon_client::lan::containers::captured_sound_packet(snap, None, 42);
        let horizon_protocol::packets::play::Clientbound::SoundEntity(packet) = packet else {
            panic!("a firework lead snap must use the entity-bound packet");
        };
        assert_eq!((packet.id, packet.source, range), (37, 6, 16.0));
        assert_eq!(packet.sound.1.unwrap().0, "minecraft:item.lead.break");
    }
}
