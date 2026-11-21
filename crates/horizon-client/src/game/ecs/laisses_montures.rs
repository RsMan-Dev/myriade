//! Lot 12 (colle) : les laisses côté jeu. Une laisse ECS est le composant `Leash` de P (son porteur : entité, joueur par id d'entité,
//! ou nœud) ; le tick, la rupture par distance et l'échange sont ceux de P. L'hôte traduit son `mobs::leash::Holder` vers le pont
//! et lit les laisses par id, sans parcourir les entités.
use super::super::Game;
use horizon_client::mobs::leash::Holder;
use horizon_ecs_proto::{
    bridge::ids::id_of,
    leash::{self as ecs_leash, Holder as EcsHolder},
    targeting::Who,
};

impl Game<'_> {
    /// The bridge's holder for the host's `h` (`uuid`: a player holder's UUID); None when it is not in the ECS.
    pub(in crate::game) fn ecs_holder(&self, h: Holder, uuid: Option<[i32; 4]>) -> Option<EcsHolder> {
        Some(match h {
            Holder::Player => EcsHolder::Player(self.player_id(uuid?)?),
            Holder::Mob(id) | Holder::Boat(id) => EcsHolder::Entity(self.ecs.entity_of(id)?),
            Holder::Knot(p) => EcsHolder::Knot(p),
        })
    }

    /// `Leashable.setLeashedTo` on the ECS entity `e`; None takes the lead off (`detach`). False when `e` is not in the ECS or refuses.
    pub(in crate::game) fn ecs_set_leash(&mut self, e: u32, holder: Option<EcsHolder>) -> bool {
        let Some(en) = self.ecs.entity_of(e) else {
            return false;
        };
        self.ecs.set_leash(en, holder)
    }

    /// The holder of the lead on `e` (`leash::holder_of`); None when `e` is not leashed.
    pub(in crate::game) fn ecs_leash_holder(&self, e: u32) -> Option<EcsHolder> {
        ecs_leash::holder_of(self.ecs.world()?, self.ecs.entity_of(e)?)
    }

    /// Whether a lead hangs on the fence at `p` (`leash::held_by`, the directory of P).
    pub(in crate::game) fn ecs_knot_held(&self, p: [i32; 3]) -> bool {
        self.ecs.world().is_some_and(|w| !ecs_leash::held_by(w, EcsHolder::Knot(p)).is_empty())
    }

    /// The network ids of the ECS entities whose lead hangs on `h` (`leash::held_by`, then the bridge's ids).
    pub(in crate::game) fn ecs_leashed_ids(&self, h: EcsHolder) -> Vec<u32> {
        let Some(w) = self.ecs.world() else {
            return Vec::new();
        };
        ecs_leash::held_by(w, h).into_iter().filter_map(|e| id_of(w, e)).collect()
    }

    /// Loaded leads whose holder was saved as a UUID or a knot: P resolves them once the ECS has its entities (`leash::resolve_pending`, missing).
    pub(in crate::game) fn ecs_resolve_pending_leashes(&mut self) {
        self.ecs.leash_resolve_pending();
    }

    /// The fences the ECS's leads hang on, among the knots the host has named: no scan of the entities.
    pub(in crate::game) fn ecs_lead_fences(&self) -> Vec<[i32; 3]> {
        self.knot_ids.keys().copied().filter(|&p| self.ecs_knot_held(p)).collect()
    }

    /// `LeashFenceKnotEntity.notifyLeasheeRemoved` on the ECS: the knot at `p` goes once no lead hangs on it.
    pub(in crate::game) fn ecs_knot_released(&mut self, p: [i32; 3]) {
        if self.ecs_knot_held(p) {
            return;
        }
        self.knot_ids.remove(&p);
        self.resident_entities.knots.retain(|knot| knot.fence != p);
    }

    /// `Leashable.setLeashedTo` on the ECS: `id` takes the lead of `holder` whatever held it before.
    pub(in crate::game) fn ecs_take_lead(&mut self, id: u32, holder: Holder, player: Option<[i32; 4]>) -> bool {
        let Some(h) = self.ecs_holder(holder, player) else {
            return false;
        };
        let old = self.ecs_leash_holder(id);
        if !self.ecs_set_leash(id, Some(h)) {
            return false;
        }
        if let Some(EcsHolder::Knot(p)) = old.filter(|&o| o != h) {
            self.ecs_knot_released(p);
        }
        true
    }

    /// `Leashable.dropLeash` on the ECS: the lead of `id` comes off, and the host gets where it was, its kind and its knot (for the hooks).
    pub(in crate::game) fn ecs_take_leash(&mut self, id: u32, from: Option<[f64; 3]>) -> Option<([f64; 3], String, Option<Holder>)> {
        let holder = self.ecs_leash_holder(id)?;
        let at = from.or_else(|| self.ecs.position(id))?;
        let kind = self.ecs.kind(id)?.to_owned();
        self.ecs_set_leash(id, None);
        let knot = match holder {
            EcsHolder::Knot(p) => Some(Holder::Knot(p)),
            _ => None,
        };
        Some((at, kind, knot))
    }

    /// The entities on the acting player's lead within 16 blocks of `center` on each axis (`player_leashed`), by the spatial index.
    pub(in crate::game) fn ecs_leashed_to_acting(&self, center: glam::DVec3) -> Vec<u32> {
        let Some(me) = self.acting_player_uuid_id() else {
            return Vec::new();
        };
        let at = [center.x, center.y, center.z];
        self.ecs
            .ids_near(at, 16.0 * 3f64.sqrt())
            .into_iter()
            .filter(|&id| {
                self.ecs.position(id).is_some_and(|p| (0..3).all(|k| (p[k] - at[k]).abs() <= 16.0))
                    && self.ecs_leash_holder(id) == Some(EcsHolder::Player(me))
            })
            .collect()
    }

    /// `mount::boost` of the player's mount when the ECS holds it and it is of kind `kind` (`equine::boost` of the host's mount).
    pub(in crate::game) fn ecs_boost_steered(&mut self, kind: &str) -> bool {
        let Some(id) = self.riding else {
            return false;
        };
        if self.mount_kind_of(id).as_deref() != Some(kind) {
            return false;
        }
        let Some(e) = self.ecs.entity_of(id) else {
            return false;
        };
        self.ecs.boost(e)
    }

    /// The entity id of the acting player: a guest's turn swaps it into the player in place, so it is the own id.
    pub(in crate::game) fn acting_player_uuid_id(&self) -> Option<u32> {
        u32::try_from(self.own_entity_id()).ok()
    }

    /// The entity id of the player with `uuid`: the one in place, or a server guest.
    pub(in crate::game) fn player_id(&self, uuid: [i32; 4]) -> Option<u32> {
        use crate::game::commands::Target;
        match self.target_by_uuid(horizon_client::lan::playerdata::ints_uuid(uuid))? {
            Target::Player => u32::try_from(self.own_entity_id()).ok(),
            Target::Guest(conn) => u32::try_from(self.lan.as_ref()?.hub.guest(conn)?.entity_id).ok(),
            _ => None,
        }
    }

    /// `AbstractHorse.tameWithName` on the ECS mount `id` by the acting player: the owner and the health are P's (`tame::tame`).
    /// The host keeps the particles (`entity_tame` is the `Tamed` event's). The trigger `minecraft:tame_animal` and the cancelled roll are not here yet.
    pub(in crate::game) fn tame_mount_ecs(&mut self, id: u32) {
        let (Some(owner), Some(e)) = (self.acting_player_uuid_id(), self.ecs.entity_of(id)) else {
            return;
        };
        // AnimalTameEvent cancelled: the roll fails and P refuses the rider (`EcsLevel::refuse_rider`, missing).
        let (tamer, tamer_entity) = (self.native_acting_id(), self.own_entity_id());
        if self.ecs_animal_tame_cancelled(id, tamer, tamer_entity) {
            self.ecs.refuse_rider(e);
            return;
        }
        if let Some(w) = self.ecs.world_mut() {
            let health = w.get::<horizon_ecs_proto::cow::Health>(e).map_or(0.0, |h| h.0);
            horizon_ecs_proto::tame::tame(w, e, Who::Player(owner), health);
        }
        self.mob_event_particles(id, 7);
        if self.ecs.entity_of(id).is_some() {
            let view = self.ecs_predicate_view(id);
            self.trigger_entity("minecraft:tame_animal", "entity", &view);
        }
    }

    /// `removePassenger` on the ECS: the rider of the mount `id` is let off by P's `mount::dismount`, and the player is placed where
    /// P says the mount lets them out (`EcsWorld::dismount_point`, read before the rider leaves).
    pub(in crate::game) fn dismount_mount_ecs(&mut self, id: u32) {
        let Some(e) = self.ecs.entity_of(id) else { return };
        let spot = self.ecs_with(|ecs, mw| ecs.dismount_point(mw, e));
        self.ecs.dismount(e);
        if let (Some(at), Some(p)) = (spot, self.player.as_mut()) {
            p.position = horizon_core::geometry::Vec3::new(at[0], at[1], at[2]);
            p.velocity = horizon_core::geometry::Vec3::ZERO;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EcsHolder;
    use crate::game::ecs::test_support::ecs_game_with;

    #[test]
    fn a_lead_is_posed_on_an_ecs_mob_and_removed_again() {
        let (mut g, id) = ecs_game_with("minecraft:cow", [0.0, 64.0, 0.0]);
        let knot = EcsHolder::Knot([0, 64, 0]);
        assert!(g.ecs_set_leash(id, Some(knot)));
        assert_eq!(g.ecs_leash_holder(id), Some(knot));
        assert!(g.ecs_knot_held([0, 64, 0]));
        assert!(g.ecs_set_leash(id, None));
        assert_eq!(g.ecs_leash_holder(id), None);
        assert!(!g.ecs_knot_held([0, 64, 0]));
    }

    #[test]
    fn a_saddle_on_an_ecs_pig_saddles_it() {
        let (mut g, id) = ecs_game_with("minecraft:pig", [0.0, 64.0, 0.0]);
        assert!(g.interact_mount_ecs(id, "minecraft:saddle"));
    }
}
