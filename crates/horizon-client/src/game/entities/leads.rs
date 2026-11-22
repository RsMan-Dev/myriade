//! Leads on the game side: putting them on and taking them off mobs
//! (`Entity.interact`'s leash part), fence knots (`LeadItem.useOn` and
//! `LeashFenceKnotEntity.interact`), loading and saving who holds what,
//! breaking, and drawing the rope (`LeashFeatureRenderer`) and the knot.
use super::{Game, commands::Target};
use horizon_client::mobs::MobEvent;
use horizon_client::{
    items::ItemStack,
    mobs::leash::{self, Holder, Pending},
};
use horizon_core::nbt::Tag;
use horizon_ecs_proto::leash::Holder as EcsHolder;
use mod_api::LootDrops;

/// The item context of a leash success (`InteractionResult.Success`): tying
/// held mobs to this one and the holder's detach are `withoutItem`, so item
/// criteria see an empty hand; shears and a new lead keep the used item.
pub(in crate::game) enum LeashInteraction {
    WithItem,
    WithoutItem,
}

/// Whether a mob's lead is held by `acting` (`getLeashHolder() == player`).
fn held_by_acting_player(
    leash: Option<Holder>,
    leash_player: Option<[i32; 4]>,
    acting: [i32; 4],
) -> bool {
    leash == Some(Holder::Player) && leash_player.is_none_or(|p| p == acting)
}

/// Write a mob's `leash` NBT entry into `tag`.
fn write_mob_leash_tag(
    tag: &mut Tag,
    holder: Option<Holder>,
    pending: Option<Pending>,
    leash_player: Option<[i32; 4]>,
    level_player_uuid: Option<[i32; 4]>,
    mob_uuid: impl FnOnce(u32) -> [i32; 4],
) {
    let Tag::Compound(map) = tag else { return };
    map.remove(&"leash".into());
    let uuid = match (holder, pending) {
        (Some(Holder::Player), _) => leash_player.or(level_player_uuid).or(Some([0, 0, 0, 1])),
        (Some(Holder::Mob(id) | Holder::Boat(id)), _) => Some(mob_uuid(id)),
        (_, Some(Pending::Uuid(u))) => Some(u),
        _ => None,
    };
    let value = match (holder, pending) {
        (Some(h), _) => leash::to_nbt(h, uuid),
        (None, Some(Pending::Knot(p))) => leash::to_nbt(Holder::Knot(p), None),
        (None, Some(Pending::Uuid(u))) => leash::to_nbt(Holder::Player, Some(u)),
        _ => None,
    };
    if let Some(v) = value {
        map.insert("leash".into(), v);
    }
}

impl Game<'_> {
    /// The fences the live leads of the level in place hang on, mobs first,
    /// each once.
    fn lead_fences(&self) -> Vec<[i32; 3]> {
        if self.ecs.is_active() {
            return self.ecs_lead_fences();
        }
        let mut fences = Vec::new();
        let mobs = self
            .remote_entities
            .iter()
            .filter(|m| !m.removed)
            .map(|m| m.extra.leash);
        for holder in mobs {
            if let Some(Holder::Knot(p)) = holder
                && !fences.contains(&p)
            {
                fences.push(p);
            }
        }
        fences
    }
    /// `LeashFenceKnotEntity.notifyLeasheeRemoved`: a fence no lead hangs on
    /// any more has lost its knot, with its id. Its UUID goes with the rest
    /// of the dead entities' memory (`prune_entity_memory`).
    pub(super) fn forget_untied_knots(&mut self) {
        let fences = self.lead_fences();
        for (_, p) in self.ecs.knots() {
            if !fences.contains(&p) {
                self.ecs.knot_released(p);
            }
        }
    }
    /// `LeashFenceKnotEntity.notifyLeasheeRemoved`: only a knot losing a
    /// holder asks whether it still has another one. The departing entity may
    /// be ticked through a copy, so its old stored payload must not count.
    pub(super) fn notify_knot_leashee_removed(&mut self, holder: Option<Holder>, departing: u32) {
        let Some(Holder::Knot(fence)) = holder else {
            return;
        };
        if self.ecs.is_active() {
            return self.ecs_knot_released(fence);
        }
        if self
            .remote_entities
            .iter()
            .any(|m| m.id != departing && !m.removed && m.extra.leash == Some(Holder::Knot(fence)))
        {
            return;
        }
        self.ecs_knot_released(fence);
    }
    /// `LeashFenceKnotEntity.getOrCreateKnot` → `createKnot`: the first lead
    /// tied to a fence makes its knot, a real entity with its own id (the
    /// shared `Entity.id` counter) and UUID, whether or not anyone tracks
    /// it. A fence that already has one keeps it. Call
    /// [`Self::forget_untied_knots`] before the tie.
    pub(super) fn tie_knot(&mut self, fence: [i32; 3]) {
        if self.ecs_knot_id(fence).is_some() {
            return;
        }
        let mut next = self.next_entity;
        let id = self.ecs.knot_tie(fence, &mut || {
            next += 1;
            next
        });
        self.next_entity = next;
        if id != 0 {
            self.uuid_for(id);
        }
    }
    /// The authoritative fence of an active natural knot, until a last-holder
    /// notification or the tracking cleanup retires its physical owner.
    pub(super) fn natural_knot(&self, id: u32) -> Option<[i32; 3]> {
        use horizon_client::join_seq::residency::table::EntityLookup;
        let EntityLookup::Found {
            location,
            value: Target::Knot(_),
        } = self.server_resident_entity(id)
        else {
            return None;
        };
        if location.owner.dimension != self.dim.key() {
            return None;
        }
        self.ecs.knot_fence_of(id)
    }
    /// Actual natural knot owners, without allocating or inferring membership from leashes.
    pub(super) fn natural_knot_id_iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.ecs.knots().into_iter().map(|(id, _)| id)
    }
    /// The ids of the level's natural knots that a lead still holds, in the
    /// order they were made.
    pub(super) fn natural_knot_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.natural_knot_id_iter().collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
    pub(super) fn lead_sound(&mut self, sound: &str, at: [f64; 3]) {
        self.audio
            .play(&format!("minecraft:item.lead.{sound}"), Some(at), 1.0, 1.0);
    }
    /// `Leashable.canHaveALeashAttachedTo` for the mob at `index` and a
    /// holder whose box centre is `center`: within the species' snapping
    /// range and leashable.
    pub(super) fn can_attach_lead(&self, index: usize, center: glam::DVec3) -> bool {
        leash::can_have_a_leash_attached(
            &self.remote_entities[index],
            horizon_core::geometry::Vec3::new(center.x, center.y, center.z),
        )
    }
    /// `Entity.gameEvent(event, player)` for a lead: the acting player is the source.
    pub(super) fn lead_game_event(&mut self, event: &'static str, at: [f64; 3]) {
        let ctx = self.player_context(None);
        self.post_game_event(event, at, ctx);
    }
    /// `Entity.interact`'s sneak branch: the mobs and boats on the player's
    /// lead within reach of `center` that `canHaveALeashAttachedTo` the
    /// entity (`skip` is the entity itself) are re-tied to `to`. Whether any
    /// was.
    pub(super) fn tie_player_leads(
        &mut self,
        center: glam::DVec3,
        to: Holder,
        skip: Option<usize>,
    ) -> bool {
        let mut any = false;
        for j in self.player_leashed(center) {
            if Some(j) != skip && self.can_attach_lead(j, center) {
                self.remote_entities[j].extra.leash = Some(to);
                any = true;
            }
        }
        // The boats on the player's lead too (game/entities/boat_leads.rs).
        any | self.tie_player_boats(center, to)
    }
    pub(super) fn player_center(&self) -> glam::DVec3 {
        self.player.as_ref().map_or(glam::DVec3::ZERO, |p| {
            glam::DVec3::new(
                p.position.x,
                p.position.y + f64::from(p.dimensions.height) / 2.0,
                p.position.z,
            )
        })
    }
    /// The mobs on the acting player's lead within 32 blocks of a point
    /// (`Leashable.leashableInArea` with `getLeashHolder() == player`).
    pub(super) fn player_leashed(&self, around: glam::DVec3) -> Vec<usize> {
        let acting = self.acting_uuid();
        (0..self.remote_entities.len())
            .filter(|&i| {
                let m = &self.remote_entities[i];
                held_by_acting_player(m.extra.leash, m.extra.leash_player, acting)
                    && (m.position.x - around.x).abs() <= 16.0
                    && (m.position.y - around.y).abs() <= 16.0
                    && (m.position.z - around.z).abs() <= 16.0
            })
            .collect()
    }
    /// `Entity.spawnAtLocation(level, Items.LEAD)`: the item entity draws its
    /// motion from its own random, not the level's.
    /// `MiscEvent::LeashBroke`: the ECS took the lead off by itself (snap or holder gone); the knot's last lead, the snap and the lead item are
    /// the host's (`Leashable.dropLeash`).
    pub(in crate::game) fn ecs_leash_broke(&mut self, id: u32, at: [f64; 3], sound: bool, knot: Option<[i32; 3]>) {
        self.notify_knot_leashee_removed(knot.map(Holder::Knot), id);
        if sound {
            self.lead_sound("break", at);
        }
        // `Leashable.tickLeash`: a lead that snapped (`sound`) always drops (`leashTooFarBehaviour` -> `dropLeash`); one whose holder or creature
        // is gone drops under `entityDrops`, else it is `removeLeash`.
        if sound || self.session.rules.bool_at(horizon_client::gamerules::vanilla::ENTITY_DROPS) {
            self.drop_lead_item(at);
        }
    }
    pub(super) fn drop_lead_item(&mut self, at: [f64; 3]) {
        self.spawn_item_own(ItemStack::new("minecraft:lead", 1), at, 10);
    }
    /// `Leashable.dropLeash` / `removeLeash`: the lead falls as an item.
    pub(super) fn break_leash(&mut self, id: u32, drop: bool, sound: bool) {
        self.drop_leash(id, drop, sound, false, None);
    }
    /// A mob's `MobEvent::LeashBreak`: the lead drops where the event says
    /// (`Mob.startRiding` drops it before the rider is seated), else at the mob.
    pub(super) fn leash_break_event(&mut self, event: MobEvent) {
        if let MobEvent::LeashBreak {
            mob,
            drop,
            sound,
            at,
        } = event
        {
            self.drop_leash(mob, drop, sound, false, at);
        }
    }
    /// `Leashable.tickLeash` ending a lead: a dying mob's falls as an item only under
    /// `entity_drops`, else it is `removeLeash`.
    pub(super) fn tick_break_leash(&mut self, id: u32, drop: bool, sound: bool) {
        let dying = if self.ecs.is_active() {
            self.mob_exists(id) && !self.mob_alive(id)
        } else {
            self.remote_entities.iter().any(|m| m.id == id && !m.alive())
        };
        let drop = drop
            && (!dying
                || self
                    .session
                    .rules
                    .bool_at(horizon_client::gamerules::vanilla::ENTITY_DROPS));
        self.break_leash(id, drop, sound);
    }
    /// `Entity.dropAllLeashConnections`: every mob and boat leashed to the
    /// acting player, inside the 32-block box around the player's centre,
    /// drops its lead. Then `GameEvent.SHEAR`. Whether anything dropped.
    /// The player is not `Leashable`, so there is no lead of their own.
    pub(super) fn drop_all_leash_connections(&mut self) -> bool {
        let center = self.player_center();
        // In the ECS the boats are entities too: the spatial read takes them with the mobs.
        let (mobs, boats): (Vec<u32>, _) = if self.ecs.is_active() {
            (self.ecs_leashed_to_acting(center), Vec::new())
        } else {
            let mobs = self
                .player_leashed(center)
                .into_iter()
                .map(|i| self.remote_entities[i].id)
                .collect();
            (mobs, self.player_led_boats(center))
        };
        if mobs.is_empty() && boats.is_empty() {
            return false;
        }
        for id in mobs {
            // One snap is played by the caller (`SoundEvents.LEAD_BREAK`),
            // not a break sound on each leashed entity.
            self.break_leash(id, true, false);
        }
        self.drop_boat_leashes(&boats);
        let at = self
            .player
            .as_ref()
            .map_or([center.x, center.y, center.z], |p| {
                [p.position.x, p.position.y, p.position.z]
            });
        // `gameEvent(GameEvent.SHEAR, null)`: the player is the position, not the source.
        let ctx = self.who_context(None, None);
        self.post_game_event("minecraft:shear", at, ctx);
        true
    }
    /// `Leashable.setLeashedTo` on the mob or boat `target`: its lead goes to `holder` whatever
    /// held it, or was still waiting for a saved holder. `player` is a player holder's UUID.
    /// Taking the lead over is no removal, so no unleash hook hears it. False for anything else.
    pub(super) fn set_leash(
        &mut self,
        target: Target,
        holder: Holder,
        player: Option<[i32; 4]>,
    ) -> bool {
        if self.ecs.is_active() {
            return match target {
                Target::Mob(id) | Target::Boat(id) => self.ecs_take_lead(id, holder, player),
                _ => false,
            };
        }
        match target {
            Target::Mob(id) => {
                let Some(m) = self.remote_entities.iter_mut().find(|m| m.id == id) else {
                    return false;
                };
                let old = m.extra.leash;
                m.extra.leash = Some(holder);
                m.extra.leash_player = player;
                m.extra.leash_pending = None;
                if old != Some(holder) {
                    self.notify_knot_leashee_removed(old, id);
                }
                true
            }
            _ => false,
        }
    }
    /// `Leashable.dropLeash` (`drop`) or `removeLeash` on the mob or boat `target`. False when no
    /// holder has the lead: a holder still unresolved from a save stays, as in `dropLeash`.
    pub(super) fn detach_leash(&mut self, target: Target, drop: bool) -> bool {
        if self.ecs.is_active() {
            return match target {
                Target::Mob(id) | Target::Boat(id) => {
                    let held = self.ecs_leash_holder(id).is_some();
                    if held {
                        self.break_leash(id, drop, false);
                    }
                    held
                }
                _ => false,
            };
        }
        match target {
            Target::Mob(id) => {
                let held = self
                    .remote_entities
                    .iter()
                    .any(|m| m.id == id && m.extra.leash.is_some());
                if held {
                    self.break_leash(id, drop, false);
                }
                held
            }
            _ => false,
        }
    }
    /// `break_leash` by the acting player's use (untying it, shears).
    fn break_leash_used(&mut self, id: u32, drop: bool) {
        self.drop_leash(id, drop, false, true, None);
    }
    /// `Leashable.dropLeash`; `from` is where the mob stood when it let go,
    /// when that is not where it is now.
    fn drop_leash(&mut self, id: u32, drop: bool, sound: bool, used: bool, from: Option<[f64; 3]>) {
        let taken = if self.ecs.is_active() {
            self.ecs_take_leash(id, from)
        } else {
            self.remote_entities.iter_mut().find(|m| m.id == id).and_then(|m| {
                let holder = m.extra.leash;
                horizon_client::mobs::leash::remove(m)?;
                Some((
                    from.unwrap_or([m.position.x, m.position.y, m.position.z]),
                    super::native_hooks::mob_kind(m),
                    holder,
                ))
            })
        };
        let Some((at, kind, holder)) = taken else {
            return;
        };
        self.notify_knot_leashee_removed(holder, id);
        self.native_entity_unleash(id, &kind);
        if used
            && let (Some(player), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone())
        {
            self.native_player_unleash_entity(player, &name, id);
        }
        if sound {
            self.lead_sound("break", at);
        }
        if !drop {
            return;
        }
        // The mods' say when a player is behind it: the user.
        let player = used.then(|| self.native_acting_id());
        let Some(player) = player else {
            self.drop_lead_item(at);
            return;
        };
        let mut drops = vec![ItemStack::new("minecraft:lead", 1)];
        let context = LootDrops::from(mod_api::__private::built::LootDrops {
            weapon: None,
            damage_source: None,
            table: "",
            dimension: self.dim.id(),
            origin: at,
            block: None,
            entity: Some((id, &kind)),
            player: Some(player),
            tool: None,
            explosion: None,
            used,
        });
        self.native_loot_drops(&context, &mut drops);
        for stack in drops {
            if stack.count > 0 && &*stack.item != "minecraft:air" {
                self.spawn_item_own(stack, at, 10);
            }
        }
    }
    /// The `leash` tag of a mob about to be saved.
    pub(super) fn write_leash(&mut self, tag: &mut Tag, id: u32) {
        if self.ecs.is_active() {
            // `leash::save` (P, missing): the holder's `leash` NBT of the ECS entity, written by P (no leash NBT in P yet).
            if let Some(e) = self.ecs.entity_of(id) {
                self.ecs.leash_save(e, tag);
            }
            return;
        }
        let Some(m) = self.mob_object(id) else {
            return;
        };
        let (holder, pending, leash_player) =
            (m.extra.leash, m.extra.leash_pending, m.extra.leash_player);
        let level_player_uuid = self.level.player_uuid;
        write_mob_leash_tag(
            tag,
            holder,
            pending,
            leash_player,
            level_player_uuid,
            |id| self.mob_uuid(id),
        );
    }
    /// `Leashable.canHaveALeashAttachedTo` for the ECS mob `id` (P's snap range, `EcsLevel::leash_can_attach`, missing).
    pub(super) fn can_attach_lead_ecs(&self, id: u32, center: glam::DVec3) -> bool {
        self.ecs.entity_of(id).is_some_and(|e| self.ecs.leash_can_attach(e, [center.x, center.y, center.z]))
    }
    /// `Entity.interact`'s sneak branch in the ECS: the acting player's leads within reach re-tie to `to`; `skip` is the entity itself.
    pub(super) fn tie_player_leads_ecs(&mut self, center: glam::DVec3, to: Holder, skip: Option<u32>) -> bool {
        let acting = self.acting_player_uuid();
        let Some(h) = self.ecs_holder(to, Some(acting)) else {
            return false;
        };
        let mut any = false;
        for id in self.ecs_leashed_to_acting(center) {
            if Some(id) != skip && self.can_attach_lead_ecs(id, center) {
                any |= self.ecs_set_leash(id, Some(h));
            }
        }
        // The boats on the lead stay the host's.
        any | self.tie_player_boats(center, to)
    }
    /// `Entity.interact` on the ECS mob `id` with the held `item`: the sneak tie, the shears, the untie and the lead.
    /// The bridge holds the leads; the host keeps the boats, the sounds, the game events and the item.
    pub(super) fn interact_leash_ecs(&mut self, id: u32, item: &str) -> Option<LeashInteraction> {
        let sneaking = self.sneak_held;
        let e = self.ecs.entity_of(id)?;
        let at = self.ecs.position(id)?;
        if !self.mob_alive(id) {
            return None;
        }
        let me = self.acting_player_uuid_id()?;
        if sneaking && self.ecs.leash_can_be_leashed(e) {
            let b = self.mob_box(id)?;
            let (lo, hi) = (b.min_position(), b.max_position());
            let center = glam::DVec3::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0, (lo.z + hi.z) / 2.0);
            if self.tie_player_leads_ecs(center, Holder::Mob(id), Some(id)) {
                self.lead_game_event("minecraft:entity_action", at);
                self.lead_sound("tied", at);
                return Some(LeashInteraction::WithoutItem);
            }
        }
        if item == "minecraft:shears" {
            let mut dropped = self.ecs_leash_holder(id).is_some();
            if dropped {
                self.break_leash_used(id, true);
            }
            for o in self.ecs_leashed_ids(EcsHolder::Entity(e)) {
                self.break_leash_used(o, true);
                dropped = true;
            }
            let boats = self.boats_on(Holder::Mob(id));
            dropped |= !boats.is_empty();
            self.drop_boat_leashes(&boats);
            if dropped {
                self.lead_game_event("minecraft:shear", at);
                self.audio.play("minecraft:item.shears.snip", Some(at), 1.0, 1.0);
                self.damage_held_tool();
                return Some(LeashInteraction::WithItem);
            }
        }
        // Only the player the lead is tied to unties it (`getLeashHolder() == player`).
        if self.ecs_leash_holder(id) == Some(EcsHolder::Player(me)) {
            let creative = self.creative;
            self.break_leash_used(id, !creative);
            self.lead_game_event("minecraft:entity_interact", at);
            self.lead_sound("untied", at);
            return Some(LeashInteraction::WithoutItem);
        }
        if item == "minecraft:lead" {
            let center = self.player_center();
            if self.can_attach_lead_ecs(id, center) {
                if self.ecs_leash_holder(id).is_some() {
                    self.break_leash_used(id, true);
                }
                self.ecs_set_leash(id, Some(EcsHolder::Player(me)));
                self.lead_sound("tied", at);
                if !self.creative
                    && let Some(stack) = self.held_mut().as_mut()
                {
                    stack.count -= 1;
                    if stack.count == 0 {
                        *self.held_mut() = None;
                    }
                }
                return Some(LeashInteraction::WithItem);
            }
        }
        None
    }
    /// `LeadItem.useOn` on the ECS fence at `knot_pos`: the knot's leads are the bridge's, the boats and the sounds the host's.
    pub(super) fn use_knot_ecs(&mut self, knot_pos: [i32; 3], knot_hit: bool) -> bool {
        let knot = leash::knot_state(knot_pos).center;
        let center = glam::DVec3::new(knot.x, knot.y, knot.z);
        let at = [center.x, center.y, center.z];
        let k = EcsHolder::Knot(knot_pos);
        let existing = self.ecs_leashed_ids(k);
        let existing_boats = self.boats_on(Holder::Knot(knot_pos));
        let held_item = self.held().map(|s| s.item.to_string());
        let sneaking = self.sneak_held;
        if knot_hit && held_item.as_deref() == Some("minecraft:shears") {
            for id in existing {
                self.break_leash_used(id, true);
            }
            self.drop_boat_leashes(&existing_boats);
            self.audio.play("minecraft:item.shears.snip", Some(at), 1.0, 1.0);
            self.damage_held_tool();
            return true;
        }
        let in_hand = self.ecs_leashed_to_acting(center);
        let boats_in_hand = self.player_led_boats(center);
        if held_item.as_deref() != Some("minecraft:lead")
            && existing.is_empty()
            && existing_boats.is_empty()
            && in_hand.is_empty()
            && boats_in_hand.is_empty()
        {
            return false;
        }
        if existing.is_empty()
            && existing_boats.is_empty()
            && (!in_hand.is_empty() || !boats_in_hand.is_empty())
            && self.hanging_place_cancelled("minecraft:leash_knot", knot_pos)
        {
            return true;
        }
        self.forget_untied_knots();
        let mut attached = self.tie_player_boats(center, Holder::Knot(knot_pos));
        for id in in_hand {
            if self.can_attach_lead_ecs(id, center) {
                self.ecs_set_leash(id, Some(k));
                attached = true;
            }
        }
        if attached {
            self.tie_knot(knot_pos);
        }
        if !attached && !sneaking && knot_hit {
            let player = self.player_center();
            if let Some(me) = self.acting_player_uuid_id() {
                for id in existing {
                    if self.can_attach_lead_ecs(id, player) {
                        self.ecs_set_leash(id, Some(EcsHolder::Player(me)));
                        attached = true;
                    }
                }
            }
            attached |= self.take_knot_boats(knot_pos);
        }
        if attached {
            let event_at = if knot_hit {
                [knot.x, knot.y - 0.25, knot.z]
            } else {
                knot_pos.map(|v| f64::from(v) + 0.5)
            };
            self.lead_game_event("minecraft:block_attach", event_at);
            self.lead_sound("tied", at);
            self.swing_arm();
        }
        attached
    }
    #[cfg(test)]
    pub(super) fn interact_leash(&mut self, index: usize, item: &str) -> bool {
        self.interact_leash_with_item_context(index, item).is_some()
    }
    /// `Entity.interact`: retain the holder-detach success's item context.
    pub(super) fn interact_leash_with_item_context(
        &mut self,
        index: usize,
        item: &str,
    ) -> Option<LeashInteraction> {
        let sneaking = self.sneak_held;
        let id = self.remote_entities[index].id;
        let at = self.remote_entities[index].center();
        if !self.remote_entities[index].alive() {
            return None;
        }
        // Sneaking with mobs in hand: tie them to this one.
        if sneaking && leash::can_be_leashed(&self.remote_entities[index]) && !self.remote_entities[index].is_baby() {
            let b = self.remote_entities[index].bounds();
            let (lo, hi) = (b.min_position(), b.max_position());
            let center = glam::DVec3::new(
                (lo.x + hi.x) / 2.0,
                (lo.y + hi.y) / 2.0,
                (lo.z + hi.z) / 2.0,
            );
            if self.tie_player_leads(center, Holder::Mob(id), Some(index)) {
                self.lead_game_event("minecraft:entity_action", at);
                self.lead_sound("tied", at);
                return Some(LeashInteraction::WithoutItem);
            }
        }
        if item == "minecraft:shears" {
            let mut dropped = false;
            if self.remote_entities[index].extra.leash.is_some() {
                self.break_leash_used(id, true);
                dropped = true;
            }
            let others: Vec<u32> = self
                .remote_entities
                .iter()
                .filter(|m| m.extra.leash == Some(Holder::Mob(id)))
                .map(|m| m.id)
                .collect();
            for o in others {
                self.break_leash_used(o, true);
                dropped = true;
            }
            let boats = self.boats_on(Holder::Mob(id));
            dropped |= !boats.is_empty();
            self.drop_boat_leashes(&boats);
            if dropped {
                self.lead_game_event("minecraft:shear", at);
                self.audio
                    .play("minecraft:item.shears.snip", Some(at), 1.0, 1.0);
                self.damage_held_tool();
                return Some(LeashInteraction::WithItem);
            }
        }
        // Mob.checkAndHandleImportantInteractions: only the player the lead
        // is tied to unties it (`getLeashHolder() == player`).
        let acting = self.acting_player_uuid();
        if held_by_acting_player(
            self.remote_entities[index].extra.leash,
            self.remote_entities[index].extra.leash_player,
            acting,
        ) {
            let creative = self.creative;
            self.break_leash_used(id, !creative);
            self.lead_game_event("minecraft:entity_interact", at);
            self.lead_sound("untied", at);
            return Some(LeashInteraction::WithoutItem);
        }
        if item == "minecraft:lead" {
            let center = self.player_center();
            if self.can_attach_lead(index, center) {
                if self.remote_entities[index].extra.leash.is_some() {
                    self.break_leash_used(id, true);
                }
                let holder = self.acting_player_uuid();
                // LeashData.setLeashHolder: a holder still waiting in the save
                // is forgotten, so it can neither take over nor drop a lead.
                let extra = &mut self.remote_entities[index].extra;
                extra.leash = Some(Holder::Player);
                extra.leash_pending = None;
                extra.leash_player = Some(holder);
                self.lead_sound("tied", at);
                // `Player.interactOn` restores the count of a player with
                // infinite materials (creative); others spend the lead.
                if !self.creative
                    && let Some(stack) = self.held_mut().as_mut()
                {
                    stack.count -= 1;
                    if stack.count == 0 {
                        *self.held_mut() = None;
                    }
                }
                return Some(LeashInteraction::WithItem);
            }
        }
        None
    }
    /// `LeadItem.useOn` on the fence the crosshair hits.
    pub(super) fn interact_knot(&mut self) -> bool {
        let Some(hit) = self.pick() else {
            return false;
        };
        let name = self
            .res
            .facts()
            .get(self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]));
        if !self
            .res
            .items()
            .block_tags
            .get("minecraft:fences")
            .is_some_and(|s| s.contains(&*name.name))
        {
            return false;
        }
        // `FenceBlock.useWithoutItem` / `LeadItem.useOn` only tie the player's own mobs
        // (`LeadItem.bindPlayerMobs`). A client whose crosshair met the knot sends Interact with
        // the knot's id instead. Taking a mob back here would undo a tie: the client sends the
        // click once for each hand.
        self.use_knot(hit.pos, false)
    }
    /// A click at the fence at `knot_pos`: `knot_hit` for `LeashFenceKnotEntity.interact`,
    /// otherwise `LeadItem.useOn`. Returns whether it was used.
    pub(super) fn use_knot(&mut self, knot_pos: [i32; 3], knot_hit: bool) -> bool {
        if self.ecs.is_active() {
            return self.use_knot_ecs(knot_pos, knot_hit);
        }
        let knot = leash::knot_state(knot_pos).center;
        let center = glam::DVec3::new(knot.x, knot.y, knot.z);
        let at = [center.x, center.y, center.z];
        let existing: Vec<usize> = (0..self.remote_entities.len())
            .filter(|&i| self.remote_entities[i].extra.leash == Some(Holder::Knot(knot_pos)))
            .collect();
        let existing_boats = self.boats_on(Holder::Knot(knot_pos));
        let held_item = self.held().map(|s| s.item.to_string());
        let sneaking = self.sneak_held;
        // Shears snip every lead on the knot.
        if knot_hit && held_item.as_deref() == Some("minecraft:shears") {
            for i in existing {
                let id = self.remote_entities[i].id;
                self.break_leash_used(id, true);
            }
            self.drop_boat_leashes(&existing_boats);
            self.audio
                .play("minecraft:item.shears.snip", Some(at), 1.0, 1.0);
            self.damage_held_tool();
            return true;
        }
        let in_hand = self.player_leashed(center);
        let boats_in_hand = self.player_led_boats(center);
        // The knot is only made for the lead item, or when mobs already hang on it.
        if held_item.as_deref() != Some("minecraft:lead")
            && existing.is_empty()
            && existing_boats.is_empty()
            && in_hand.is_empty()
            && boats_in_hand.is_empty()
        {
            return false;
        }
        // A knot is made when a lead is tied to a fence that has none.
        if existing.is_empty()
            && existing_boats.is_empty()
            && (!in_hand.is_empty() || !boats_in_hand.is_empty())
            && self.hanging_place_cancelled("minecraft:leash_knot", knot_pos)
        {
            return true;
        }
        self.forget_untied_knots();
        let mut attached = self.tie_player_boats(center, Holder::Knot(knot_pos));
        for j in in_hand {
            if self.can_attach_lead(j, center) {
                self.remote_entities[j].extra.leash = Some(Holder::Knot(knot_pos));
                attached = true;
            }
        }
        if attached {
            self.tie_knot(knot_pos);
        }
        if !attached && !sneaking && knot_hit {
            let player = self.player_center();
            let holder = self.acting_player_uuid();
            for i in existing {
                if self.can_attach_lead(i, player) {
                    self.remote_entities[i].extra.leash = Some(Holder::Player);
                    self.remote_entities[i].extra.leash_player = Some(holder);
                    attached = true;
                }
            }
            attached |= self.take_knot_boats(knot_pos);
        }
        if attached {
            let event_at = if knot_hit {
                [knot.x, knot.y - 0.25, knot.z]
            } else {
                knot_pos.map(|v| f64::from(v) + 0.5)
            };
            self.lead_game_event("minecraft:block_attach", event_at);
            self.lead_sound("tied", at);
            self.swing_arm();
        }
        attached
    }
}

#[cfg(test)]
mod attach_tests {
    use crate::game::{Audio, Options, Progress, Settings, load_resources};
    use horizon_core::geometry::Vec3;

    /// A game with resources but no level: a test pushes the mobs it needs
    /// and drives the lead code alone.
    fn game(resources: &mut Option<std::sync::Arc<crate::game::Resources>>) -> super::Game<'_> {
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
        options.execution_role = crate::game::execution_role::ExecutionRole::IntegratedServer;
        let res = resources.insert(loaded.resources);
        let mut game = super::Game::new(
            res,
            loaded.pack,
            options,
            settings,
            Audio::default(),
        )
        .unwrap();
        game.creative = false;
        game.player = Some(
            horizon_client::motion::PlayerMotion::new(game.motion, Vec3::new(0.5, 64.0, 0.5))
                .unwrap(),
        );
        game
    }

    /// The centre of a mob's box: `Leashable.leashDistanceTo`'s own end.
    fn centre(m: &horizon_client::mobs::Mob) -> glam::DVec3 {
        let b = m.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        glam::DVec3::new(
            (lo.x + hi.x) / 2.0,
            (lo.y + hi.y) / 2.0,
            (lo.z + hi.z) / 2.0,
        )
    }

    /// `Leashable.canHaveALeashAttachedTo` measures with the leashee's own
    /// `leashSnapDistance`: a happy ghast's 16, a cow's default 12.
    #[test]
    #[ignore = "daily"]
    fn a_lead_attaches_within_the_species_own_snap_distance() {
        use horizon_client::mobs::{Mob, Species};

        let mut resources = None;
        let mut game = game(&mut resources);
        for (id, kind, species) in [
            (1, "minecraft:happy_ghast", Species::HappyGhast),
            (2, "minecraft:cow", Species::Cow),
        ] {
            let kind = game.res.mob_types().get(kind).expect(kind).clone();
            game.remote_entities
                .push(Mob::new(id, kind, Vec3::new(0.5, 64.0, 0.5), 0.0));
            assert_eq!(game.remote_entities.last().unwrap().species(), species);
        }
        let at = |index: usize, blocks: f64| {
            let c = centre(&game.remote_entities[index]);
            glam::DVec3::new(c.x + blocks, c.y, c.z)
        };
        assert!(
            game.can_attach_lead(0, at(0, 15.0)),
            "a happy ghast takes a lead 15 blocks out, as vanilla ties it"
        );
        assert!(
            !game.can_attach_lead(0, at(0, 17.0)),
            "past its 16-block snap distance it cannot be tied"
        );
        assert!(
            game.can_attach_lead(1, at(1, 11.0)),
            "a cow takes a lead inside the default distance"
        );
        assert!(
            !game.can_attach_lead(1, at(1, 13.0)),
            "a cow still snaps its lead past 12 blocks"
        );
    }

}

#[cfg(test)]
#[path = "tests/leash_knot_uuid_tests.rs"]
mod leash_knot_uuid_tests;
