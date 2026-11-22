//! The lot's families that the host kept in `Game`: lightning, end crystals, evoker fangs and area clouds. Their state is in components and
//! their deadlines on the wheel; the only system is the area cloud's, the one family that acts at every tick. `MiscOut` queues what the
//! host reads (`EcsWorld::drain_misc`).
use crate::host_out_misc::MiscEvent;
use bevy_ecs::prelude::*;

/// The events of the lot since the host last read them, in order.
#[derive(Resource, Default)]
pub struct MiscOut(pub Vec<MiscEvent>);

pub fn register(s: &mut Schedule) {
    s.add_systems(crate::cloud_tick::tick.after(crate::cow::apply_pushes).run_if(crate::cloud_tick::any_live));
}
