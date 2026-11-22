//! Firework rockets (`FireworkRocketItem`, `FireworkRocketEntity`): launching
//! from a block face, boosting a gliding player, the 1.15 horizontal
//! acceleration and 0.04 lift, the flight-duration lifetime, explosion
//! damage with line of sight, and the client explosion (`createFireworks`).
use super::{Game, Hit, view_vector};
use horizon_client::{
    items::ItemStack,
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
        let right = !offhand_only;
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
}

/// `FireworkRocketItem.useOn` returns PASS while the player is fall flying,
/// leaving the boost to `use` (a guest's UseItemOn must not launch a rocket).
fn use_on_passes(fall_flying: bool) -> bool {
    fall_flying
}

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

    #[test]
    fn use_on_passes_only_while_gliding() {
        assert!(super::use_on_passes(true));
        assert!(!super::use_on_passes(false));
    }
}
