//! Minecarts (`AbstractMinecart` with `OldMinecartBehavior`): following
//! rails and slopes, powered and activator rails, coming off the track,
//! riding with the rider's push, pushing by players and other carts,
//! `MinecartItem` placement, `VehicleEntity` breaking, the chest, hopper,
//! furnace and TNT variants, saving, and `AbstractMinecartRenderer`'s old
//! rail-following render with the display block.
#[cfg(test)]
#[path = "minecarts/lifetime_tests.rs"]
mod lifetime_tests;
#[path = "minecarts/section_steps.rs"]
mod section_steps;

use super::{Game, Hit, InteractTarget, Keys, entity::Entity};
use crate::ecs_bridge::cart_variants::CartAttacker;
use horizon_ecs_proto::cart_variants::HitResult;
use horizon_client::actions::Action;
use horizon_client::{
    blocks::Fluid,
    containers::BlockEntity,
    items::ItemStack,
    mesher::Vertex,
    mobs::{Species, Who, anim::Poses, riders},
    particles::{ParticleOptions, ParticleType},
    random_source,
    sound_engine::{BLOCKS, CartSound, PlayResult, SoundInstance, Sounds},
};
use horizon_core::{
    geometry::{Aabb, Vec3, trig},
};
use horizon_world::registry::BlockStateId;
#[cfg(test)]
use std::sync::Arc;
#[path = "minecarts/block_effects.rs"]
mod block_effects;
#[path = "minecarts/cells.rs"]
mod cells;
// The cart state and its pure rules live in horizon-gameplay; the old paths keep working.
pub(in crate::game) use horizon_gameplay::minecarts::*;
impl Game<'_> {
    /// `getCurrentBlockPosOrRailBelow`.
    fn rail_block(&self, p: Vec3) -> [i32; 3] {
        let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let below = if self.minecart_improvements() {
            (p.y - 0.1 - f64::from(1.0e-5_f32)).floor() as i32
        } else {
            y - 1
        };
        if self.rail(None, x, below, z).is_some() {
            [x, below, z]
        } else {
            [x, y, z]
        }
    }
    /// `AbstractMinecart.isRedstoneConductor`: the block state's own flag
    /// (soul sand conducts without a full collision shape).
    fn conductor(&self, x: i32, y: i32, z: i32) -> bool {
        self.res.behaviours().facts(self.world.block(x, y, z)).flags
            & horizon_world::level::data::state_flags::CONDUCTOR
            != 0
    }
    /// `OldMinecartBehavior.getPos`: the point on the rail under (x, y, z), on the track the
    /// rail lays for `cart`.
    fn rail_pos(&self, cart: Option<u32>, x: f64, y: f64, z: f64) -> Option<[f64; 3]> {
        let [xt, yt, zt] = self.rail_block(Vec3::new(x, y, z));
        let rail = self.rail(cart, xt, yt, zt)?;
        let (e0, e1) = rail.shape.exits();
        let (fx, fy, fz) = (f64::from(xt), f64::from(yt), f64::from(zt));
        let x0 = fx + 0.5 + f64::from(e0[0]) * 0.5;
        let y0 = fy + 0.0625 + f64::from(e0[1]) * 0.5;
        let z0 = fz + 0.5 + f64::from(e0[2]) * 0.5;
        let x1 = fx + 0.5 + f64::from(e1[0]) * 0.5;
        let y1 = fy + 0.0625 + f64::from(e1[1]) * 0.5;
        let z1 = fz + 0.5 + f64::from(e1[2]) * 0.5;
        let (xd, yd, zd) = (x1 - x0, (y1 - y0) * 2.0, z1 - z0);
        let progress = if xd == 0.0 {
            z - fz
        } else if zd == 0.0 {
            x - fx
        } else {
            ((x - x0) * xd + (z - z0) * zd) * 2.0
        };
        let mut out = [x0 + xd * progress, y0 + yd * progress, z0 + zd * progress];
        if yd < 0.0 {
            out[1] += 1.0;
        } else if yd > 0.0 {
            out[1] += 0.5;
        }
        Some(out)
    }
    /// `OldMinecartBehavior.getPosOffs`.
    fn rail_pos_offs(
        &self,
        cart: Option<u32>,
        mut x: f64,
        y: f64,
        mut z: f64,
        offs: f64,
    ) -> Option<[f64; 3]> {
        let [xt, yt, zt] = self.rail_block(Vec3::new(x, y, z));
        let rail = self.rail(cart, xt, yt, zt)?;
        let mut y = f64::from(yt) + if rail.shape.is_slope() { 1.0 } else { 0.0 };
        let (e0, e1) = rail.shape.exits();
        let (mut xd, mut zd) = (f64::from(e1[0] - e0[0]), f64::from(e1[2] - e0[2]));
        let dd = xd.hypot(zd);
        xd /= dd;
        zd /= dd;
        x += xd * offs;
        z += zd * offs;
        let at = |e: [i32; 3]| x.floor() as i32 - xt == e[0] && z.floor() as i32 - zt == e[2];
        if e0[1] != 0 && at(e0) {
            y += f64::from(e0[1]);
        } else if e1[1] != 0 && at(e1) {
            y += f64::from(e1[1]);
        }
        self.rail_pos(cart, x, y, z)
    }
    /// `AbstractMinecart.canCollideWith`, for `Entity.collide`'s `getEntityCollisions`:
    /// `AbstractBoat.canVehicleCollide` takes any other cart, boat, shulker or pushable mob and
    /// player, except one riding this cart. A cart that runs into one has a horizontal
    /// collision, which sets a TNT minecart off.
    pub(super) fn cart_entity_collisions(&self, c: &Minecart) -> Vec<Aabb> {
        let near = c.bounds().inflate(Vec3::new(2.0, 2.0, 2.0));
        // Every moving cart asks each tick, and the box turns nearly every entity of
        // the level away: it is tested first, and only the mobs near it are read.
        let close = |b: &Aabb| b.intersects(near);
        let mut boxes: Vec<Aabb> = self
            .ecs
            .boats_in(near.min_position(), near.max_position())
            .into_iter()
            .filter_map(|id| self.ecs.bounds(id))
            .filter(close)
            .collect();
        self.mobs_meeting(near, |_, m, b| {
            let counts = m.pushable() || m.species() == Species::Shulker && m.alive();
            if !m.removed && m.extra.mount != Some(c.id) && counts {
                boxes.push(b);
            }
        });
        if self.riding != Some(c.id)
            && let Some(p) = self.player.as_ref()
            && !self.spectator()
            && close(&p.bounds())
        {
            boxes.push(p.bounds());
        }
        if let Some(host) = self.lan.as_ref() {
            boxes.extend(
                host.hub
                    .guests
                    .iter()
                    .filter(|g| {
                        host.hub.here(g)
                            && g.loaded
                            && !g.dead
                            && !g.spectator()
                            && g.vehicle != Some(c.id)
                    })
                    .map(|g| g.bounds())
                    .filter(close),
            );
        }
        boxes
    }
    fn max_speed(&self, c: &Minecart) -> f64 {
        let base = if c.in_water { 0.2 } else { 0.4 };
        furnace_speed(c, base)
    }
    /// `AbstractMinecart.getMaxSpeed`: the behavior's, which a furnace
    /// minecart cuts (`MinecartFurnace.getMaxSpeed`).
    pub(super) fn cart_max_speed(&self, c: &Minecart) -> f64 {
        if self.minecart_improvements() {
            furnace_speed(c, self.max_speed_new(c))
        } else {
            self.max_speed(c)
        }
    }
    /// `applyNaturalSlowdown` (with `MinecartFurnace`'s push); `factor` is
    /// `getSlowdownFactor`: 0.997 riding, the empty cart's own by behavior.
    fn natural_slowdown(&self, c: &mut Minecart, ridden: bool, factor: f64) -> Vec3 {
        let mut v = c.velocity;
        let water = c.in_water;
        if c.kind == CartKind::Furnace {
            if c.push.length_squared() > 1.0e-7 {
                c.push = push_along(c.push, v);
                v = Vec3::new(v.x * 0.8 + c.push.x, 0.0, v.z * 0.8 + c.push.z);
                if water {
                    v = Vec3::new(v.x * 0.1, 0.0, v.z * 0.1);
                }
            } else {
                v = Vec3::new(v.x * 0.98, 0.0, v.z * 0.98);
            }
        }
        if matches!(c.kind, CartKind::Chest | CartKind::Hopper) {
            // `AbstractMinecartContainer.applyNaturalSlowdown` replaces the
            // base one whole: the keep factor leans on how empty it is, and
            // riders and the behavior's own factor do not enter.
            let signal = Minecart::container_signal(&c.items, self.items());
            let keep = container_keep(signal, c.loot.is_some(), water);
            return Vec3::new(v.x * keep, 0.0, v.z * keep);
        }
        let f = if ridden { 0.997 } else { factor };
        let mut out = Vec3::new(v.x * f, 0.0, v.z * f);
        if water {
            // scale(0.95F): the float widened.
            let drag = f64::from(0.95_f32);
            out = Vec3::new(out.x * drag, 0.0, out.z * drag);
        }
        out
    }
    /// `Entity.isVehicle` for a cart: the local player, a LAN guest or a mob
    /// sits in it. `getSlowdownFactor` (0.997) and the 0.75 speed scale key on
    /// this, not on who rides.
    fn cart_is_vehicle(&self, id: u32) -> bool {
        self.riding == Some(id) || self.ridden_by_guest(id) || self.mob_rides(id)
    }
    /// `AbstractMinecart`: `minecart_improvements` picks the smoother
    /// physics.
    pub(super) fn minecart_improvements(&self) -> bool {
        self.enabled_feature_ids()
            .iter()
            .any(|f| f == "minecraft:minecart_improvements")
    }
    /// `NewMinecartBehavior.getMaxSpeed`: the game rule over water's half,
    /// every cart alike (`cart_max_speed` cuts a furnace minecart's).
    fn max_speed_new(&self, c: &Minecart) -> f64 {
        f64::from(
            self.ui
                .rules
                .int_at(horizon_client::gamerules::vanilla::MAX_MINECART_SPEED),
        ) * if c.in_water { 0.5 } else { 1.0 }
            / 20.0
    }
    /// `AbstractMinecart.getRedstoneDirection` (over the old port's
    /// conductor proxy): which way a powered rail pushes a resting cart.
    fn redstone_direction(&self, pos: [i32; 3], shape: Shape) -> Vec3 {
        let (dx, dz) = match shape {
            Shape::NorthSouth => (0, 1),
            Shape::EastWest => (1, 0),
            _ => return Vec3::ZERO,
        };
        // The push points away from the signal's side (getRedstoneDirection).
        if self.conductor(pos[0] - dx, pos[1], pos[2] - dz) {
            Vec3::new(f64::from(dx), 0.0, f64::from(dz))
        } else if self.conductor(pos[0] + dx, pos[1], pos[2] + dz) {
            Vec3::new(f64::from(-dx), 0.0, f64::from(-dz))
        } else {
            Vec3::ZERO
        }
    }
    /// `NewMinecartBehavior.isDecending`: the movement goes down the
    /// shape's rise.
    fn is_descending(movement: Vec3, shape: Shape) -> bool {
        match shape {
            Shape::AscendingEast => movement.x < 0.0,
            Shape::AscendingWest => movement.x > 0.0,
            Shape::AscendingNorth => movement.z > 0.0,
            Shape::AscendingSouth => movement.z < 0.0,
            _ => false,
        }
    }
    /// `MinecartTNT.primeFuse`: fuse 80, then `Level.broadcastEntityEvent`
    /// (`EntityEvent.TNT_PRIME`, 70) and, unless the cart is silent,
    /// `SoundEvents.TNT_PRIMED` on `SoundSource.BLOCKS`. A client that
    /// receives 70 only sets the fuse (`handleEntityEvent`).
    pub(super) fn prime_cart(&mut self, c: &mut Minecart) {
        if !self
            .ui
            .rules
            .bool_at(horizon_client::gamerules::vanilla::TNT_EXPLODES)
        {
            return;
        }
        c.fuse = 80;
        let at = [c.position.x, c.position.y, c.position.z];
        let id = c.id as i32;
        self.lan_entity_event(id, TNT_PRIME_EVENT, at);
        if !self.entity_silent(c.id) {
            self.audio
                .play_on(BLOCKS, "minecraft:entity.tnt.primed", Some(at), 1.0, 1.0);
        }
    }
    /// `positionRider` for the mobs in minecarts: each sits on the cart's
    /// passenger attachment, less its own vehicle attachment.
    pub(super) fn seat_cart_mobs(&mut self, riding: &super::passengers::Riders) {
        // The riders of an ECS cart sit where P puts them: nothing to seat here.
        if self.ecs.is_active() {
            return;
        }
        for i in 0..self.carts.len() {
            let id = self.carts[i].id;
            let Some(js) = riding.get(&id) else {
                continue;
            };
            let (pos, prev) = (self.carts[i].position, self.carts[i].previous);
            let cart_box = self.carts[i].bounds();
            let seats: Vec<(usize, f64, f64)> = js
                .iter()
                .map(|&j| {
                    let m = &self.remote_entities[j];
                    // Minecart's box is 0.98 wide.
                    let attachment = riders::vehicle_attachment(m, 0.98);
                    (j, attachment, cart_seat_offset(m.species(), attachment))
                })
                .collect();
            for (j, attachment, offset) in seats {
                let m = &mut self.remote_entities[j];
                m.seated(id, attachment, cart_box);
                m.position = cart_seat(pos, offset);
                m.previous = cart_seat(prev, offset);
                m.velocity = Vec3::ZERO;
                m.fall_distance = 0.0;
                m.on_ground = false;
                m.extra.passenger = true;
                m.ai.nav.stop();
            }
        }
    }
    /// `ejectPassengers` for the mobs in cart `i`.
    pub(super) fn eject_cart_mobs(&mut self, id: u32) {
        let Some(y) = self.ecs_view(id).map(|v| v.position[1]) else {
            return;
        };
        for (_, rider, player) in self.seated_passengers(id) {
            if player
                || self
                    .mob_object(rider)
                    .is_none_or(|mob| mob.removed || mob.extra.mount != Some(id))
            {
                continue;
            }
            self.dismount_cart_mob_at(rider, y);
        }
    }

    /// `ClientPacketListener.postAddEntitySoundInstance` for the carts that
    /// joined the level since the last tick, and `LocalPlayer.startRiding`'s
    /// two inside sounds when the player got in one; then what the sound
    /// instances read this tick (the ridden cart and every cart).
    pub(super) fn start_cart_sounds(&mut self) -> (Option<u32>, Vec<CartSound>) {
        let carts: Vec<CartSound> = self
            .carts
            .iter()
            .map(|c| CartSound {
                id: c.id,
                position: [c.position.x, c.position.y, c.position.z],
                // Vec3.horizontalDistance.
                speed: (c.velocity.x * c.velocity.x + c.velocity.z * c.velocity.z).sqrt(),
            })
            .collect();
        self.cart_sounds
            .retain(|id| carts.iter().any(|c| c.id == *id));
        for cart in &carts {
            if !self.cart_sounds.contains(&cart.id) {
                let instance = SoundInstance::minecart(cart, random_source::create());
                let (_, res) = self.audio.play_instance(instance, false);
                if res != PlayResult::NotStarted {
                    self.cart_sounds.insert(cart.id);
                }
            }
        }
        let vehicle = self.riding.filter(|id| carts.iter().any(|c| c.id == *id));
        if vehicle != self.riding_cart_sounds
            && let Some(cart) = carts.iter().find(|c| Some(c.id) == vehicle)
        {
            let mut started = true;
            for underwater in [true, false] {
                let instance =
                    SoundInstance::riding_minecart(cart, underwater, random_source::create());
                let (_, res) = self.audio.play_instance(instance, false);
                if res == PlayResult::NotStarted {
                    started = false;
                }
            }
            if started {
                self.riding_cart_sounds = vehicle;
            }
        } else if vehicle.is_none() {
            self.riding_cart_sounds = None;
        }
        (vehicle, carts)
    }
    /// The rider's tick: the cart moves with the rider's input as its push.
    pub(super) fn tick_cart_riding(&mut self, keys: Keys) -> bool {
        let Some(id) = self.riding.filter(|&id| self.ecs.cart_kind(id).is_some()) else {
            return false;
        };
        if keys.shift {
            self.dismount_cart(id);
            return true;
        }
        // ServerPlayer.getLastClientMoveIntent: getInputVector((left, 0, forward), 1, yRot).
        let left = f64::from(i8::from(keys.left) - i8::from(keys.right));
        let forward = f64::from(i8::from(keys.forward) - i8::from(keys.backward));
        let len = left.hypot(forward);
        let (left, forward) = if len > 1.0 {
            (left / len, forward / len)
        } else {
            (left, forward)
        };
        // The cart moves in P with this input as its push; the rider is seated where the cart is.
        if let Ok(player) = u32::try_from(self.own_entity_id()) {
            self.ecs.pilot_input(id, player, forward as f32, left as f32, self.yaw, self.pitch, 0.0, false, false, true);
        }
        let step = section_steps::seat_rider(self, id);
        section_steps::tell_player(self, step);
        true
    }
    /// `getDismountLocationForPassenger`: beside the cart's motion direction,
    /// else on top.
    pub(super) fn dismount_cart(&mut self, id: u32) {
        if self.riding == Some(id) && !self.native_may_leave(id) {
            return;
        }
        self.riding = None;
        let Some(at) = self.ecs_view(id).map(|v| Vec3::new(v.position[0], v.position[1], v.position[2])) else {
            return;
        };
        if self.dismount_in_portal(at) {
            return;
        }
        let target = self.cart_player_dismount_target(id);
        let from = self.player.as_ref().map(|player| player.position);
        if let Some(p) = self.player.as_mut() {
            p.position = target;
            p.velocity = Vec3::ZERO;
        }
        section_steps::tell_player(self, from.map(|from| (from, target)));
    }
    /// Where the cart at `i` lets a player off.
    pub(super) fn cart_player_dismount_target(&self, id: u32) -> Vec3 {
        let Some(v) = self.ecs_view(id) else {
            return Vec3::ZERO;
        };
        let [px, py, pz] = v.position;
        let (bx, by, bz) = (px.floor() as i32, py.floor() as i32, pz.floor() as i32);
        let mut target = None;
        let flipped = self.ecs.cart_flipped(id).unwrap_or(false);
        let face = cart_motion_face(v.yaw, flipped, self.minecart_improvements());
        'search: for dy in [0, 1, -1] {
            for (dx, dz) in dismount_offsets(face) {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                let below = self.res.facts().get(self.world.block(x, y - 1, z));
                let here = self.res.facts().get(self.world.block(x, y, z));
                let head = self.res.facts().get(self.world.block(x, y + 1, z));
                if (below.full_collision || below.legacy_solid)
                    && !here.full_collision
                    && !head.full_collision
                    && here.fluid == Fluid::None
                {
                    target = Some(Vec3::new(
                        f64::from(x) + 0.5,
                        f64::from(y),
                        f64::from(z) + 0.5,
                    ));
                    break 'search;
                }
            }
        }
        target.unwrap_or(Vec3::new(px, py + HEIGHT, pz))
    }
    /// `MinecartItem.useOn`: a cart on the clicked rail.
    pub(super) fn place_minecart(&mut self, hit: &Hit, stack: &ItemStack) -> bool {
        let Some(kind) = CartKind::from_id(&stack.item) else {
            return false;
        };
        let Some(rail) = self.rail(None, hit.pos[0], hit.pos[1], hit.pos[2]) else {
            return false;
        };
        let offset = if rail.shape.is_slope() { 0.5 } else { 0.0 };
        let at = Vec3::new(
            f64::from(hit.pos[0]) + 0.5,
            f64::from(hit.pos[1]) + 0.0625 + offset,
            f64::from(hit.pos[2]) + 0.5,
        );
        let mut next = self.next_entity;
        let spawned = self.ecs_with(|ecs, mw| {
            ecs.spawn_cart_id(mw, kind.id(), [at.x, at.y, at.z], 0.0, &mut || {
                next += 1;
                next
            })
        });
        self.next_entity = next;
        let Some(id) = spawned else {
            return false;
        };
        self.ecs.cart_start(id);
        self.world
            .modified
            .insert((hit.pos[0] >> 4, hit.pos[2] >> 4));
        self.apply_item_stack_config(super::commands::Target::Cart(id), kind.id(), stack, true);
        if self.serving.is_some() || self.options.dedicated.is_some() {
            self.native_vehicle_create(kind.id(), [at.x, at.y, at.z]);
        }
        // MinecartItem.useOn: ENTITY_PLACE at the rail, by the player, with the block under it.
        let below = self.world.block(hit.pos[0], hit.pos[1] - 1, hit.pos[2]);
        self.post_block_game_event(
            "minecraft:entity_place",
            hit.pos,
            self.sculk_actor(),
            Some(below),
        );
        if !self.creative {
            self.consume_held();
        }
        // ItemStack.useOn counts the use.
        self.award_stat("used", &stack.item, 1);
        self.swing_arm();
        true
    }
    /// The cart the aim enters first within reach, by its network id; the cart the player rides is passed over.
    pub(super) fn cart_hit(&self) -> Option<(u32, f64)> {
        let (eye, dir) = self.aim()?;
        self.ecs.aimed_cart_except(Vec3::new(eye[0], eye[1], eye[2]), Vec3::new(dir[0], dir[1], dir[2]), self.entity_reach, self.riding)
    }
    /// The slots of the cart `id`, for a command: `getChestVehicleItem` and
    /// `setChestVehicleItem` first unpack its loot table with no player, so
    /// no `player_generates_container_loot` fires later. While its menu is
    /// open the live slots are the placeholder copy (`open_cart_container`).
    pub(super) fn edit_cart_slots<R>(
        &mut self,
        id: u32,
        edit: impl FnOnce(&mut Vec<Option<ItemStack>>) -> R,
    ) -> Option<R> {
        self.ecs.cart_kind(id)?;
        self.unpack_loot(super::container_loot::LootTarget::Cart(id), None);
        match self
            .world
            .block_entities
            .get_mut(&(i32::MAX - 15, CART_KEY_Y, id as i32))
        {
            Some(BlockEntity::Chest(open)) => Some(edit(open)),
            _ => {
                let mut items = self.ecs.cart_items(id)?;
                let r = edit(&mut items);
                for (slot, stack) in items.into_iter().enumerate() {
                    self.ecs.cart_set_slot(id, slot, stack);
                }
                Some(r)
            }
        }
    }
    fn unpack_cart_loot(&mut self, id: u32) {
        self.unpack_loot(
            super::container_loot::LootTarget::Cart(id),
            self.native_acting_player(),
        );
    }
    /// `interact`: ride, open or fuel the cart under the crosshair, or the one
    /// a guest's interact packet named. A TNT cart has no interact of its own
    /// (`Entity.interact` passes), so nothing here lights it.
    pub(super) fn interact_cart(&mut self, target: Option<InteractTarget>) -> bool {
        let Some((id, d)) = (match target {
            Some(t) => u32::try_from(t.id).ok().and_then(|id| self.ecs.cart_kind(id).map(|_| (id, 0.0))),
            None => self.cart_hit(),
        }) else {
            return false;
        };
        // Only a client's own pick is stopped by a block: the range check passed.
        if target.is_none() && !self.before_block(d) {
            return false;
        }
        let Some(kind) = self.cart_genre(id) else {
            return false;
        };
        let sneaking = self.keys.held(Action::Sneak);
        let held = self.held().cloned();
        match kind {
            CartKind::Rideable => {
                // isVehicle: a mob already sits in it. A player in another
                // vehicle may board: `Entity.startRiding` leaves the old one.
                let has_mob = !self.seated_passengers(id).is_empty();
                if sneaking || self.ridden_by_guest(id) || has_mob {
                    return false;
                }
                self.start_riding_target(
                    super::commands::Target::Player,
                    super::commands::Target::Cart(id),
                    false,
                );
            }
            // AbstractMinecartContainer.interact: no sneak check.
            CartKind::Chest | CartKind::Hopper => {
                let Some(player) = self.acting_player_uuid_id() else {
                    return false;
                };
                self.open_cart_container(id, player);
            }
            CartKind::Furnace => {
                let fuel = held
                    .as_ref()
                    .is_some_and(|s| matches!(&*s.item, "minecraft:coal" | "minecraft:charcoal"));
                let fueled = fuel && self.ecs.cart_feed(id);
                if fueled && !self.creative {
                    self.consume_held();
                }
            }
            // MinecartTNT has no interact of its own (`Entity.interact` passes).
            CartKind::Tnt => return false,
            // MinecartCommandBlock.interact: a game master gets SUCCESS (the
            // client opens the editor, `open_cart_command_block`), anyone else
            // PASSes to the held item. SUCCESS swings on the client only.
            CartKind::CommandBlock => return self.can_use_game_master_blocks(),
            // MinecartSpawner has no interact of its own (`Entity.interact` passes).
            CartKind::Spawner => return false,
        }
        self.swing_arm();
        true
    }
    /// `AbstractMinecartContainer.interact`: the menu opens over the cart's
    /// own slots, kept under a placeholder key until it closes
    /// (`store_cart_items`/`close_cart_container`).
    pub(super) fn open_cart_container(&mut self, id: u32, _player: u32) {
        self.unpack_cart_loot(id);
        let (Some(kind), Some(items), Some(at)) = (
            self.cart_genre(id),
            self.ecs.cart_items(id),
            self.ecs_view(id).map(|v| v.position),
        ) else {
            return;
        };
        let key = (i32::MAX - 15, CART_KEY_Y, id as i32);
        // `startOpen` shares one container, it does not reset it: a stand-in
        // another player's menu is still editing over stays as it is.
        if !self.world.block_entities.contains_key(&key) {
            self.world
                .block_entities
                .insert(key, BlockEntity::Chest(items));
        }
        self.container = Some(key);
        self.set_screen(super::Screen::Inventory);
        self.capture(false);
        self.right = false;
        if kind == CartKind::Chest {
            // `MinecartChest.interact` (which overrides
            // `AbstractMinecartContainer.interact`): a successful open fires
            // CONTAINER_OPEN with the player and angers the piglins that see
            // it. `MinecartHopper` does not override it, so a hopper minecart
            // fires neither. Vanilla plays no open sound for a chest minecart.
            let ctx = self.player_context(None);
            self.post_game_event("minecraft:container_open", at, ctx);
            self.anger_piglins(true);
        }
    }
    /// Closing the cart's container open under `key` copies the items back.
    pub(super) fn close_cart_container(&mut self, key: (i32, i32, i32)) {
        if key.1 != CART_KEY_Y {
            return;
        }
        self.store_cart_items(key);
        let guest_viewers = self
            .lan
            .as_ref()
            .is_some_and(|host| Self::vehicle_has_other_viewers(None, &host.hub.guests, None, key));
        if !guest_viewers {
            self.world.block_entities.remove(&key);
        }
    }
    /// A chest or hopper minecart or a chest boat is about to be removed
    /// (`Containers.dropContents` / `AbstractChestBoat.remove`): what an open
    /// menu edited goes back to the vehicle, so it is what drops or travels
    /// with it, and the menu's stand-in leaves the level. A menu that is
    /// still open over it then has nothing left to take.
    pub(super) fn release_vehicle_container(&mut self, id: u32) {
        let key = (i32::MAX - 15, CART_KEY_Y, id as i32);
        self.store_cart_items(key);
        self.world.block_entities.remove(&key);
        if self.container == Some(key) {
            self.container = None;
        }
    }
    /// The menu of a chest or hopper minecart or a chest boat works on the
    /// vehicle's own items (`ContainerEntity`): the placeholder under `key`
    /// that the menu edits is copied back to the vehicle.
    pub(super) fn store_cart_items(&mut self, key: (i32, i32, i32)) {
        if key.1 != CART_KEY_Y {
            return;
        }
        let Some(BlockEntity::Chest(items)) = self.world.block_entities.get(&key) else {
            return;
        };
        let items = items.clone();
        let id = key.2 as u32;
        if self.ecs.cart_kind(id).is_some() {
            for (slot, stack) in items.into_iter().enumerate() {
                self.ecs.cart_set_slot(id, slot, stack);
            }
        } else {
            for (slot, stack) in items.into_iter().enumerate() {
                self.ecs.boat_set_slot(id, slot, stack);
            }
        }
    }
    /// The open cart's kind when a cart container is shown.
    pub(super) fn open_cart_kind(&self) -> Option<CartKind> {
        let key = self.container.filter(|k| k.1 == CART_KEY_Y)?;
        // A chest boat's chest shows like a chest minecart's.
        self.cart_genre(key.2 as u32).or_else(|| {
            (self.boat_has_chest(key.2 as u32) == Some(true)).then_some(CartKind::Chest)
        })
    }
    /// The genre of the cart `id`, None when it is not a cart.
    pub(super) fn cart_genre(&self, id: u32) -> Option<CartKind> {
        CartKind::from_id(self.ecs.cart_kind(id)?)
    }
    /// An arrow hit cart `i` (`MinecartTNT.hurtServer`, then
    /// `VehicleEntity.hurtServer`): a burning arrow sets off a TNT minecart.
    /// `owner` is `AbstractArrow.getOwner`, the hit's `DamageSource.getEntity`;
    pub(super) fn arrow_hits_cart(
        &mut self,
        id: u32,
        damage: f32,
        arrow_velocity: Vec3,
        burning: bool,
        creative: bool,
        owner: Option<Who>,
        guest: Option<u32>,
    ) {
        let Some(kind) = self.cart_genre(id) else {
            return;
        };
        if burning && kind == CartKind::Tnt {
            self.ecs.cart_ignite(id, Some(arrow_velocity.length_squared() as f32));
            return;
        }
        self.projectile_cart_hit(id, damage, creative, owner, guest);
    }
    /// A projectile's blow on the cart `id` (`CartHit.projectile`): its owner is the credit, a guest's connection when it is one.
    pub(super) fn projectile_cart_hit(&mut self, id: u32, damage: f32, creative: bool, owner: Option<Who>, guest: Option<u32>) {
        let attacker = match owner {
            Some(Who::Player) => guest
                .or_else(|| self.acting_player_uuid_id())
                .map(CartAttacker::Player),
            Some(Who::Mob(m)) => Some(CartAttacker::Mob(m)),
            None => None,
        };
        self.ecs_with(|ecs, mw| ecs.cart_hit(mw, id, damage, attacker, creative));
    }
    /// The player's own blow on the cart `id` (`Player.attack`): a fire aspect lights it first. True when the cart was hurt.
    pub(super) fn cart_player_hurt(&mut self, id: u32, attacker: i32, damage: f32, fire: bool) -> bool {
        if fire {
            self.ecs.cart_ignite(id, None);
        }
        let creative = self.creative;
        let attacker = Some(CartAttacker::Player(attacker as u32));
        matches!(
            self.ecs_with(|ecs, mw| ecs.cart_hit(mw, id, damage, attacker, creative)),
            HitResult::Hurt
        )
    }
    /// `Containers.dropContents(level, entity, container)` at a vehicle's
    /// position `at`: each slot (an empty one too) from a spot in its block,
    /// in pieces of 10 to 30 with the level random's scatter, no pickup
    /// delay. `AbstractChestBoat.remove` and `AbstractMinecartContainer.remove`
    /// spill this way for a destroying removal (kill or discard), and
    /// `chestVehicleDestroyed` again for the emptied container.
    pub(in crate::game) fn drop_vehicle_contents(
        &mut self,
        at: [f64; 3],
        items: &[Option<ItemStack>],
    ) {
        for stack in items {
            let count = stack.as_ref().map_or(0, |s| s.count);
            let pieces = horizon_world::level::removal::drop_item_stack_at(
                &mut self.world.random,
                at,
                count,
            );
            if let Some(stack) = stack {
                for piece in pieces {
                    let piece_stack = ItemStack {
                        count: piece.count,
                        ..stack.clone()
                    };
                    self.spawn_item(piece_stack, piece.pos, piece.motion, 0);
                }
            }
        }
    }
    /// Baked block-state faces with default block tints and terrain shade or diffuse lighting.
    pub(super) fn block_model_vertices(
        &self,
        state: BlockStateId,
        seed: i64,
        m: glam::Mat4,
        light: u32,
        diffuse: bool,
        out: &mut Vec<Vertex>,
    ) {
        // BlockTintSource.color(state) with no level: each tinted face's default colour.
        let grass = self.pack.item_renderer.grass_color(0.5, 1.0) as u32;
        let tables = &self.pack.context.tables;
        let tint = |index: i32| tables.default_tint(state, index, grass);
        self.tinted_block_model_vertices(state, seed, m, |_| light, diffuse, tint, out);
    }

    /// The block the cart shows: `None` for air (`AbstractMinecartRenderer`
    /// draws and stretches its culling box only for a non-air state).
    fn display_state(&self, c: &Minecart) -> Option<BlockStateId> {
        shown_unless_air(self.display_state_unfiltered(c), |s| {
            self.res.facts().is_air(s)
        })
    }
    fn display_state_unfiltered(&self, c: &Minecart) -> Option<BlockStateId> {
        let registry = &self.pack.assets.registry;
        let with = |name: &str, props: &[(&str, &str)]| {
            let mut s = registry.state(registry.default_state(name)?)?.clone();
            for (k, v) in props {
                s.properties.insert((*k).to_owned(), (*v).to_owned());
            }
            registry.id(&s)
        };
        // AbstractMinecart.getDisplayBlockState: a custom state wins.
        if let Some(state) = c
            .custom_display
            .as_ref()
            .and_then(|tag| horizon_client::entity_nbt::state_from_nbt(registry, tag))
        {
            return Some(state);
        }
        match c.kind {
            CartKind::Rideable => None,
            CartKind::Chest => with("minecraft:chest", &[("facing", "north")]),
            CartKind::Hopper => registry.default_state("minecraft:hopper"),
            CartKind::Furnace => with(
                "minecraft:furnace",
                &[
                    ("facing", "north"),
                    ("lit", if c.fuel > 0 { "true" } else { "false" }),
                ],
            ),
            CartKind::Tnt => registry.default_state("minecraft:tnt"),
            CartKind::CommandBlock => registry.default_state("minecraft:command_block"),
            // MinecartSpawner.getDefaultDisplayBlockState.
            CartKind::Spawner => registry.default_state("minecraft:spawner"),
        }
    }
}

#[cfg(test)]
#[path = "minecarts/minecart_tnt_tests.rs"]
mod minecart_tnt_tests;

/// Local geometry with atlas coordinates resolved for both CPU and GPU consumers.
#[derive(Clone, Copy)]
pub(super) struct PreparedModelFace {
    positions: [[f32; 3]; 4],
    uv: [[f32; 2]; 4],
    pub(super) facing: horizon_assets::model::Direction,
    pub(super) tint_index: i32,
    light_emission: u8,
}

impl PreparedModelFace {
    pub(super) fn new(
        quad: &horizon_assets::bake::ModelQuad,
        sprite: horizon_client::hud::Rect,
    ) -> Self {
        Self {
            positions: quad.positions,
            uv: quad.uv.map(|uv| {
                [
                    sprite.u0 + uv[0] * (sprite.u1 - sprite.u0),
                    sprite.v0 + uv[1] * (sprite.v1 - sprite.v0),
                ]
            }),
            facing: quad.facing,
            tint_index: quad.tint_index,
            light_emission: quad.light_emission,
        }
    }

    pub(super) fn positions(&self) -> [[f32; 3]; 4] {
        self.positions
    }

    pub(in crate::game) fn gpu_vertex(
        &self,
        corner: usize,
    ) -> horizon_client::gfx::PreparedModelVertex {
        let normal = horizon_client::item_render::direction_vector(self.facing);
        horizon_client::gfx::PreparedModelVertex {
            pos: [
                self.positions[corner][0],
                self.positions[corner][1],
                self.positions[corner][2],
                1.0,
            ],
            uv: [self.uv[corner][0], self.uv[corner][1], 0.0, 0.0],
            normal: [normal.x, normal.y, normal.z, 0.0],
            flags: [
                u32::from(self.tint_index >= 0),
                u32::from(self.light_emission),
                0,
                0,
            ],
        }
    }
}

/// Shared transform, shading and light path for block-state and prepared standalone faces.
pub(super) fn emit_model_faces(
    faces: impl IntoIterator<Item = (PreparedModelFace, u32)>,
    m: glam::Mat4,
    diffuse: bool,
    tint: impl Fn(i32) -> Option<u32>,
    out: &mut Vec<Vertex>,
) {
    use horizon_assets::model::Direction as D;
    let mut normals = horizon_client::item_render::FaceNormals::new(m);
    for (quad, light) in faces {
        let shade: u32 = match quad.facing {
            D::Up => 255,
            D::Down => 127,
            D::North | D::South => 204,
            D::West | D::East => 153,
        };
        let mut color = if diffuse {
            u32::MAX
        } else {
            0xFF00_0000 | shade << 16 | shade << 8 | shade
        };
        if let Some(t) = tint(quad.tint_index) {
            color =
                horizon_client::hud_layout::multiply(color, horizon_client::mobs::render::abgr(t));
        }
        let normal = if diffuse {
            let n = normals
                .face(quad.facing)
                .to_array()
                .map(|v| (v.clamp(-1.0, 1.0) * 127.0) as i8);
            [n[0], n[1], n[2], 127]
        } else {
            [0; 4]
        };
        let light = if diffuse {
            let emission = u32::from(quad.light_emission) * 16;
            (light & 0xFF) | (light >> 8 & 0xFF).max(emission) << 8
        } else {
            light
        };
        for k in 0..4 {
            let uv = quad.uv[k];
            out.push(Vertex {
                normal,
                pos: m
                    .transform_point3(glam::Vec3::from_array(quad.positions[k]))
                    .to_array(),
                uv,
                layer: 0,
                color,
                light,
                glint_uv: [0.0, 0.0],
            });
        }
    }
}

/// Vanilla multiplies tint, alpha included, only when `tintIndex != -1`.
pub(in crate::game) fn scale_tint_alpha(
    vertices: &mut [Vertex],
    faces: &[PreparedModelFace],
    tint: u32,
) {
    let alpha = tint >> 24;
    if alpha == 255 {
        return;
    }
    let (quads, _) = vertices.as_chunks_mut::<4>();
    for (face, quad) in faces.iter().zip(quads) {
        if face.tint_index < 0 {
            continue;
        }
        for vertex in quad {
            let scaled = (vertex.color >> 24) * alpha / 255;
            vertex.color = vertex.color & 0x00FF_FFFF | scaled << 24;
        }
    }
}

#[cfg(test)]
#[path = "minecarts/tests.rs"]
mod tests;

#[cfg(test)]
mod tnt_impact_tests {
    use super::{CartKind, Minecart};
    use crate::game::native_entity_refs::tests::with_game;
    use horizon_core::geometry::Vec3;

    /// Ticks a moving TNT cart until it stops or blows up; true when it
    /// blew up.
    fn runs_into_the_wall(rails: bool, at: Vec3, velocity: Vec3) -> bool {
        let mut result = false;
        with_game(|game| {
            game.world.chunks.insert(
                (0, 0),
                std::sync::Arc::new(horizon_client::chunk::Chunk::empty(0, 0)),
            );
            let registry = game.pack.assets.registry;
            let stone = registry.default_state("minecraft:stone").unwrap();
            let rail = registry.default_state("minecraft:rail").unwrap();
            let mut east_west = registry.state(rail).unwrap().clone();
            east_west
                .properties
                .insert("shape".into(), "east_west".into());
            let rail = registry.id(&east_west).unwrap();
            for x in 0..16 {
                for z in 0..16 {
                    game.world.set(x, 63, z, stone).unwrap();
                }
            }
            for z in 0..16 {
                game.world.set(9, 64, z, stone).unwrap();
                game.world.set(9, 65, z, stone).unwrap();
            }
            if rails {
                for x in 1..9 {
                    game.world.set(x, 64, 8, rail).unwrap();
                }
            }
            let mut cart = Minecart::new(1, CartKind::Tnt, at);
            cart.velocity = velocity;
            for _ in 0..40 {
                if !game.tick_cart(&mut cart, [0.0; 2], 0, &mut super::CartIndex::default()) {
                    result = true;
                    break;
                }
            }
        });
        result
    }

    #[test]
    #[ignore = "daily"]
    fn a_powered_rail_pushes_a_resting_cart_away_from_soul_sand() {
        let mut resources = None;
        let mut game = crate::game::game_tests::tests::server_game(&mut resources);
        let registry = game.pack.assets.registry;
        let rail = registry.default_state("minecraft:powered_rail").unwrap();
        let mut state = registry.state(rail).unwrap().clone();
        state.properties.insert("shape".into(), "east_west".into());
        state.properties.insert("powered".into(), "true".into());
        let rail = registry.id(&state).unwrap();
        game.world.set(5, 64, 8, rail).unwrap();
        for (block, pushed) in [
            ("minecraft:soul_sand", true),
            ("minecraft:stone", true),
            ("minecraft:oak_slab", false),
        ] {
            let id = registry.default_state(block).unwrap();
            game.world.set(4, 64, 8, id).unwrap();
            let push = game.redstone_direction([5, 64, 8], super::Shape::EastWest);
            assert_eq!(push.x == 1.0, pushed, "{block}");
        }
    }

    #[test]
    #[ignore = "daily"]
    fn a_tnt_cart_stopped_head_on_by_a_wall_does_not_blow_up() {
        // OldMinecartBehavior.moveAlongTrack: the move zeroes the blocked x
        // and the track's re-pointing scales what is left, so MinecartTNT
        // reads no speed at the wall.
        assert!(!runs_into_the_wall(
            true,
            Vec3::new(3.5, 64.0, 8.5),
            Vec3::new(0.4, 0.0, 0.0)
        ));
    }

    #[test]
    #[ignore = "daily"]
    fn a_tnt_cart_off_its_rails_glancing_off_a_wall_blows_up() {
        // comeOffTrack keeps the delta the move resolved: the wall zeroes x,
        // z goes on at over a tenth of a block a tick.
        assert!(runs_into_the_wall(
            false,
            Vec3::new(8.3, 64.0, 4.5),
            Vec3::new(0.4, 0.0, 0.4)
        ));
    }

}

#[cfg(test)]
mod cart_push_tests {
    use super::{CartKind, Minecart};
    use horizon_client::{items::ItemStack, save::PendingContainerLoot};
    use horizon_core::geometry::Vec3;

}

#[cfg(test)]
mod hook_mutation_regressions {
    use super::*;
    use horizon_client::mobs::Mob;
    use std::sync::{Arc, Mutex};

    struct MutateRider {
        discard: bool,
        heard: Arc<Mutex<Vec<(u32, bool)>>>,
    }
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for MutateRider {
        fn entity_mount(
            &self,
            host: &mut dyn mod_api::ServerHost,
            event: &mut mod_api::EntityMountEvent<'_>,
        ) -> mod_api::Flow {
            self.heard
                .lock()
                .unwrap()
                .push((event.entity, event.mounting));
            if !event.mounting && event.entity == 72 {
                if self.discard {
                    assert!(host.entity(72).unwrap().discard(host));
                } else {
                    let later = host.entity(73).unwrap();
                    let replacement = host.entity(74).unwrap();
                    assert!(later.start_riding(host, replacement, true));
                }
            }
            mod_api::Flow::Continue
        }
    }
    struct DiscardDamagedVehicle(Arc<Mutex<usize>>);
    #[mod_api::__subscriptions]
    impl mod_api::Hooks for DiscardDamagedVehicle {
        fn vehicle_damage(
            &self,
            host: &mut dyn mod_api::ServerHost,
            _: &mut mod_api::VehicleDamageEvent<'_>,
        ) -> mod_api::Flow {
            assert!(host.entity(71).unwrap().discard(host));
            mod_api::Flow::Continue
        }
        fn vehicle_destroy(
            &self,
            _: &mut dyn mod_api::ServerHost,
            _: &mut mod_api::VehicleDestroyEvent<'_>,
        ) {
            *self.0.lock().unwrap() += 1;
        }
    }
    #[test]
    #[ignore = "daily"]
    fn a_damage_callback_discard_does_not_also_destroy_or_drop_the_vehicle() {
        let destroyed = Arc::new(Mutex::new(0));
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
            "discard_damaged",
            Box::new(DiscardDamagedVehicle(destroyed.clone())),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            super::super::native_entity_refs::tests::with_game(|game| {
                game.options.dedicated = Some(Vec::new());
                game.ui
                    .rules
                    .set("entity_drops", horizon_client::gamerules::Value::Bool(true));
                let boat = game.ecs_spawn("minecraft:oak_boat", [0.0; 3], None).unwrap();
                game.hurt_boat(boat, 5.0, false);
                assert_eq!(*destroyed.lock().unwrap(), 0);
                assert!(
                    game.dropped.is_empty(),
                    "discard must not create the vehicle item"
                );
                game.native_removals_flush();
                assert_eq!(*destroyed.lock().unwrap(), 0);
                assert!(game.dropped.is_empty());
                assert!(game.entity(boat).is_none());
            });
        });
    }
}

#[cfg(test)]
mod tnt_cart_power_tests {
    use super::*;

}

#[cfg(test)]
mod cart_cost_bench {
    use super::{CartKind, Minecart};
    use crate::game::game_tests::tests::server_game;
    use horizon_core::geometry::Vec3;

}

#[cfg(test)]
#[path = "minecarts/minecart_riders_tests.rs"]
mod minecart_riders_tests;
