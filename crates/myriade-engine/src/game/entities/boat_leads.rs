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
use myriade_engine::mobs::leash::{self, Holder};
use horizon_core::geometry::Vec3;
use horizon_ecs_proto::leash::Holder as EcsHolder;

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
        self.notify_knot_leashee_removed(knot);
        if drop {
            self.drop_lead_item(at);
        }
    }

    /// Whether the acting player holds the lead of boat `id`.
    fn boat_led_by_player(&self, id: u32) -> bool {
        self.acting_player_uuid_id()
            .is_some_and(|me| self.ecs_leash_holder(id) == Some(EcsHolder::Player(me)))
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
        self.notify_knot_leashee_removed(Some(Holder::Knot(knot)));
        any
    }

    /// `Leashable.dropLeash` for each boat in `ids`.
    pub(super) fn drop_boat_leashes(&mut self, ids: &[u32]) {
        for &id in ids {
            self.release_boat_leash(id, true);
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
