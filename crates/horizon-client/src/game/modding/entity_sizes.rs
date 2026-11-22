//! `entity_size` (`EntityEvent.Size`): vanilla's `Entity.refreshDimensions` lets a listener
//! change the size it just computed, whenever the pose changes and from the constructor.
//! Horizon computes a mob's size from its state (`Mob::dims`) and keeps a player's in its body
//! (`PlayerMotion::refresh_dimensions`), so the hook is asked here instead: a pass tells it each
//! entity whose pose or vanilla size changed since it was last asked (or that was never asked,
//! the constructor's call), and keeps the answer on the entity (`pose::SizeAnswer`) for as long
//! as that pose and vanilla size hold. The server's mobs are asked before and after their ticks,
//! a guest's body once per server tick, the client's own body after its tick.

use super::Game;
use horizon_client::pose::{Dimensions, SizeAnswer};
use mod_api::{EntityRef, Pose};

impl Game<'_> {
    /// Ask `entity_size` for an entity whose vanilla size in `pose` is `vanilla`, unless
    /// `previous` was already asked for both. `None`: nothing to ask, keep `previous`.
    fn ask_entity_size(
        &self,
        id: u32,
        pose: Pose,
        vanilla: [f32; 3],
        previous: Option<SizeAnswer>,
    ) -> Option<SizeAnswer> {
        if previous.is_some_and(|answer| answer.asked(pose, vanilla)) {
            return None;
        }
        let entity = EntityRef::new(self.native_lifecycle.epoch()?, id);
        // `getOldSize`: the size it had (the last answer's, or vanilla's on the first ask).
        let old = previous.map_or(vanilla, |answer| answer.effective());
        let mut size = vanilla;
        self.native_entity_size(entity, pose, old, &mut size);
        // Bound mod answers once, before collision/pathfinding consume them. 32 blocks
        // exceeds even the dragon's natural width (16), while capping a path node's
        // bounding-box scan at 33^3 cells and keeping integer extents representable.
        // Invalid answers retain vanilla size, just like non-finite answers.
        let usable = size
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0 && *v <= 32.0);
        Some(SizeAnswer {
            pose,
            vanilla,
            size: (usable && size != vanilla).then_some(size),
        })
    }

    /// The server's mobs whose pose or size changed since they were last asked.
    pub(super) fn native_mob_sizes(&mut self) {
        if !mod_api::__private::listened(mod_api::HookId::EntitySize) {
            return;
        }
        // ECS sizes are not asked yet (listed in the report).
    }

    /// A body's pose or size changed since it was last asked: ask, and size it by the answer.
    fn ask_body_size(
        &self,
        id: u32,
        body: &horizon_client::motion::PlayerMotion,
    ) -> Option<SizeAnswer> {
        let vanilla = body.pose.dimensions(body.scale).to_array();
        self.ask_entity_size(id, body.pose.into(), vanilla, body.size_answer)
    }

    fn apply_body_size(body: &mut horizon_client::motion::PlayerMotion, answer: SizeAnswer) {
        body.size_answer = Some(answer);
        body.dimensions = Dimensions::from_array(answer.effective());
    }


    /// Each guest's body on the server, once a tick.
    pub(super) fn native_guest_sizes(&mut self) {
        if !mod_api::__private::listened(mod_api::HookId::EntitySize) {
            return;
        }
        let Some(host) = self.lan.as_ref() else {
            return;
        };
        let asked: Vec<(u32, SizeAnswer)> = host
            .hub
            .guests
            .as_slice()
            .iter()
            .filter_map(|guest| {
                let body = guest.motion.as_deref()?;
                Some((
                    guest.conn,
                    self.ask_body_size(guest.entity_id as u32, body)?,
                ))
            })
            .collect();
        for (conn, answer) in asked {
            if let Some(body) = self
                .lan
                .as_mut()
                .and_then(|host| host.hub.guests.motion_mut(conn))
            {
                Self::apply_body_size(body, answer);
            }
        }
    }
}
