//! Outputs toward the host: a block write, a sound, particles. The `Mob` pushes `MobEvent::SetBlock`, `Sound` and `LevelParticles`
//! for the host to apply; here a module writes one at the moment of the act (an event, never a scan) and the host reads them all
//! once per tick with `drain`. One queue (a field of `goals::Events`, `events.host`), empty at rest: no allocation and no byte per entity. Fire and forget, as
//! the `Mob`'s: the host's refusal of a block is not reported back.
use horizon_world::registry::BlockStateId;

/// One output, one for one with the `MobEvent` of the same name.
#[derive(Clone, Debug, PartialEq)]
pub enum HostEvent {
    SetBlock { pos: [i32; 3], state: BlockStateId },
    Sound { name: &'static str, at: [f64; 3], volume: f32, pitch: f32 },
    Particles { kind: &'static str, at: [f64; 3], spread: [f64; 3], speed: f64, count: u32 },
    /// A velocity added to a player (`MobEvent::Push`).
    /// `MobEvent::EntityEvent`: the client's entity event code (animation, sound).
    EntityEvent { entity: bevy_ecs::entity::Entity, code: u8 },
    PushPlayer { player: u32, velocity: [f64; 3] },
}

#[derive(Default)]
pub struct HostOut {
    out: Vec<HostEvent>,
}

impl HostOut {
    /// `MobEvent::SetBlock`.
    pub fn set_block(&mut self, pos: [i32; 3], state: BlockStateId) {
        self.out.push(HostEvent::SetBlock { pos, state });
    }
    /// `MobEvent::Sound(name, at, volume, pitch)`.
    pub fn sound(&mut self, name: &'static str, at: [f64; 3], volume: f32, pitch: f32) {
        self.out.push(HostEvent::Sound { name, at, volume, pitch });
    }
    /// `MobEvent::LevelParticles`.
    pub fn particles(&mut self, kind: &'static str, at: [f64; 3], spread: [f64; 3], speed: f64, count: u32) {
        self.out.push(HostEvent::Particles { kind, at, spread, speed, count });
    }
    pub fn entity_event(&mut self, entity: bevy_ecs::entity::Entity, code: u8) {
        self.out.push(HostEvent::EntityEvent { entity, code });
    }
    pub fn push_player(&mut self, player: u32, velocity: [f64; 3]) {
        self.out.push(HostEvent::PushPlayer { player, velocity });
    }
    /// What was written since the last call, in order; the queue keeps its capacity.
    pub fn drain(&mut self) -> std::vec::Drain<'_, HostEvent> {
        self.out.drain(..)
    }
    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_come_out_in_order_once() {
        let mut o = HostOut::default();
        assert!(o.is_empty());
        o.sound("block.beehive.exit", [1.0, 2.0, 3.0], 1.0, 1.0);
        o.set_block([1, 2, 3], BlockStateId(7));
        o.particles("happy_villager", [0.0; 3], [0.5; 3], 0.0, 3);
        let v: Vec<_> = o.drain().collect();
        assert!(matches!(v[0], HostEvent::Sound { .. }) && matches!(v[1], HostEvent::SetBlock { pos: [1, 2, 3], .. }) && matches!(v[2], HostEvent::Particles { count: 3, .. }));
        assert!(o.drain().next().is_none());
    }
}
