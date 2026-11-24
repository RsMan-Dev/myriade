//! Leads on the game side: putting them on and taking them off mobs
//! (`Entity.interact`'s leash part), fence knots (`LeadItem.useOn` and
//! `LeashFenceKnotEntity.interact`), loading and saving who holds what,
//! breaking, and drawing the rope (`LeashFeatureRenderer`) and the knot.
use super::{Game, commands::Target};
use myriade_engine::{
    items::ItemStack,
    mobs::{
        MobEvent,
        leash::{self, Holder},
    },
};
use horizon_core::nbt::Tag;
use horizon_ecs_proto::leash::Holder as EcsHolder;

/// The item context of a leash success (`InteractionResult.Success`): tying
/// held mobs to this one and the holder's detach are `withoutItem`, so item
/// criteria see an empty hand; shears and a new lead keep the used item.
pub(in crate::game) enum LeashInteraction {
    WithItem,
    WithoutItem,
}

impl Game<'_> {
    /// `LeashFenceKnotEntity.notifyLeasheeRemoved`: a fence no lead hangs on
    /// any more has lost its knot, with its id. Its UUID goes with the rest
    /// of the dead entities' memory (`prune_entity_memory`).
    pub(super) fn forget_untied_knots(&mut self) {
        let fences = self.ecs_lead_fences();
        for (_, p) in self.ecs.knots() {
            if !fences.contains(&p) {
                self.ecs.knot_released(p);
            }
        }
    }
    /// `LeashFenceKnotEntity.notifyLeasheeRemoved`: only a knot losing a
    /// holder asks whether it still has another one. The departing entity may
    /// be ticked through a copy, so its old stored payload must not count.
    pub(super) fn notify_knot_leashee_removed(&mut self, holder: Option<Holder>) {
        let Some(Holder::Knot(fence)) = holder else {
            return;
        };
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
        // The level in place's ECS holds the knot: no residency row is kept for a knot (`Kind::Knot` has no list), so the ECS answers.
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
    /// `Entity.gameEvent(event, player)` for a lead: the acting player is the source.
    pub(super) fn lead_game_event(&mut self, event: &'static str, at: [f64; 3]) {
        let ctx = self.player_context(None);
        self.post_game_event(event, at, ctx);
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
    /// `Entity.spawnAtLocation(level, Items.LEAD)`: the item entity draws its
    /// motion from its own random, not the level's.
    /// `MiscEvent::LeashBroke`: the ECS took the lead off by itself (snap or holder gone); the knot's last lead, the snap and the lead item are
    /// the host's (`Leashable.dropLeash`).
    pub(in crate::game) fn ecs_leash_broke(&mut self, id: u32, at: [f64; 3], sound: bool, knot: Option<[i32; 3]>) {
        self.notify_knot_leashee_removed(knot.map(Holder::Knot));
        if sound {
            self.lead_sound("break", at);
        }
        // `Leashable.tickLeash`: a lead that snapped (`sound`) always drops (`leashTooFarBehaviour` -> `dropLeash`); one whose holder or creature
        // is gone drops under `entityDrops`, else it is `removeLeash`.
        if sound || self.session.rules.bool_at(myriade_engine::gamerules::vanilla::ENTITY_DROPS) {
            self.drop_lead_item(at);
        }
    }
    pub(super) fn drop_lead_item(&mut self, at: [f64; 3]) {
        self.spawn_item_own(ItemStack::new("minecraft:lead", 1), at, 10);
    }
    /// `Leashable.dropLeash` / `removeLeash`: the lead falls as an item.
    pub(super) fn break_leash(&mut self, id: u32, drop: bool, sound: bool) {
        self.drop_leash(id, drop, sound, None);
    }
    /// A mob's `MobEvent::LeashBreak`: the lead drops where the event says, else at the mob.
    pub(super) fn leash_break_event(&mut self, event: MobEvent) {
        if let MobEvent::LeashBreak { mob, drop, sound, at } = event {
            self.drop_leash(mob, drop, sound, at);
        }
    }
    /// `Leashable.tickLeash` ending a lead: a dying mob's falls as an item only under
    /// `entity_drops`, else it is `removeLeash`.
    pub(super) fn tick_break_leash(&mut self, id: u32, drop: bool, sound: bool) {
        let dying = self.mob_exists(id) && !self.mob_alive(id);
        let drop = drop
            && (!dying
                || self
                    .session
                    .rules
                    .bool_at(myriade_engine::gamerules::vanilla::ENTITY_DROPS));
        self.break_leash(id, drop, sound);
    }
    /// `Entity.dropAllLeashConnections`: every mob and boat leashed to the
    /// acting player, inside the 32-block box around the player's centre,
    /// drops its lead. Then `GameEvent.SHEAR`. Whether anything dropped.
    /// The player is not `Leashable`, so there is no lead of their own.
    pub(super) fn drop_all_leash_connections(&mut self) -> bool {
        let center = self.player_center();
        // In the ECS the boats are entities too: the spatial read takes them with the mobs.
        let mobs = self.ecs_leashed_to_acting(center);
        if mobs.is_empty() {
            return false;
        }
        for id in mobs {
            // One snap is played by the caller (`SoundEvents.LEAD_BREAK`),
            // not a break sound on each leashed entity.
            self.break_leash(id, true, false);
        }
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
        match target {
            Target::Mob(id) | Target::Boat(id) => self.ecs_take_lead(id, holder, player),
            _ => false,
        }
    }
    /// `Leashable.dropLeash` (`drop`) or `removeLeash` on the mob or boat `target`. False when no
    /// holder has the lead: a holder still unresolved from a save stays, as in `dropLeash`.
    pub(super) fn detach_leash(&mut self, target: Target, drop: bool) -> bool {
        match target {
            Target::Mob(id) | Target::Boat(id) => {
                let held = self.ecs_leash_holder(id).is_some();
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
        self.drop_leash(id, drop, false, None);
    }
    /// `Leashable.dropLeash`; `from` is where the mob stood when it let go,
    /// when that is not where it is now.
    fn drop_leash(&mut self, id: u32, drop: bool, sound: bool, from: Option<[f64; 3]>) {
        let Some((at, _kind, holder)) = self.ecs_take_leash(id, from) else {
            return;
        };
        self.notify_knot_leashee_removed(holder);
        if sound {
            self.lead_sound("break", at);
        }
        if !drop {
            return;
        }
        self.drop_lead_item(at);
    }
    /// The `leash` tag of a mob about to be saved.
    pub(super) fn write_leash(&mut self, tag: &mut Tag, id: u32) {
        // `leash::save` (P, missing): the holder's `leash` NBT of the ECS entity, written by P (no leash NBT in P yet).
        if let Some(e) = self.ecs.entity_of(id) {
            self.ecs.leash_save(e, tag);
        }
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
        self.use_knot_ecs(knot_pos, knot_hit)
    }
}

#[cfg(test)]
mod attach_tests {
    use crate::game::{Game, ecs::test_support::ecs_game_with};

    /// The centre of a mob's box: `Leashable.leashDistanceTo`'s own end.
    fn centre(game: &Game<'_>, id: u32) -> glam::DVec3 {
        let b = game.mob_box(id).unwrap();
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
        let (mut game, ghast) = ecs_game_with("minecraft:happy_ghast", [0.5, 64.0, 0.5]);
        let cow = game.ecs_spawn("minecraft:cow", [1.5, 64.0, 0.5], None).unwrap();
        let at = |game: &Game<'_>, id: u32, blocks: f64| {
            let c = centre(game, id);
            glam::DVec3::new(c.x + blocks, c.y, c.z)
        };
        assert!(
            game.can_attach_lead_ecs(ghast, at(&game, ghast, 15.0)),
            "a happy ghast takes a lead 15 blocks out, as vanilla ties it"
        );
        assert!(
            !game.can_attach_lead_ecs(ghast, at(&game, ghast, 17.0)),
            "past its 16-block snap distance it cannot be tied"
        );
        assert!(
            game.can_attach_lead_ecs(cow, at(&game, cow, 11.0)),
            "a cow takes a lead inside the default distance"
        );
        assert!(
            !game.can_attach_lead_ecs(cow, at(&game, cow, 13.0)),
            "a cow still snaps its lead past 12 blocks"
        );
    }
}

#[cfg(test)]
#[path = "tests/leash_knot_uuid_tests.rs"]
mod leash_knot_uuid_tests;
