//! Fishing: `FishingRodItem.use` (cast / reel in), the `FishingHook` (flight,
//! bobbing, lure and bite timers, open water, hooking mobs, retrieving the
//! `gameplay/fishing` loot) and `FishingHookRenderer` (bobber and line).
use super::Game;
use horizon_client::{
    items::ItemStack,
    particles::{ParticleOptions, ParticleType},
    random_source,
};
use horizon_core::{
    geometry::{Aabb, Vec3, trig},
    random::{EntityRandom, RandomSource},
};

const SIZE: f64 = 0.25;
pub(in crate::game) fn bounds(p: Vec3) -> Aabb {
    let h = SIZE / 2.0;
    Aabb::new(
        Vec3::new(p.x - h, p.y, p.z - h),
        Vec3::new(p.x + h, p.y + SIZE, p.z + h),
    )
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
    pub(in crate::game) fn biome_name_at(&self, p: Vec3) -> Option<String> {
        let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let c = self.world.chunks.get(&(x >> 4, z >> 4))?;
        Some(world_biomes!(self).name(c.biome(x, y, z)).to_owned())
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
}
impl Game<'_> {
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
                .filter_map(|g| Some((self.ecs.hook_of(horizon_ecs_proto::targeting::Who::Player(g.entity_id as u32))?, g.conn)))
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
    use super::cast_direction;

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
}
