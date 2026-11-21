//! Leads on boats and rafts (`AbstractBoat implements Leashable`): putting a
//! lead on one, taking it off, saving who holds it, and dropping the lead
//! when the boat breaks. The rope is drawn by `leads.rs`
//! (`LeashFeatureRenderer`, four ropes from a boat to a happy ghast).
//! A boat is also a holder (`Holder::Boat`): `Entity.interact`'s sneak click
//! ties the player's mobs and boats to it.
//!
//! The lead of a boat is the ECS's `Leash` component on the boat's entity: the
//! pull of the rope (`Leashable.tickLeash`) and the snap are the ECS's, and the
//! host only ties, unties and drops the lead by network id.
use super::Game;
use horizon_client::actions::Action;
use horizon_client::mobs::leash::{self, Holder};
use horizon_core::geometry::Vec3;
use horizon_core::nbt::Tag;
use horizon_ecs_proto::{bridge::ids::id_of, leash::Holder as EcsHolder};

/// `AbstractBoat`'s `getBbWidth` and `getBbHeight`.
const WIDTH: f32 = 1.375;
const HEIGHT: f32 = 0.5625;

/// The centre of a boat standing at `at`: half its height up.
fn boat_centre(at: Vec3) -> Vec3 {
    Vec3::new(at.x, at.y + f64::from(HEIGHT) / 2.0, at.z)
}
fn distance(a: Vec3, b: Vec3) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

impl Game<'_> {
    /// The centre of the ECS boat `id`, None when it is not in the level.
    fn boat_centre_of(&self, id: u32) -> Option<Vec3> {
        let p = self.ecs.position(id)?;
        Some(boat_centre(Vec3::new(p[0], p[1], p[2])))
    }

    /// `Leashable.dropLeash` / `removeLeash` on a boat: the ECS takes the lead off, and it falls as an item (`drop`).
    pub(super) fn release_boat_leash(&mut self, id: u32, drop: bool) {
        let Some((at, _, knot)) = self.ecs_take_leash(id, None) else {
            return;
        };
        self.notify_knot_leashee_removed(knot, id);
        if drop {
            self.drop_lead_item(at);
        }
    }

    /// The `leash` tag of the boat `id` about to be saved: the ECS's lead, under the vanilla key.
    pub(super) fn write_boat_leash(&mut self, tag: &mut Tag, id: u32) {
        let Some(e) = self.ecs.entity_of(id) else {
            return;
        };
        if let Tag::Compound(map) = tag {
            map.remove(&"leash".into());
        }
        self.ecs.leash_save(e, tag);
    }

    /// Whether the acting player holds the lead of boat `id`.
    fn boat_led_by_player(&self, id: u32) -> bool {
        self.acting_player_uuid_id()
            .is_some_and(|me| self.ecs_leash_holder(id) == Some(EcsHolder::Player(me)))
    }

    /// The boat with this UUID (`ServerLevel.getEntity`).
    pub(super) fn boat_by_uuid(&self, uuid: [i32; 4]) -> Option<u32> {
        self.persist
            .uuids
            .iter()
            .find(|(id, u)| {
                **u == uuid
                    && self.ecs.boat_status(**id).is_some()
                    && self.resident_source_target_visible(
                        self.dim.key(),
                        super::commands::Target::Boat(**id),
                    )
            })
            .map(|(id, _)| *id)
    }

    /// The fence post of the leash knot with this UUID (`ServerLevel.getEntity`):
    /// a summoned or loaded `minecraft:leash_knot`, which sits on its block.
    pub(super) fn knot_by_uuid(&self, uuid: [i32; 4]) -> Option<[i32; 3]> {
        self.others
            .iter()
            .find(|o| o.kind == "minecraft:leash_knot" && self.persist.uuid_of(o.id) == Some(uuid))
            .map(|o| {
                let block = horizon_core::position::BlockPos::containing(o.position);
                [block.x, block.y, block.z]
            })
    }

    /// The boats on the acting player's lead within 16 blocks of a point
    /// (`Leashable.leashableInArea`), found by the spatial index.
    pub(super) fn player_led_boats(&self, around: glam::DVec3) -> Vec<u32> {
        let reach = 16.0 + f64::from(WIDTH) / 2.0;
        let at = [around.x, around.y, around.z];
        self.ecs
            .ids_near(at, reach * 3f64.sqrt())
            .into_iter()
            .filter(|&id| {
                self.ecs.boat_status(id).is_some()
                    && self.boat_led_by_player(id)
                    && self.boat_centre_of(id).is_some_and(|c| {
                        (c.x - around.x).abs() <= reach
                            && (c.y - around.y).abs() <= 16.0
                            && (c.z - around.z).abs() <= reach
                    })
            })
            .collect()
    }

    /// The boats whose lead hangs on `holder`.
    pub(super) fn boats_on(&self, holder: Holder) -> Vec<u32> {
        self.ecs_holder(holder, None).map_or_else(Vec::new, |h| {
            self.ecs_leashed_ids(h)
                .into_iter()
                .filter(|&id| self.ecs.boat_status(id).is_some())
                .collect()
        })
    }

    /// `Leashable.canHaveALeashAttachedTo` for the boat `id` and an entity
    /// whose box centre is `at`.
    fn boat_can_attach(&self, id: u32, at: glam::DVec3) -> bool {
        self.boat_centre_of(id).is_some_and(|c| {
            distance(Vec3::new(at.x, at.y, at.z), c) <= leash::SNAP
        })
    }

    /// The acting player's boats on a lead go to `to` (`Entity.interact` of a
    /// sneak click on a mob, `LeadItem.bindPlayerMobs`, the knot's
    /// `interact`), the ones in reach of `at`. Whether any did.
    pub(super) fn tie_player_boats(&mut self, at: glam::DVec3, to: Holder) -> bool {
        let mut any = false;
        for id in self.player_led_boats(at) {
            // Leashable.setLeashedTo: `this != holder`.
            if to == Holder::Boat(id) {
                continue;
            }
            if self.boat_can_attach(id, at) {
                any |= self.ecs_take_lead(id, to, None);
            }
        }
        any
    }

    /// `LeashFenceKnotEntity.interact`: the boats on the knot go back on the
    /// acting player's lead. Whether any did.
    pub(super) fn take_knot_boats(&mut self, knot: [i32; 3]) -> bool {
        let me = self.acting_player_uuid();
        let player = self.player_center();
        let mut any = false;
        for id in self.boats_on(Holder::Knot(knot)) {
            if self.boat_can_attach(id, player) {
                any |= self.ecs_take_lead(id, Holder::Player, Some(me));
            }
        }
        self.notify_knot_leashee_removed(Some(Holder::Knot(knot)), 0);
        any
    }

    /// `Leashable.dropLeash` for each boat in `ids`.
    pub(super) fn drop_boat_leashes(&mut self, ids: &[u32]) {
        for &id in ids {
            self.release_boat_leash(id, true);
        }
    }

    /// `Entity.interact`'s lead part for the boat `id`; whether it took
    /// the click.
    pub(super) fn interact_boat_leash(&mut self, id: u32) -> bool {
        let item = self.held().map(|s| s.item.to_string());
        let Some(c) = self.boat_centre_of(id) else {
            return false;
        };
        let at = [c.x, c.y, c.z];
        // The sneak branch: the player's mobs and boats on leads are re-tied
        // to this boat.
        let sneaking = self.keys.held(Action::Sneak);
        if sneaking
            && self.tie_player_leads(glam::DVec3::new(c.x, c.y, c.z), Holder::Boat(id), None)
        {
            self.lead_game_event("minecraft:entity_action", at);
            self.lead_sound("tied", at);
            return true;
        }
        // Entity.shearOffAllLeashConnections: its own lead and every one
        // hanging on it.
        if item.as_deref() == Some("minecraft:shears") {
            let held_mobs: Vec<u32> = self
                .ecs_holder(Holder::Boat(id), None)
                .map_or_else(Vec::new, |h| self.ecs_leashed_ids(h))
                .into_iter()
                .filter(|&m| self.ecs.boat_status(m).is_none())
                .collect();
            let mut held_boats = self.boats_on(Holder::Boat(id));
            if self.ecs_leash_holder(id).is_some() {
                held_boats.push(id);
            }
            if !held_mobs.is_empty() || !held_boats.is_empty() {
                for m in held_mobs {
                    self.break_leash(m, true, false);
                }
                self.drop_boat_leashes(&held_boats);
                self.lead_game_event("minecraft:shear", at);
                self.audio
                    .play("minecraft:item.shears.snip", Some(at), 1.0, 1.0);
                self.damage_held_tool();
                return true;
            }
        }
        if self.boat_led_by_player(id) {
            // hasInfiniteMaterials: removeLeash, else dropLeash.
            let drop = !self.creative;
            self.release_boat_leash(id, drop);
            self.lead_game_event("minecraft:entity_interact", at);
            self.lead_sound("untied", at);
            return true;
        }
        let taken_by_player = matches!(self.ecs_leash_holder(id), Some(EcsHolder::Player(_)));
        if item.as_deref() == Some("minecraft:lead") && !taken_by_player {
            let player = self.player_center();
            if self.boat_can_attach(id, player) {
                self.release_boat_leash(id, true);
                let player_uuid = self.acting_player_uuid();
                if !self.ecs_take_lead(id, Holder::Player, Some(player_uuid)) {
                    return true;
                }
                self.lead_sound("tied", at);
                // The lead is always used up.
                if let Some(stack) = self.held_mut().as_mut() {
                    stack.count -= 1;
                    if stack.count == 0 {
                        *self.held_mut() = None;
                    }
                }
                return true;
            }
        }
        false
    }

    /// The leads on boats that hang on a player or a mob, for the entity
    /// tracker: the holder's entity id (`ClientboundSetEntityLinkPacket`).
    pub(super) fn boat_leash_link(&self, id: u32) -> Option<i32> {
        match self.ecs_leash_holder(id)? {
            EcsHolder::Player(p) => Some(p as i32),
            EcsHolder::Entity(e) => self.ecs.world().and_then(|w| id_of(w, e)).map(|h| h as i32),
            EcsHolder::Knot(p) => self.knot_ids.get(&p).map(|&k| k as i32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_boat_is_centred_half_its_height_up() {
        let c = boat_centre(Vec3::new(1.0, 64.0, 2.0));
        assert_eq!((c.x, c.z), (1.0, 2.0));
        assert!((c.y - 64.28125).abs() < 1e-9);
    }
    #[test]
    fn a_boat_holder_is_an_entity_not_a_player() {
        assert_eq!(Holder::Boat(7).who(), None);
        assert_ne!(Holder::Boat(7), Holder::Mob(7));
    }
    #[test]
    fn distances_are_euclidean() {
        let d = distance(Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 4.0, 0.0));
        assert!((d - 5.0).abs() < 1e-12);
    }
}
