//! Fishing: `FishingRodItem.use` (cast / reel in), the `FishingHook` (flight,
//! bobbing, lure and bite timers, open water, hooking mobs, retrieving the
//! `gameplay/fishing` loot) and `FishingHookRenderer` (bobber and line).
use super::{Game, view_vector};
use horizon_client::lan::entity::States;
use horizon_client::{
    blocks::Fluid,
    collisions::SceneCollisions,
    gizmos::Line,
    items::ItemStack,
    mesher::Vertex,
    particles::{ParticleOptions, ParticleType},
    pose::Pose,
    random_source,
};
use horizon_core::{
    geometry::{Aabb, BlockHit, Vec3, traverse_blocks, trig},
    position::BlockPos,
    random::{EntityRandom, LegacyRandom, RandomSource},
};
use horizon_protocol::entity_data::{Value, serializer};

#[path = "fishing/impact.rs"]
mod impact;

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Flying,
    Bobbing,
    Hooked(u32),
}
pub(super) struct Hook {
    pub(super) uuid: [i32; 4],
    pub(super) attachment_lifetime: horizon_mod_runtime::attachments::AttachmentLifetime,
    pub(super) join: horizon_client::join_seq::JoinSeq,
    pub(super) caster: Option<u32>,
    pub(super) caster_generation: horizon_client::join_seq::JoinSeq,
    /// Its entity id, which a server's players track it under.
    id: u32,
    pub(super) position: Vec3,
    pub(super) previous: Vec3,
    velocity: Vec3,
    state: State,
    last_deflected_by: Option<u32>,
    needs_sync: bool,
    tick_count: i32,
    life: i32,
    on_ground: bool,
    nibble: i32,
    time_until_lured: i32,
    time_until_hooked: i32,
    fish_angle: f32,
    out_of_water: i32,
    open_water: bool,
    luck: i32,
    lure: i32,
    /// The fishing medium it last floated in outside water (`native_fishing_medium`): the
    /// table its catch rolls and the wait it draws. None in water.
    medium: Option<HookMedium>,
    /// The position a server's update put it heading for.
    lerp: HookLerp,
    pub(super) random: EntityRandom,
}
/// What a hook keeps of the mod's fishing medium it floats in.
#[derive(Clone, Debug, Default, PartialEq)]
struct HookMedium {
    loot_table: Option<String>,
    lure_ticks: Option<(i32, i32)>,
}
/// The medium a mod answered, when the hook is in it: in the block of its
/// surface or below, as a hook is in water anywhere in a water block.
fn inside_medium(
    position: Vec3,
    medium: Option<super::native_hooks::NativeFishingMedium>,
) -> Option<super::native_hooks::NativeFishingMedium> {
    medium.filter(|m| m.surface.is_finite() && position.y < m.surface.floor() + 1.0)
}
const SIZE: f64 = 0.25;
pub(in crate::game) fn bounds(p: Vec3) -> Aabb {
    let h = SIZE / 2.0;
    Aabb::new(
        Vec3::new(p.x - h, p.y, p.z - h),
        Vec3::new(p.x + h, p.y + SIZE, p.z + h),
    )
}
horizon_mod_runtime::impl_attachment_lifetime!(Hook);

impl horizon_client::join_seq::Joined for Hook {
    fn join_seq(&self) -> horizon_client::join_seq::JoinSeq {
        self.join
    }
    fn set_join_seq(&mut self, join: horizon_client::join_seq::JoinSeq) {
        self.join = join;
    }
}
impl Hook {
    pub(super) fn bounds(&self) -> Aabb {
        bounds(self.position)
    }
}

/// `FishingHook.syncronizedRandom`, reseeded every tick from the hook's UUID and the game
/// time so the server and its clients draw alike. Kept off the level's random,
/// whose draws must stay in vanilla's order.
fn synchronized_random(uuid: [i32; 4], game_time: i64) -> LegacyRandom {
    let least = (i64::from(uuid[2]) << 32) | i64::from(uuid[3] as u32);
    LegacyRandom::new(least ^ game_time)
}
/// `FishingHook.onSyncedDataUpdated` on DATA_BITING: the downward plunge,
/// `-0.4F * Mth.nextFloat(syncronizedRandom, 0.6F, 1.0F)`, in float as vanilla.
fn bite_impulse(r: &mut impl RandomSource) -> f64 {
    // Mth.nextFloat(random, min, max): `random.nextFloat() * (max - min) + min`.
    f64::from(-0.4f32 * (r.next_float() * (1.0f32 - 0.6f32) + 0.6f32))
}
/// `FishingHook.onHitBlock`: `getDeltaMovement().normalize().scale(hit.distanceTo(this))`,
/// where `HitResult.distanceTo` is the *squared* distance to the hook: the bobber
/// springs back a little off a wall instead of stopping dead.
fn rebound(velocity: Vec3, hit: Vec3, at: Vec3) -> Vec3 {
    velocity.normalize().scale(hit.distance_squared(at))
}
/// `FishingHook` constructor's three independent triangle draws.
fn cast_velocity(y_sin: f32, y_cos: f32, dir_y: f32, random: &mut EntityRandom) -> Vec3 {
    let v = Vec3::new(f64::from(-y_sin), f64::from(dir_y), f64::from(-y_cos));
    let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    Vec3::new(
        v.x * (0.6 / len + random.triangle(0.5, 0.0103365)),
        v.y * (0.6 / len + random.triangle(0.5, 0.0103365)),
        v.z * (0.6 / len + random.triangle(0.5, 0.0103365)),
    )
}
impl Game<'_> {
    /// `instanceof FishingRodItem`: vanilla's rod, or a content mod's item
    /// copied from it, which casts, floats and reels in as vanilla's does.
    pub(super) fn is_rod(&self, stack: &ItemStack) -> bool {
        self.items()
            .get(&stack.item)
            .is_some_and(|i| i.is("minecraft:fishing_rod"))
    }
    fn holds_rod(&self) -> bool {
        self.held().is_some_and(|s| self.is_rod(s))
            || self.offhand.as_ref().is_some_and(|s| self.is_rod(s))
    }
    /// `FishingRodItem.use`.
    pub(super) fn use_fishing_rod(&mut self, stack: &ItemStack) -> bool {
        if !self.is_rod(stack) {
            return false;
        }
        // The mods hear a server player's cast or reel-in first.
        if self.player.is_some()
            && let (Some(uuid), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone())
        {
            // The hook's bite and catch states are the ECS's: only "out" and "none" are known here.
            let state = if self.ecs_own_hook().is_some() { 3 } else { 0 };
            let mut cancel = false;
            self.native_player_fish(uuid, &name, state, &mut cancel);
            if cancel {
                return true;
            }
        }
        let Some(p) = self.player.as_ref() else {
            return false;
        };
        let at = [p.position.x, p.position.y, p.position.z];
        if self.ecs_own_hook().is_some() {
            let damage = self.ecs_reel_hook(stack);
            // FishingRodItem.hurtAndBreak(..., hand.asEquipmentSlot()): the
            // rod in the acting hand breaks, not necessarily the main hand.
            self.damage_held_in_slot(damage, self.acting_hand_slot());
            let pitch = 0.4 / (self.world.random.next_float() * 0.4 + 0.8);
            self.audio.play(
                "minecraft:entity.fishing_bobber.retrieve",
                Some(at),
                1.0,
                pitch,
            );
            // A rod that broke during retrieval is now an empty ItemStack,
            // whose use-effects component is absent.
            if let Some(rod) = self.held().cloned() {
                self.use_vibration(&rod, "minecraft:item_interact_finish");
            }
        } else {
            let pitch = 0.4 / (self.world.random.next_float() * 0.4 + 0.8);
            self.audio.play(
                "minecraft:entity.fishing_bobber.throw",
                Some(at),
                0.5,
                pitch,
            );
            // FishingHook(player, level, luck, lureSpeed): the levels, the ECS takes them.
            let lure = stack.enchantment_level("minecraft:lure") as u8;
            let luck = stack.enchantment_level("minecraft:luck_of_the_sea") as u8;
            let (yaw, pitch) = (self.yaw, self.pitch);
            let (y_sin, y_cos, dir_y) = cast_direction(yaw, pitch);
            let eye_y = p.position.y + f64::from(p.dimensions.eye_height);
            // snapTo(x, getEyeY(), z): the hook's feet start at the owner's eye.
            let start = Vec3::new(
                p.position.x - f64::from(y_sin) * 0.3,
                eye_y,
                p.position.z - f64::from(y_cos) * 0.3,
            );
            // The hook's own random (`Entity.random`): its UUID, then its velocity.
            let mut random = EntityRandom::own(random_source::entity());
            horizon_client::save::insecure_uuid(&mut random);
            let v = cast_velocity(y_sin, y_cos, dir_y, &mut random);
            self.ecs_cast_hook([start.x, start.y, start.z], [v.x, v.y, v.z], luck, lure);
            // FishingRodItem.use counts a cast, not a reel-in.
            self.award_stat("used", &stack.item, 1);
            self.use_vibration(stack, "minecraft:item_interact_start");
        }
        self.swing_arm();
        true
    }
    /// The boat the hook's owner rides (`Entity.isPassengerOfSameVehicle`).
    fn own_vehicle(&self) -> Option<u32> {
        match self.lan_acting {
            None => self.riding,
            Some(conn) => self.lan.as_ref().and_then(|h| {
                h.hub
                    .guests
                    .iter()
                    .find(|g| g.conn == conn)
                    .and_then(|g| g.vehicle)
            }),
        }
    }
    pub(in crate::game) fn biome_name_at(&self, p: Vec3) -> Option<String> {
        let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let c = self.world.chunks.get(&(x >> 4, z >> 4))?;
        Some(world_biomes!(self).name(c.biome(x, y, z)).to_owned())
    }
    fn water_height(&self, x: i32, y: i32, z: i32) -> Option<(f32, bool)> {
        let f = self.res.facts().get(self.world.block(x, y, z));
        if f.fluid != Fluid::Water {
            return None;
        }
        let above = self.res.facts().get(self.world.block(x, y + 1, z));
        let h = if above.fluid == Fluid::Water {
            1.0
        } else {
            f32::from(f.fluid_level) / 9.0
        };
        Some((h, f.fluid_level == 8 && !f.fluid_falling))
    }
    /// `ServerLevel.sendParticles` of a simple particle.
    fn send_particles(
        &mut self,
        kind: ParticleType,
        at: [f64; 3],
        spread: [f64; 3],
        speed: f64,
        count: u32,
    ) {
        let options = ParticleOptions::simple(kind);
        self.particles
            .send_particles(&options, at, spread, speed, count);
        // The players a server serves are sent them too.
        self.lan_send_particles(super::lan_particles::QueuedParticle {
            dimension: self.dim.id(),
            options,
            state: None,
            at,
            spread,
            speed: [speed; 3],
            count: count as i32,
            randomization: 0,
        });
    }
    /// The bite: the splash at `at` at the pitch the ECS drew, then bubbles and fishing particles around the bobber.
    pub(in crate::game) fn hook_bite_effects(&mut self, at: [f64; 3], pitch: f32) {
        self.audio.play("minecraft:entity.fishing_bobber.splash", Some(at), 0.25, pitch);
        let at = [at[0], at[1] + 0.5, at[2]];
        let count = (1.0 + SIZE as f32 * 20.0) as u32;
        let spread = [SIZE, 0.0, SIZE];
        for kind in [ParticleType::Bubble, ParticleType::Fishing] {
            self.send_particles(kind, at, spread, f64::from(0.2f32), count);
        }
    }
    /// `FishingRodCast`: the stack in the arm holding the rod
    /// (`FishingHookRenderer.getHoldingArm`) while the hook is out.
    pub(super) fn cast_rod(&self) -> Option<&ItemStack> {
        self.hook.as_ref()?;
        if self.main_hand_rod() {
            self.held()
        } else {
            self.offhand.as_ref()
        }
    }
    pub(super) fn main_hand_rod(&self) -> bool {
        self.held().is_some_and(|s| self.is_rod(s))
    }
    /// The hook's position this frame.
    fn hook_position(&self, alpha: f64) -> Option<glam::DVec3> {
        let h = self.hook.as_ref()?;
        let lerp = |a: f64, b: f64| a + (b - a) * alpha;
        Some(glam::DVec3::new(
            lerp(h.previous.x, h.position.x),
            lerp(h.previous.y, h.position.y),
            lerp(h.previous.z, h.position.z),
        ))
    }
    /// FishingHookRenderer: the bobbers (this player's and the others' a
    /// server tracks), camera-facing half-block quads centred on the hooks.
    pub(super) fn fishing_geometry(&self, camera: glam::DVec3, alpha: f64, out: &mut Vec<Vertex>) {
        let hooks: Vec<glam::DVec3> = self
            .hook_position(alpha)
            .into_iter()
            .chain(self.remote_hook_ends(alpha).into_iter().map(|(h, _)| h))
            .collect();
        if hooks.is_empty() {
            return;
        }
        let Some(r) = self
            .gui
            .atlas
            .sprites
            .get("entity/fishing/fishing_hook")
            .copied()
        else {
            return;
        };
        let forward = glam::DVec3::from_array(view_vector(self.yaw, self.pitch));
        let right = forward.cross(glam::DVec3::Y).normalize_or_zero();
        let up = right.cross(forward).normalize_or_zero();
        let s = 0.25;
        let uv = [[r.u0, r.v1], [r.u1, r.v1], [r.u1, r.v0], [r.u0, r.v0]];
        for p in hooks {
            let light =
                self.world
                    .light(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
            let packed = (u32::from(light >> 4) * 16) | ((u32::from(light & 15) * 16) << 8);
            let corners = [
                p - right * s - up * s,
                p + right * s - up * s,
                p + right * s + up * s,
                p - right * s + up * s,
            ];
            for (v, uv) in corners.iter().zip(uv) {
                out.push(Vertex {
                    normal: [0; 4],
                    pos: (*v - camera).as_vec3().to_array(),
                    uv,
                    layer: 0,
                    color: 0xFFFF_FFFF,
                    light: packed,

                    glint_uv: [0.0, 0.0],
                });
            }
        }
    }
    /// FishingHookRenderer: each line, 16 black `lines` segments sagging
    /// from a quarter block above the hook to the rod in its owner's hand,
    /// `width` pixels wide (`Window.getAppropriateLineWidth`).
    pub(super) fn fishing_line(&self, alpha: f64, size: [u32; 2]) -> Vec<Line> {
        let own = self
            .hook_position(alpha)
            .zip(self.rod_hand_position(alpha, size));
        let width = horizon_client::gfx::outline_width(size[0]);
        let mut lines = Vec::new();
        for (hook, hand) in own.into_iter().chain(self.remote_hook_ends(alpha)) {
            let offset = (hand - (hook + glam::DVec3::new(0.0, 0.25, 0.0))).as_vec3();
            let point = |a: f32| {
                let y = offset.y * (a * a + a) * 0.5 + 0.25;
                hook + glam::Vec3::new(offset.x * a, y, offset.z * a).as_dvec3()
            };
            lines.extend((0..16).map(|i| Line {
                start: point(i as f32 / 16.0).to_array(),
                end: point((i + 1) as f32 / 16.0).to_array(),
                color: 0xFF00_0000,
                width,
            }));
        }
        lines
    }
    /// `FishingHookRenderer.getPlayerHandPos`: on the near plane in first
    /// person, beside the body otherwise.
    fn rod_hand_position(&self, alpha: f64, size: [u32; 2]) -> Option<glam::DVec3> {
        let p = self.player.as_ref()?;
        let right_arm = self.main_hand_rod() != self.settings.main_hand_left;
        let invert: f32 = if right_arm { 1.0 } else { -1.0 };
        let swing = trig::sin(f64::from(
            self.swing.animation(alpha as f32).sqrt() * std::f32::consts::PI,
        ));
        let feet = self.previous_position.lerp(p.position, alpha);
        let eye = glam::DVec3::new(feet.x, feet.y + f64::from(p.dimensions.eye_height), feet.z);
        if self.camera_type.is_first_person() {
            let fov = self.settings.fov as i32 as f32;
            // Camera.getNearPlane(fov).getPointOnPlane(invert * 0.525, -0.1).
            let aspect = f64::from(size[0]) / f64::from(size[1].max(1));
            let height =
                (f64::from(fov * (std::f64::consts::PI / 180.0) as f32) / 2.0).tan() * 0.05;
            let forward = glam::DVec3::from_array(view_vector(self.yaw, self.pitch));
            let left = -forward.cross(glam::DVec3::Y).normalize_or_zero();
            let up = forward.cross(left).normalize_or_zero();
            let point = forward * 0.05 + up * (height * -0.1)
                - left * (height * aspect * f64::from(invert * 0.525));
            let point = point * (960.0 / f64::from(fov));
            // Vec3.yRot(swing * 0.5), then xRot(-swing * 0.7).
            let (s, c) = (
                f64::from(trig::sin(f64::from(swing * 0.5))),
                f64::from(trig::cos(f64::from(swing * 0.5))),
            );
            let point = glam::DVec3::new(
                point.x * c + point.z * s,
                point.y,
                point.z * c - point.x * s,
            );
            let (s, c) = (
                f64::from(trig::sin(f64::from(-swing * 0.7))),
                f64::from(trig::cos(f64::from(-swing * 0.7))),
            );
            let point = glam::DVec3::new(
                point.x,
                point.y * c + point.z * s,
                point.z * c - point.y * s,
            );
            Some(eye + point)
        } else {
            let model = self.player_model.as_ref()?;
            let body = mod_api::client::camera::lerp(
                alpha as f32,
                model.previous_body_yaw,
                model.body_yaw,
            );
            let angle = f64::from(body * (std::f64::consts::PI / 180.0) as f32);
            let (sin, cos) = (f64::from(trig::sin(angle)), f64::from(trig::cos(angle)));
            let right = f64::from(invert) * 0.35;
            let crouch = if p.pose == Pose::Crouching {
                -0.1875
            } else {
                0.0
            };
            Some(
                eye + glam::DVec3::new(
                    -cos * right - sin * 0.8,
                    crouch - 0.45,
                    -sin * right + cos * 0.8,
                ),
            )
        }
    }
}
impl Game<'_> {
    /// The hooks drawn here, with their owners' ids (0 for this player's own).
    pub(super) fn hooks_status(&self) -> Vec<serde_json::Value> {
        let at = |p: Vec3| [p.x, p.y, p.z];
        let own = self.ecs_own_hook().and_then(|id| Some((id, 0, self.ecs_view(id)?.position)));
        let others = self
            .remote_hooks
            .iter()
            .map(|h| (h.id, h.owner, at(h.position)));
        own.into_iter()
            .chain(others)
            .map(|(id, owner, p)| {
                serde_json::json!({"id": id, "kind": "minecraft:fishing_bobber", "owner": owner, "position": p})
            })
            .collect()
    }
    /// The cast hooks of the level in place, by ECS id, and who cast each: the player in place, then the guests here. A guest
    /// displaced by a nested swap holds someone else's hook for the moment, so theirs is left out.
    pub(in crate::game) fn fishing_hooks_here(&self) -> impl Iterator<Item = (u32, super::commands::Target)> + '_ {
        use super::commands::Target;
        let own = self.ecs_own_hook().map(|id| (id, Target::Player));
        let guests = self.lan.iter().flat_map(|host| {
            host.hub
                .guests
                .iter()
                .filter(|g| {
                    host.hub.here(g)
                        && self.lan_acting != Some(g.conn)
                        && !self.lan_guest_swaps.contains(&g.conn)
                })
                .filter_map(|g| Some((host.play.get(&g.conn)?.hook?, g.conn)))
                .map(|(id, conn)| (id, Target::Guest(conn)))
        });
        own.into_iter().chain(guests)
    }
    /// The box of the cast hook `id` as the ECS holds it, when it is in the level.
    pub(in crate::game) fn fishing_hook_box(&self, id: u32) -> Option<Aabb> {
        let p = self.ecs_view(id)?.position;
        Some(bounds(Vec3::new(p[0], p[1], p[2])))
    }
    /// The cast hook with an entity id: where it is and who cast it.
    pub(in crate::game) fn fishing_hook_by_id(&self, id: u32) -> Option<(Vec3, super::commands::Target)> {
        let (_, caster) = self.fishing_hooks_here().find(|(hook, _)| *hook == id)?;
        let p = self.ecs_view(id)?.position;
        Some((Vec3::new(p[0], p[1], p[2]), caster))
    }
    /// Takes a cast hook out of the level: whoever holds its id lets it go (the ECS entity is left to its own tick).
    pub(in crate::game) fn remove_fishing_hook(&mut self, id: u32) {
        self.ecs.remove_hook(id);
    }
    /// The hook for `horizon-ctl status`.
    pub(super) fn hook_status(&self) -> Option<serde_json::Value> {
        let h = self.hook.as_ref()?;
        Some(serde_json::json!({
            "state": format!("{:?}", h.state),
            "nibble": h.nibble,
            "lured_in": h.time_until_lured,
            "hooked_in": h.time_until_hooked,
            "open_water": h.open_water,
            "loot_table": h.medium.as_ref().and_then(|m| m.loot_table.clone()),
        }))
    }
}

/// A hook a server tracks to this client (`FishingHook.recreateFromPacket`)
/// that another player cast: drawn where the server puts it, its line
/// running to that player's hand.
pub(super) struct RemoteHook {
    id: u32,
    uuid: [i32; 4],
    pub(super) owner: u32,
    pub(super) owner_fishing: bool,
    pub(super) position: Vec3,
    pub(super) previous: Vec3,
    velocity: Vec3,
    state: State,
    on_ground: bool,
    lerp: HookLerp,
    pub(super) random: EntityRandom,
}

fn push_remote_hook(
    hooks: &mut Vec<RemoteHook>,
    id: u32,
    owner: u32,
    at: Vec3,
    velocity: Vec3,
    uuid: [i32; 4],
) {
    for h in hooks.iter_mut().filter(|h| h.owner == owner) {
        h.owner_fishing = false;
    }
    hooks.push(RemoteHook {
        id,
        uuid,
        owner,
        owner_fishing: true,
        position: at,
        previous: at,
        velocity,
        state: State::Flying,
        on_ground: false,
        lerp: HookLerp::default(),
        random: EntityRandom::own(random_source::entity()),
    });
}

fn remove_remote_hook_from(hooks: &mut Vec<RemoteHook>, id: u32) {
    let owner = hooks.iter().find(|h| h.id == id).map(|h| h.owner);
    hooks.retain(|h| h.id != id);
    if let Some(owner) = owner {
        for h in hooks.iter_mut().filter(|h| h.owner == owner) {
            h.owner_fishing = false;
        }
    }
}

/// The `onGround` of a server's update (`handleMoveEntity`), except that a
/// hook resting on the ground stays so while the update does not lift it: the
/// server's tracker can send the flag a tick behind the landing.
fn resting(was: bool, at: Vec3, target: [f64; 3], sent: bool) -> bool {
    sent || (was && target[1] <= at.y + 1.0e-3)
}

/// `LinearInterpolationHandler`: where a server's update has a hook heading
/// and the steps left (`DEFAULT_INTERPOLATION_STEPS` 3).
#[derive(Default)]
pub(super) struct HookLerp {
    target: Vec3,
    steps: u32,
}
impl HookLerp {
    /// `interpolateTo`: a new target restarts the steps, the same one does not.
    fn start(&mut self, target: [f64; 3]) {
        let target = Vec3::new(target[0], target[1], target[2]);
        if self.steps == 0 || self.target != target {
            self.target = target;
            self.steps = 3;
        }
    }
    /// `doInterpolate`: a step of the way there.
    fn step(&mut self, at: Vec3) -> Vec3 {
        if self.steps == 0 {
            return at;
        }
        let n = f64::from(self.steps);
        self.steps -= 1;
        Vec3::new(
            at.x + (self.target.x - at.x) / n,
            at.y + (self.target.y - at.y) / n,
            at.z + (self.target.z - at.z) / n,
        )
    }
    /// `applyPredictedMovement`: what the hook moved by itself carries the
    /// target along while it is heading there.
    fn moved(&mut self, by: Vec3) {
        if self.steps > 0 {
            self.target = Vec3::new(
                self.target.x + by.x,
                self.target.y + by.y,
                self.target.z + by.z,
            );
        }
    }
}

impl Hook {
    pub(super) fn id(&self) -> u32 {
        self.id
    }
    /// `FishingHook.getHookedIn`, by entity id.
    pub(in crate::game) fn hooked_in(&self) -> Option<u32> {
        match self.state {
            State::Hooked(id) => Some(id),
            _ => None,
        }
    }
    /// `FishingHook.isOpenWaterFishing`.
    pub(in crate::game) fn open_water(&self) -> bool {
        self.open_water
    }
    #[cfg(test)]
    pub(in crate::game) fn set_hooked_in(&mut self, id: u32) {
        self.state = State::Hooked(id);
    }
    /// A server's update of this client's own hook (`handleMoveEntity` also
    /// sets `onGround`).
    pub(super) fn move_to(&mut self, target: [f64; 3], on_ground: bool) {
        self.on_ground = resting(self.on_ground, self.position, target, on_ground);
        self.lerp.start(target);
    }
}
impl RemoteHook {
    pub(super) fn id(&self) -> u32 {
        self.id
    }
    /// A server's update of another player's hook.
    pub(super) fn move_to(&mut self, target: [f64; 3], on_ground: bool) {
        self.on_ground = resting(self.on_ground, self.position, target, on_ground);
        self.lerp.start(target);
    }
}

impl Hook {
    /// This client's own hook as its server tracks it: drawn, never ticked here.
    pub(in crate::game) fn remote(id: u32, at: Vec3, uuid: [i32; 4]) -> Self {
        let mut random = random_source::entity();
        // Entity initializes a local UUID before recreateFromPacket replaces it.
        let _local_uuid = horizon_client::save::insecure_uuid(&mut random);
        let _synchronized = random_source::create();
        Self {
            uuid,
            attachment_lifetime: Default::default(),
            join: horizon_client::join_seq::JoinSeq::UNJOINED,
            caster: None,
            caster_generation: horizon_client::join_seq::JoinSeq::UNJOINED,
            id,
            position: at,
            previous: at,
            velocity: Vec3::ZERO,
            state: State::Flying,
            last_deflected_by: None,
            needs_sync: false,
            tick_count: 0,
            life: 0,
            on_ground: false,
            nibble: 0,
            time_until_lured: 0,
            time_until_hooked: 0,
            fish_angle: 0.0,
            out_of_water: 0,
            open_water: true,
            luck: 0,
            lure: 0,
            medium: None,
            lerp: HookLerp::default(),
            random: EntityRandom::own(random),
        }
    }
    /// `FishingHook`'s entity for the players tracking it: the owner's id in
    /// its add packet, DATA_HOOKED_ENTITY (the hooked id plus one) and
    /// DATA_BITING when they differ from their defaults.
    fn track(&self, owner: i32, out: &mut States) {
        let p = self.position;
        let e = out.add(
            self.id as i32,
            horizon_client::lan::playerdata::ints_uuid(self.uuid),
            "minecraft:fishing_bobber",
        );
        e.position = [p.x, p.y, p.z];
        e.velocity = [self.velocity.x, self.velocity.y, self.velocity.z];
        e.on_ground = self.on_ground;
        e.needs_sync = self.needs_sync;
        e.data = owner;
        if let State::Hooked(id) = self.state {
            e.meta.push((8, serializer::INT, Value::Int(id as i32 + 1)));
        }
        if self.nibble > 0 {
            e.meta.push((9, serializer::BOOLEAN, Value::Bool(true)));
        }
    }
}

impl Game<'_> {
    /// The hooks guests track: each player's in the level.
    pub(super) fn lan_hook_states(&self, out: &mut States) {
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        for g in host.hub.guests.iter().filter(|g| host.hub.here(g)) {
            if let Some(h) = host.play.get(&g.conn).and_then(|p| p.hook.as_ref()) {
                h.track(g.entity_id, out);
            }
        }
    }

    /// `handleAddEntity` for a fishing bobber: this player's own becomes its
    /// hook (`Player.fishing`), another player's is drawn on its line to them.
    pub(super) fn spawn_remote_hook(
        &mut self,
        id: u32,
        uuid: [i32; 4],
        at: Vec3,
        velocity: Vec3,
        owner: i32,
    ) {
        let own = self.remote.as_ref().map(|r| r.entity_id);
        if own == Some(owner) {
            let mut hook = Hook::remote(id, at, uuid);
            hook.velocity = velocity;
            self.install_fishing_hook(hook);
        } else {
            push_remote_hook(&mut self.remote_hooks, id, owner as u32, at, velocity, uuid);
        }
    }

    /// `FishingHook.onSyncedDataUpdated`: this player's hook is biting
    /// (DATA_BITING), which `status` shows as a nibble.
    pub(super) fn remote_hook_data(&mut self, id: u32, values: &[(u8, i32, Value)]) {
        // DATA_HOOKED_ENTITY (the hooked id plus one, 0 for none): a hook on a
        // client follows what it hooked (`FishingHook.onSyncedDataUpdated`).
        let time = self.world.game_time;
        let hooked = |value: &Value| match value {
            Value::Int(v) if *v > 0 => State::Hooked(*v as u32 - 1),
            _ => State::Flying,
        };
        if let Some(h) = self
            .hook
            .as_mut()
            .filter(|h| h.id == id && self.remote.is_some())
        {
            for (index, _, value) in values {
                match (index, value) {
                    (9, Value::Bool(biting)) => {
                        h.nibble = i32::from(*biting);
                        if *biting {
                            h.velocity.y = bite_impulse(&mut synchronized_random(h.uuid, time));
                        }
                    }
                    (8, value) => h.state = hooked(value),
                    _ => {}
                }
            }
        } else if let Some(h) = self.remote_hooks.iter_mut().find(|h| h.id == id) {
            for (index, _, value) in values {
                match (index, value) {
                    (9, Value::Bool(biting)) => {
                        if *biting {
                            h.velocity.y = bite_impulse(&mut synchronized_random(h.uuid, time));
                        }
                    }
                    (8, value) => h.state = hooked(value),
                    _ => {}
                }
            }
        }
    }

    /// `Entity.lerpMotion` for a hook a server tracks: the motion its own tick
    /// moves it on with between the tracker's updates.
    pub(super) fn remote_hook_motion(&mut self, id: u32, v: Vec3) -> bool {
        if self.remote.is_none() {
            return false;
        }
        if let Some(h) = self.hook.as_mut().filter(|h| h.id == id) {
            h.velocity = v;
        } else if let Some(h) = self.remote_hooks.iter_mut().find(|h| h.id == id) {
            h.velocity = v;
        } else {
            return false;
        }
        true
    }

    /// `FishingHook.tick` on a client: the interpolation step towards the
    /// server's last update, then its own movement between the tracker's
    /// updates (every five ticks), floating in water and riding what it
    /// hooked, as the server's does.
    pub(super) fn tick_remote_hooks(&mut self) {
        if self.remote.is_none() {
            return;
        }
        if let Some(mut h) = self.hook.extract() {
            h.previous = h.position;
            h.position = h.lerp.step(h.position);
            let before = h.position;
            let owner = self.player.as_ref().map(|p| p.position);
            (h.position, h.velocity, h.state, h.on_ground) = self.client_hook_step(
                h.id,
                owner,
                (h.position, h.velocity, h.state, h.on_ground),
                &mut h.random,
            );
            h.lerp.moved(h.position.subtract(before));
            self.hook.put_back(h);
        }
        let mut hooks = std::mem::take(&mut self.remote_hooks);
        for h in &mut hooks {
            h.previous = h.position;
            h.position = h.lerp.step(h.position);
            let before = h.position;
            let owner = if self.ecs.is_active() {
                self.ecs.position(h.owner).map(|p| Vec3::new(p[0], p[1], p[2]))
            } else {
                self.remote_entities
                    .iter()
                    .find(|m| m.id == h.owner)
                    .map(|m| m.position)
            };
            (h.position, h.velocity, h.state, h.on_ground) = self.client_hook_step(
                h.id,
                owner,
                (h.position, h.velocity, h.state, h.on_ground),
                &mut h.random,
            );
            h.lerp.moved(h.position.subtract(before));
        }
        self.remote_hooks = hooks;
    }

    /// The fishing medium a mod says the hook at `position` floats in
    /// (`native_fishing_medium`), when it is in it. API 578.
    fn hook_medium(
        &mut self,
        id: u32,
        position: Vec3,
        owner: Option<Vec3>,
    ) -> Option<super::native_hooks::NativeFishingMedium> {
        let mut medium = None;
        let dimension = self.dim.id();
        self.native_fishing_medium(
            dimension,
            id,
            [position.x, position.y, position.z],
            owner.map(|o| [o.x, o.y, o.z]),
            &mut medium,
        );
        inside_medium(position, medium)
    }
    /// `FishingHook.checkCollision` clips blocks on both sides of the connection.
    fn hook_block_hit(&self, from: Vec3, to: Vec3) -> Option<BlockHit> {
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        traverse_blocks(from, to, 1024, |p: BlockPos| {
            let state = self.world.block(p.x, p.y, p.z);
            if self.res.facts().is_air(state) {
                return None;
            }
            self.collision
                .collision_shape(&mut scene, state, [p.x, p.y, p.z])
                .ok()?
                .clip(from, to, p)
        })
        .ok()
        .flatten()
    }

    /// `FishingHook.tick`: one tick of movement without the fishing (the
    /// server's): position, motion, state and whether it rests on the ground.
    fn client_hook_step(
        &mut self,
        id: u32,
        owner: Option<Vec3>,
        (mut position, mut velocity, state, mut on_ground): (Vec3, Vec3, State, bool),
        random: &mut EntityRandom,
    ) -> (Vec3, Vec3, State, bool) {
        let b = [
            position.x.floor() as i32,
            position.y.floor() as i32,
            position.z.floor() as i32,
        ];
        let liquid = self.water_height(b[0], b[1], b[2]).map_or(0.0, |w| w.0);
        // The same fishing medium the server's hook floats in, so it is drawn floating too.
        let medium = if liquid > 0.0 || matches!(state, State::Hooked(_)) {
            None
        } else {
            self.hook_medium(id, position, owner)
        };
        let in_water = liquid > 0.0 || medium.is_some();
        match state {
            State::Flying if in_water => {
                velocity = Vec3::new(velocity.x * 0.3, velocity.y * 0.2, velocity.z * 0.3);
                return (position, velocity, State::Bobbing, on_ground);
            }
            // A landed hook: the server stopped it dead (zero motion) and sends no
            // more than the odd update; it does not fall on from here.
            State::Flying if velocity == Vec3::ZERO && on_ground => {
                return (position, velocity, state, on_ground);
            }
            State::Flying => {
                if let Some(hit) = self.hook_block_hit(position, position.add_vector(velocity)) {
                    velocity = rebound(velocity, hit.location, position);
                }
            }
            State::Hooked(id) => {
                // updateHookedEntity: at 0.8 of the hooked entity's height.
                let hooked = if self
                    .remote
                    .as_ref()
                    .is_some_and(|r| r.entity_id as u32 == id)
                {
                    self.player
                        .as_ref()
                        .map(|p| (p.position, f64::from(p.dimensions.height)))
                } else if self.ecs.is_active() {
                    self.ecs
                        .position(id)
                        .zip(self.ecs.height(id))
                        .map(|(p, h)| (Vec3::new(p[0], p[1], p[2]), f64::from(h)))
                } else {
                    self.remote_entities
                        .iter()
                        .find(|m| m.id == id && m.alive())
                        .map(|m| (m.position, f64::from(m.height())))
                        .or_else(|| {
                            self.carts
                                .iter()
                                .find(|c| c.id == id)
                                .map(|c| (c.position, super::minecarts::HEIGHT))
                        })
                        .or_else(|| {
                            self.dropped
                                .iter()
                                .find(|i| i.id == id)
                                .map(|i| (i.position, horizon_client::entities::ItemEntity::WIDTH))
                        })
                };
                return match hooked {
                    Some((at, height)) => (
                        Vec3::new(at.x, at.y + height * 0.8, at.z),
                        Vec3::ZERO,
                        state,
                        on_ground,
                    ),
                    None => (position, velocity, State::Flying, on_ground),
                };
            }
            State::Bobbing => {
                let mut force = match &medium {
                    Some(m) => position.y + velocity.y - m.surface,
                    None => position.y + velocity.y - f64::from(b[1]) - f64::from(liquid),
                };
                if force.abs() < 0.01 {
                    force += force.signum() * 0.1;
                }
                let r = f64::from(random.next_float());
                velocity = Vec3::new(
                    velocity.x * 0.9,
                    velocity.y - force * r * 0.2,
                    velocity.z * 0.9,
                );
            }
        }
        if !in_water && !on_ground {
            velocity.y -= f64::from(0.03_f32);
        }
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        // The bobber's Entity.move goes through the move gate (arismoko/horizon#2414); PR 1a
        // records the movement unchanged.
        let mut gate = horizon_client::move_gate::armed(
            &self.loop_context,
            id,
            "minecraft:fishing_bobber",
            horizon_world::entity_move::MoverType::SelfMovement,
            velocity,
        );
        let wanted = gate.delta();
        if let Ok(moved) = horizon_client::move_gate::scene_resolve(
            &mut gate,
            &mut scene,
            bounds(position),
            wanted,
            0.0,
            on_ground,
        ) {
            let horizontal =
                (moved.x - wanted.x).abs() > 1.0e-7 || (moved.z - wanted.z).abs() > 1.0e-7;
            on_ground = wanted.y < 0.0 && moved.y > wanted.y;
            position = Vec3::new(
                position.x + moved.x,
                position.y + moved.y,
                position.z + moved.z,
            );
            if state == State::Flying && (on_ground || horizontal) {
                velocity = Vec3::ZERO;
            }
        }
        velocity = Vec3::new(velocity.x * 0.92, velocity.y * 0.92, velocity.z * 0.92);
        (position, velocity, state, on_ground)
    }

    pub(super) fn remove_remote_hook(&mut self, id: u32) {
        if self.remote.is_some() && self.hook.as_ref().is_some_and(|h| h.id == id) {
            self.hook.discard();
        }
        remove_remote_hook_from(&mut self.remote_hooks, id);
    }

    pub(super) fn clear_remote_hooks(&mut self) {
        if self.remote.is_some() {
            self.hook.discard();
        }
        self.remote_hooks.clear();
    }

    /// Other players' hooks: the bobber and its line to the owner's hand
    /// (`FishingHookRenderer.getPlayerHandPos` in third person), as
    /// (hook, hand) this frame.
    fn remote_hook_ends(&self, alpha: f64) -> Vec<(glam::DVec3, glam::DVec3)> {
        let lerp = |a: Vec3, b: Vec3| {
            glam::DVec3::new(
                a.x + (b.x - a.x) * alpha,
                a.y + (b.y - a.y) * alpha,
                a.z + (b.z - a.z) * alpha,
            )
        };
        self.remote_hooks
            .iter()
            .filter_map(|h| {
                // The ECS keeps no previous pose or body yaw of its own yet: see the report (`remote_hook_ends`).
                let owner = self.remote_entities.iter().find(|m| m.id == h.owner)?;
                let feet = lerp(owner.previous, owner.position);
                let body = mod_api::client::camera::lerp(
                    alpha as f32,
                    owner.previous_body_yaw,
                    owner.body_yaw,
                );
                let angle = f64::from(body * (std::f64::consts::PI / 180.0) as f32);
                let (sin, cos) = (f64::from(trig::sin(angle)), f64::from(trig::cos(angle)));
                // The right arm; a crouching owner's eyes are lower.
                let eye = owner.eye_y() - owner.position.y;
                let hand = feet
                    + glam::DVec3::new(
                        -cos * 0.35 - sin * 0.8,
                        eye - 0.45,
                        -sin * 0.35 + cos * 0.8,
                    );
                Some((lerp(h.previous, h.position), hand))
            })
            .collect()
    }
}

/// `FishingHook(Player, Level, int, int)`: `(ySin, yCos, y)` of the cast direction, using
/// `Mth.sin`/`Mth.cos` (the 65536-entry table), so straight down gives `xCos == -0.0`, y `-5`.
fn cast_direction(yaw: f32, pitch: f32) -> (f32, f32, f32) {
    let ya = f64::from(-yaw * trig::DEG_TO_RAD - std::f32::consts::PI);
    let pa = f64::from(-pitch * trig::DEG_TO_RAD);
    let (y_sin, y_cos) = (trig::sin(ya), trig::cos(ya));
    let (x_cos, x_sin) = (-trig::cos(pa), trig::sin(pa));
    (y_sin, y_cos, (-(x_sin / x_cos)).clamp(-5.0, 5.0))
}

#[cfg(test)]
mod tests {
    use super::{Hook, cast_direction, rebound};

    #[test]
    fn casting_straight_down_aims_down_and_straight_up_aims_up() {
        assert_eq!(cast_direction(0.0, 90.0).2, -5.0);
        assert_eq!(cast_direction(123.0, 89.9).2, -5.0);
        assert_eq!(cast_direction(0.0, -90.0).2, 5.0);
        assert_eq!(cast_direction(0.0, 0.0).2, 0.0);
    }

    #[test]
    fn cast_direction_uses_the_vanilla_sine_table() {
        // pitch 45: y = -(sin(-a) / -cos(-a)) with table entries built independently.
        let a = f64::from(-45.0_f32 * std::f32::consts::PI / 180.0);
        let table = |i: i64| ((i & 65535) as f64 / 10430.378350470453).sin() as f32;
        let (s, c) = (
            table((a * 10430.378350470453) as i64),
            -table((a * 10430.378350470453 + 16384.0) as i64),
        );
        let (_, _, y) = cast_direction(0.0, 45.0);
        assert!((y - (-(s / c))).abs() < 1e-3, "{y}");
    }

    #[test]
    fn removing_one_of_two_remote_hooks_clears_owner_fishing() {
        let mut hooks = Vec::new();
        super::push_remote_hook(&mut hooks, 10, 7, Vec3::ZERO, Vec3::ZERO, [0; 4]);
        super::push_remote_hook(&mut hooks, 11, 7, Vec3::ZERO, Vec3::ZERO, [0; 4]);
        super::push_remote_hook(&mut hooks, 20, 12, Vec3::ZERO, Vec3::ZERO, [0; 4]);
        assert!(!hooks[0].owner_fishing);
        assert!(hooks[1].owner_fishing);
        assert!(hooks[2].owner_fishing);
        super::remove_remote_hook_from(&mut hooks, 10);
        assert_eq!(hooks.len(), 2);
        assert!(!hooks.iter().any(|h| h.owner == 7 && h.owner_fishing));
        assert!(hooks.iter().any(|h| h.owner == 12 && h.owner_fishing));
    }
}
