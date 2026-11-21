use super::*;
use crate::game::native_entity_refs::tests::with_game;
use mod_api::{
    EnderPearlTeleportEvent, EntityTeleportEvent, Flow, Hooks, ModEntry, ServerHost,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, Ordering},
};

struct PearlProbe {
    mode: Arc<AtomicU8>,
    owners: Arc<Mutex<Vec<u32>>>,
    command_teleports: Arc<Mutex<Vec<u32>>>,
}
#[mod_api::__subscriptions]
impl Hooks for PearlProbe {
    fn entity_teleport(&self, _: &mut dyn ServerHost, event: &mut EntityTeleportEvent<'_>) {
        self.command_teleports.lock().unwrap().push(event.entity);
    }

    fn ender_pearl_teleport(
        &self,
        host: &mut dyn ServerHost,
        event: &mut EnderPearlTeleportEvent<'_>,
    ) -> Flow {
        self.owners.lock().unwrap().push(event.owner.id());
        assert_eq!(event.owner.position(host), Some(event.from));
        assert_eq!(
            event.damage, 0.0,
            "non-player owners take no vanilla fall damage"
        );
        match self.mode.load(Ordering::SeqCst) {
            0 => Flow::Cancel,
            1 => {
                *event.target = [12.0, 80.0, -4.0];
                Flow::Continue
            }
            2 => {
                assert!(event.owner.teleport_to(host, [3.0, 70.0, 3.0], None));
                *event.target = [12.0, 80.0, -4.0];
                Flow::Continue
            }
            3 => {
                assert!(event.owner.discard(host));
                Flow::Continue
            }
            _ => unreachable!(),
        }
    }
}

struct PlayerPearlProbe(Arc<AtomicU8>);
#[mod_api::__subscriptions]
impl Hooks for PlayerPearlProbe {
    fn ender_pearl_teleport(
        &self,
        host: &mut dyn ServerHost,
        event: &mut EnderPearlTeleportEvent<'_>,
    ) -> Flow {
        assert_eq!(
            host.players().acting_player().unwrap().entity(),
            event.owner
        );
        assert_eq!(event.damage, 5.0);
        // What the pearl hit: the block and its face, no entity.
        assert_eq!(event.hit_entity, None);
        assert_eq!(
            (event.hit_block, event.hit_face, event.hit_at),
            ([6, 75, 6], [0, 1, 0], [6.0, 75.0, 6.0])
        );
        match self.0.load(Ordering::SeqCst) {
            0 => return Flow::Cancel,
            2 => assert!(event.owner.teleport_to(host, [3.0, 70.0, 3.0], None)),
            _ => {}
        }
        *event.target = [12.0, 80.0, -4.0];
        event.damage = 0.0;
        Flow::Continue
    }
}
