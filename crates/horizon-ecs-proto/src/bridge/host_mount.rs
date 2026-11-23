//! The mounts of the host's entities, by `Entity` (greffe lot 70): the rider's input and the charged jump, the jump's sound and cooldown,
//! the seats and the dismounts, the fall of the riders, the saddle and the shears. The drive is `mount::ridden_drive`; these functions
//! write the input, read the state, and mark the views they change.
use crate::bridge::{host::EcsWorld, view};
use crate::cow::{now_of, Body, Cause, DamageQueue, FluidState, GrowAt, Health, MaxHealth, Pos, Yaw};
use crate::equipment::set_slot;
use crate::goals::{self, Equipment, Online, Slot};
use crate::items::{item_id, worn_in};
use crate::attr_mod::AttrMods;
use crate::mount::{self, Mount, PassengerSeats, Ridden, RiderSeat, Steer};
use crate::mount_jump::{JumpCharge, JumpSound};
use crate::pilot::{self, PilotInput};
use crate::rules::attachments::{attachment, rider_offset, scaled, seat};
use crate::tame::Owner;
use crate::targeting::Who;
use crate::type_tags;
use bevy_ecs::prelude::*;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};

/// A player's standing box (`Player`'s 0.6 by 1.8), for the dismount search of a player rider.
const PLAYER_WIDTH: f64 = 0.6;
const PLAYER_HEIGHT: f64 = 1.8;

impl EcsWorld {
    /// `LocalPlayer.aiStep`'s jump charge of the mount `e`: pressed with a rider, the bar fills; released, the bar's scale is the result and
    /// the charge ends. 0.0 without a rider, a charge or a mount.
    pub fn charge_jump(&mut self, e: Entity, pressed: bool) -> f32 {
        // A mount whose jump key does nothing (a stick mount, a happy ghast) has no bar to charge.
        if !self.jumpable(e) {
            return 0.0;
        }
        let w = self.world_mut();
        let Some(rider) = w.get::<Mount>(e).map(|m| m.rider.is_some()) else { return 0.0 };
        if pressed && rider {
            match w.get_mut::<JumpCharge>(e) {
                Some(mut c) => c.ticks += 1,
                None => {
                    w.entity_mut(e).insert(JumpCharge { ticks: 1 });
                }
            }
            return 0.0;
        }
        let ticks = w.get::<JumpCharge>(e).map(|c| c.ticks);
        if ticks.is_some() {
            w.entity_mut(e).remove::<JumpCharge>();
        }
        // The packet's data is the bar's scale times 100; the server turns it into the power (`getPlayerJumpPendingScale`).
        ticks.filter(|_| rider && !pressed).map_or(0.0, |t| crate::rules::mount::pending_scale((mount::jump_bar(t) * 100.0) as i32))
    }

    /// `Mount.input` of the mount `e` from its player rider's keys this tick. A mount with no player rider takes nothing. A sneak gets the
    /// player off (`Player.wantsToStopRiding`) and ends the charge. While the jump key is held the jump is 0: the bar's result comes on release.
    /// The stick (carrot or warped fungus on a stick) in the rider's hand steers. `jump` is the charge's result (`charge_jump`).
    #[allow(clippy::too_many_arguments)]
    pub fn ride_input(&mut self, e: Entity, move_xz: [f32; 2], jump: f32, jump_held: bool, sprint: bool, sneak: bool, yaw: f32, pitch: f32) {
        let w = self.world_mut();
        let Some(Who::Player(id)) = w.get::<Mount>(e).and_then(|m| m.rider) else { return };
        if sneak {
            mount::dismount(w, e);
            w.entity_mut(e).remove::<JumpCharge>();
            return;
        }
        let held = w.resource::<Online>().get(id).map_or(0, |p| p.main);
        let steer = held == goals::carrot_on_a_stick() || held == goals::warped_fungus_on_a_stick();
        // A flying mount (`HappyGhast.getRiddenInput`) reads the key held (`Player.isJumping`), not the bar: its jump is 1 while held.
        let flies = w.get::<Mount>(e).is_some_and(|m| matches!(m.steer, Steer::Fly { .. }));
        // A jump already pending (a guest's START_RIDING_JUMP, `start_jump`) stays until the drive uses it, as `playerJumpPendingScale` does.
        let pending = w.get::<PilotInput>(e).map_or(0.0, |p| p.jump);
        let jump = if flies { f32::from(jump_held) } else if jump_held { 0.0 } else if jump > 0.0 { jump } else { pending };
        pilot::set(w, e, PilotInput { who: Who::Player(id), forward: move_xz[1], strafe: move_xz[0], yaw, pitch, jump, sprint, sneak: false, steer, keys: pilot::keys_of(move_xz[1], move_xz[0]) });
    }

    /// `AbstractHorse.handleStartJump`: a jumpable mount with a rider starts its jump and plays its sound. None when refused.
    /// `ServerGamePacketListenerImpl.handlePlayerCommand` START_RIDING_JUMP of a guest: the gate and the sound below, then the power
    /// the packet carries (`getPlayerJumpPendingScale(data)`) becomes the pilot's pending jump, written on arrival.
    pub fn start_jump(&mut self, e: Entity, data: i32) -> Option<(&'static str, f32)> {
        let sound = self.jump_gate(e, data)?;
        let w = self.world_mut();
        if let Some(Who::Player(id)) = w.get::<Mount>(e).and_then(|m| m.rider) {
            let power = crate::rules::mount::pending_scale(data);
            match w.get::<PilotInput>(e).copied() {
                Some(p) => {
                    pilot::set(w, e, PilotInput { jump: power, ..p });
                }
                None => {
                    let none = PilotInput { who: Who::Player(id), forward: 0.0, strafe: 0.0, yaw: 0.0, pitch: 0.0, jump: power, sprint: false, sneak: false, steer: false, keys: [false; 4] };
                    pilot::set(w, e, none);
                }
            }
        }
        Some(sound)
    }

    fn jump_gate(&self, e: Entity, data: i32) -> Option<(&'static str, f32)> {
        let w = self.world();
        let m = w.get::<Mount>(e)?;
        if m.rider.is_none() || data <= 0 {
            return None;
        }
        // `AbstractNautilus.handleStartJump`: the dash sound of the water or of the land.
        if let Steer::Look { sounds, .. } = m.steer {
            let wet = w.get::<crate::cow::FluidState>(e).is_some_and(|f| f.eye_in_water);
            return m.saddled.then_some((sounds[usize::from(!wet)], 1.0));
        }
        // `Camel.canJump` (saddled, not refusing to move) then `handleStartJump`: the dash sound its rule carries.
        if matches!(m.steer, Steer::Dash { .. }) {
            let refused = w.get::<crate::behaviours::posture::Posture>(e).is_some_and(|p| p.refuses(crate::cow::game_now(w)));
            if !m.saddled || refused {
                return None;
            }
            return w.get::<JumpSound>(e).map(|s| (s.sound, s.volume));
        }
        self.jump_config(e).map(|c| (c.sound, c.volume))
    }

    /// The sound and volume a jump of the mount `e` plays (`AbstractHorse.playJumpSound`), None for a mount that does not jump.
    pub fn jump_sound(&self, e: Entity) -> Option<(&'static str, f32)> {
        self.jump_config(e).map(|c| (c.sound, c.volume))
    }

    /// The jump capability of `e`: the mount jumps when it is jumpable, with the `JumpSound` it carries or the default one.
    fn jump_config(&self, e: Entity) -> Option<JumpSound> {
        self.bar_jumps(e).then(|| self.world().get::<JumpSound>(e).copied().unwrap_or_default())
    }

    /// A saddled mount that jumps with the bar: a charged steer with a jump strength.
    fn bar_jumps(&self, e: Entity) -> bool {
        self.world().get::<Mount>(e).is_some_and(|m| matches!(m.steer, Steer::Charged) && m.jump_strength > 0.0 && m.saddled)
    }

    /// `PlayerRideableJumping.canJump`: a saddled mount whose jump key does something, the bar of a horse, the dash of a camel (unless it
    /// refuses to move) or of a nautilus. The marker of the jump (HUD bar, jump packet), no species.
    pub fn jumpable(&self, e: Entity) -> bool {
        let w = self.world();
        let Some(m) = w.get::<Mount>(e) else { return false };
        if !m.saddled {
            return false;
        }
        match m.steer {
            Steer::Charged => m.jump_strength > 0.0,
            Steer::Look { .. } => true,
            Steer::Dash { .. } => !w.get::<crate::behaviours::posture::Posture>(e).is_some_and(|p| p.refuses(crate::cow::game_now(w))),
            Steer::Stick { .. } | Steer::Fly { .. } => false,
        }
    }

    /// The player `player` leaves the mount `e` (a guest who disconnects): its seat, first or second, is freed. False when it did not ride it.
    pub fn dismount_player(&mut self, e: Entity, player: u32) -> bool {
        let Some(m) = self.world().get::<Mount>(e) else { return false };
        let (first, second) = (m.rider == Some(Who::Player(player)), m.passenger == Some(Who::Player(player)));
        if first {
            mount::dismount(self.world_mut(), e).is_some()
        } else if second {
            mount::dismount_passenger(self.world_mut(), e).is_some()
        } else {
            false
        }
    }

    /// A mount that dashes (`Steer::Dash`): its jump start also raises the ENTITY_ACTION game event (`Camel.handleStartJump`).
    pub fn dashes(&self, e: Entity) -> bool {
        self.world().get::<Mount>(e).is_some_and(|m| matches!(m.steer, Steer::Dash { .. } | Steer::Look { .. }))
    }

    /// A mount steered by the look of its rider (`Steer::Look`, the nautilus kind): the player's vehicle view reads it as one.
    pub fn steers_by_look(&self, e: Entity) -> bool {
        self.world().get::<Mount>(e).is_some_and(|m| matches!(m.steer, Steer::Look { .. }))
    }

    /// A dashing mount (`Steer::Dash`) that may not dash yet: its `ready_at` is ahead of its own time.
    pub fn has_jump_cooldown(&self, e: Entity) -> bool {
        let w = self.world();
        w.get::<Mount>(e).is_some_and(|m| matches!(m.steer, Steer::Dash { .. } | Steer::Look { .. }) && m.ready_at > now_of(w, e))
    }

    /// The max health and the health of `e`, None without health.
    /// `getControllingPassenger` is a player: a saddled mount whose first rider is a player (a stick-steered one only while the rider
    /// holds the stick). A harnessed happy ghast (`Steer::Fly`) steers when it is not on its still timeout.
    pub fn player_steers(&self, e: Entity) -> bool {
        let w = self.world();
        let Some(m) = w.get::<crate::mount::Mount>(e) else { return false };
        if !matches!(m.rider, Some(crate::targeting::Who::Player(_))) {
            return false;
        }
        if matches!(m.steer, crate::mount::Steer::Fly { .. }) {
            let still = w.get::<crate::behaviours::still_under_riders::Above>(e).is_some() || w.get::<crate::behaviours::still_under_riders::StillUntil>(e).is_some_and(|u| now_of(w, e) < u.0);
            return w.get::<crate::behaviours::harness_worn::Harness>(e).is_some() && !still;
        }
        if !m.saddled {
            return false;
        }
        match m.steer {
            crate::mount::Steer::Stick { .. } => crate::pilot::input_of(w, e).is_some_and(|p| p.steer),
            _ => true,
        }
    }

    pub fn vehicle_health(&self, e: Entity) -> Option<(f32, f32)> {
        let w = self.world();
        Some((w.get::<MaxHealth>(e)?.0, w.get::<Health>(e)?.0))
    }

    /// The point of the first seat of `e` (`mount::seats`): where its rider stands.
    pub fn rider_seat(&self, e: Entity) -> Option<[f64; 3]> {
        let w = self.world();
        let m = w.get::<Mount>(e)?;
        let rider = m.rider;
        let count = [m.rider, m.passenger].iter().flatten().count();
        let (pos, yaw, body) = (w.get::<Pos>(e)?.0, w.get::<Yaw>(e)?.0, w.get::<Body>(e)?);
        let scale = mount::scale_of(w.get::<AttrMods>(e));
        let baby = w.get::<GrowAt>(e).is_some();
        // As `mount::seats`: the posture's seats, else the species' list; a rearing mount slides its rider back.
        let base = match (w.get::<crate::behaviours::posture::Posture>(e), w.get::<PassengerSeats>(e)) {
            (Some(p), _) => {
                let elapsed = crate::cow::game_now(w) - p.since;
                let moving = (elapsed < crate::rules::posture::transition_ticks(p.sitting, p.sit_down, p.stand_up)).then_some(elapsed as f32);
                crate::rules::attachments::camel_attachment(body.height, baby, 0, count, matches!(rider, Some(Who::Entity(_))), p.sitting, moving)
            }
            (None, Some(s)) => attachment(&s.adult, s.baby, baby, 0),
            (None, None) => [0.0, f64::from(body.height), 0.0],
        };
        let slide = w.get::<crate::behaviours::random_stand::StandAnim>(e).map_or([0.0; 3], |a| crate::rules::attachments::rear_slide(a.last, baby));
        let attachment_of = [base[0] + slide[0], base[1] + slide[1], base[2] + slide[2]];
        let offset = match rider {
            Some(Who::Entity(r)) => w.get::<RiderSeat>(r).map_or(0.0, |rs| {
                let rider_width = w.get::<Body>(r).map_or(0.0, |b| f64::from(b.width));
                rider_offset(*rs, f64::from(body.width), rider_width, w.get::<GrowAt>(r).is_some(), mount::scale_of(w.get::<AttrMods>(r)))
            }),
            Some(Who::Player(_)) => crate::species::attachments::rider_seat("minecraft:player").map_or(0.0, |rs| rs.adult),
            None => 0.0,
        };
        let at = seat(scaled(attachment_of, scale), offset, pos, yaw);
        Some([at.x, at.y, at.z])
    }

    /// `LivingEntity.getDismountLocationForPassenger` of the mount `e` for its rider, read in the world `mw` (floor heights and collisions,
    /// as `spawn` and `teleport` read it): the right side, then the left, each `getDismountLocationInDirection`'s search from the top of the
    /// mount up to 0.75 above it; else the top of the mount.
    pub fn dismount_point(&self, mw: &MobWorld<'_>, e: Entity) -> Option<[f64; 3]> {
        let w = self.world();
        let rider = w.get::<Mount>(e)?.rider?;
        let (pw, ph) = match rider {
            Who::Entity(r) => w.get::<Body>(r).map(|b| (f64::from(b.width), f64::from(b.height)))?,
            Who::Player(_) => (PLAYER_WIDTH, PLAYER_HEIGHT),
        };
        let (at, yaw, body) = (w.get::<Pos>(e)?.0, w.get::<Yaw>(e)?.0, w.get::<Body>(e)?);
        let (mount_w, top) = (f64::from(body.width), at.y + f64::from(body.height));
        let limit = top + 0.75;
        let in_direction = |x: f64, z: f64| {
            let mut y = top.floor() as i32;
            loop {
                let floor = mw.block_floor_height(x.floor() as i32, y, z.floor() as i32);
                if f64::from(y) + floor > limit {
                    return None;
                }
                if floor.is_finite() && floor < 1.0 {
                    let fy = f64::from(y) + floor;
                    let b = Aabb::new(Vec3::new(x - pw / 2.0, fy, z - pw / 2.0), Vec3::new(x + pw / 2.0, fy + ph, z + pw / 2.0));
                    if mw.no_collision(b) {
                        return Some([x, fy, z]);
                    }
                }
                y += 1;
                if f64::from(y) >= limit {
                    return None;
                }
            }
        };
        // `getCollisionHorizontalEscapeVector`: a step of (width + rider's width) / 2 at the yaw turned a quarter.
        let found = [90.0_f64, -90.0].into_iter().find_map(|turn| {
            let f = (mount_w + pw) / 2.0 + 1e-5;
            let a = (f64::from(yaw) + turn).to_radians();
            in_direction(at.x - a.sin() * f, at.z + a.cos() * f)
        });
        Some(found.unwrap_or([at.x, top, at.z]))
    }

    /// The riders of `e` get off in water when their eyes are in it and `e` carries the `dismounts_underwater` tag.
    pub fn dismounts_in_water(&self, e: Entity, eyes_in_water: bool) -> bool {
        eyes_in_water && type_tags::named(self.world(), "dismounts_underwater", e)
    }

    /// Each ridden mount with the `dismounts_underwater` tag drops its creature rider when that rider's eyes are in water. Only the ridden
    /// mounts are visited, never every entity.
    pub fn dismount_underwater(&mut self) {
        let w = self.world_mut();
        let mounts: Vec<Entity> = w.query_filtered::<Entity, With<Ridden>>().iter(w).collect();
        for m in mounts {
            if !type_tags::named(w, "dismounts_underwater", m) {
                continue;
            }
            let Some(Who::Entity(r)) = w.get::<Mount>(m).and_then(|m| m.rider) else { continue };
            if w.get::<FluidState>(r).is_some_and(|f| f.eye_in_water) {
                mount::dismount(w, m);
            }
        }
    }

    /// The creature riders of `e` take the fall of `distance` blocks: `calculateFallDamage` with the safe fall distance (3), the fall's
    /// `modifier` and the rider's `fall_damage_multiplier` attribute, as a hit of cause `Fall`. A rider tagged `fall_damage_immune` takes none.
    pub fn fall_riders(&mut self, e: Entity, distance: f64, modifier: f32) {
        let Some(m) = self.world().get::<Mount>(e).copied() else { return };
        for who in [m.rider, m.passenger].into_iter().flatten() {
            let Who::Entity(r) = who else { continue };
            if type_tags::named(self.world(), "fall_damage_immune", r) {
                continue;
            }
            let attr = self.attribute(r, "fall_damage_multiplier").unwrap_or(1.0);
            let dmg = ((distance + 1e-6 - 3.0) * f64::from(modifier) * attr).floor() as f32;
            if dmg > 0.0 {
                self.world_mut().resource_mut::<DamageQueue>().add(r, dmg, Cause::Fall);
            }
        }
    }

    /// `AbstractHorse.mobInteract` of the mount `e` by `player` with `item` in hand. A saddle goes on the empty saddle slot of an adult mount
    /// that takes one (a tame one when it has a temper); the item leaves the hand in the host. Otherwise an adult mount with no rider takes the
    /// player (`doPlayerRide`), unless the player sneaks (`Entity.canRide`). True when the interaction took.
    pub fn mount_interact(&mut self, e: Entity, player: u32, item: &str, sneaking: bool) -> bool {
        let id = item_id(item);
        let w = self.world_mut();
        let Some(max_temper) = w.get::<Mount>(e).map(|m| m.max_temper) else { return false };
        let baby = w.get::<GrowAt>(e).is_some();
        let tame_only = max_temper > 0;
        let saddle = !baby
            && matches!(worn_in(id), Some(Slot::Saddle))
            && !w.get::<Equipment>(e).is_some_and(|q| q.saddle != 0)
            && (!tame_only || w.get::<Owner>(e).is_some_and(|o| o.0.is_some()));
        if saddle {
            set_slot(w, e, Slot::Saddle, id);
            view::mark(w, e);
            return true;
        }
        if baby || sneaking {
            return false;
        }
        let ridden = mount::ride(w, e, Who::Player(player));
        if ridden {
            view::mark(w, e);
        }
        ridden
    }

    /// Shears take the saddle off `e`: Some(true) when they did, Some(false) for another item, None when `e` has no saddle. The dropped
    /// saddle is the host's.
    pub fn shear_saddle(&mut self, e: Entity, item: &str) -> Option<bool> {
        let w = self.world_mut();
        if w.get::<Equipment>(e).is_none_or(|q| q.saddle == 0) {
            return None;
        }
        let shears = item_id(item) == goals::shears();
        if shears {
            set_slot(w, e, Slot::Saddle, 0);
            view::mark(w, e);
        }
        Some(shears)
    }

    /// `Entity.ejectPassengers` of `e`: every rider gets off, and the view marks it.
    pub fn refuse_rider(&mut self, e: Entity) {
        let w = self.world_mut();
        mount::eject_passengers(w, e);
        view::mark(w, e);
    }
}
