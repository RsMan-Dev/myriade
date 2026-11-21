//! Outputs toward the host: a block write, a sound, particles. The `Mob` pushes `MobEvent::SetBlock`, `Sound` and `LevelParticles`
//! for the host to apply; here a module writes one at the moment of the act (an event, never a scan) and the host reads them all
//! once per tick with `drain`. One queue (a field of `goals::Events`, `events.host`), empty at rest: no allocation and no byte per entity. Fire and forget, as
//! the `Mob`'s: the host's refusal of a block is not reported back.
use horizon_client_lib::items::ItemStack;
use horizon_world::registry::BlockStateId;
use crate::targeting::Who;

/// One output, one for one with the `MobEvent` of the same name.
#[derive(Clone, Debug, PartialEq)]
pub enum HostEvent {
    SetBlock { pos: [i32; 3], state: BlockStateId },
    Sound { name: &'static str, at: [f64; 3], volume: f32, pitch: f32 },
    Particles { kind: &'static str, at: [f64; 3], spread: [f64; 3], speed: f64, count: u32 },
    /// A velocity added to a player (`MobEvent::Push`).
    /// `MobEvent::EntityEvent`: the client's entity event code (animation, sound).
    EntityEvent { entity: bevy_ecs::entity::Entity, code: u8 },
    /// `Warden.applyDarknessAround`: darkness for the survival players within `radius` of `at`.
    Darkness { at: [f64; 3], radius: f64 },
    /// A boss bar of entity `id`: its progress (0..1) and colour; `remove` closes it.
    BossBar { id: u32, name: &'static str, progress: f32, color: u8, remove: bool },
    PushPlayer { player: u32, velocity: [f64; 3] },
    ParrotToShoulder { parrot: bevy_ecs::entity::Entity, player: u32 },
    /// An area effect cloud appeared (`radius`, `duration` in ticks).
    Cloud { at: [f64; 3], radius: f32, duration: i32 },
    /// A display's data changed: the host resends it to the players tracking it.
    DisplayUpdate { entity: bevy_ecs::entity::Entity },
    /// A world event (`LevelEvent`: sound and particle codes played by the client).
    LevelEvent { event: i32, at: [i32; 3], data: i32 },
    /// A game event (vibration source), by name.
    GameEvent { name: &'static str, at: [f64; 3] },
    /// A game event the entity makes while it moves (`Entity.gameEvent`: the source is the context): `ground` adds the block under it as the
    /// affected state (step, hit_ground), `sneaking` is `isSteppingCarefully`, `dampens` is `dampensVibrations`.
    GameEventBy { entity: bevy_ecs::entity::Entity, name: &'static str, at: [f64; 3], ground: bool, sneaking: bool, dampens: bool },
    /// Experience dropped as orbs.
    Xp { at: [f64; 3], amount: u32 },
    /// A player's ender pearl landed: teleport the player to `to`, hurt it by `damage` (a fall), and spawn an endermite where it stood if `endermite`.
    Pearl { player: u32, to: [f64; 3], damage: f32, endermite: bool },
    /// A creature pulled by a hook: its velocity is added by the host.
    PullEntity { entity: bevy_ecs::entity::Entity, velocity: [f64; 3] },
    /// `MobEvent::ChestOpen`: a chest's lid moves for the mob `entity`.
    ChestOpen { chest: [i32; 3], open: bool, entity: bevy_ecs::entity::Entity },
    /// `MobEvent::ChestSlot`: a chest's slot now holds `stack` (none: empty).
    ChestSlot { chest: [i32; 3], slot: usize, stack: Option<ItemStack> },
    /// `copper_golem` animation state of a golem (`IDLE`, `GETTING_ITEM`...): the `Mob`'s `variant3`.
    GolemState { entity: bevy_ecs::entity::Entity, state: u8 },
    /// A lethal hit: `cause` is the vanilla damage type's name, `attacker` the entity that struck, if any.
    /// `at` is where it died. While the host listens (`DeathPolicy`) the entity is still in the world, `Dying`, when the host reads
    /// this: it calls `loot_dying`, its hook, then `despawn`; else P has removed it already.
    Death { entity: bevy_ecs::entity::Entity, cause: &'static str, attacker: Option<Who>, at: [f64; 3] },
    /// `Mob.convertTo` is about to replace `entity` by a new one of the species `into`; only while the host listens
    /// (`ConversionWatched`). The host answers with `resolve_conversion`.
    ConversionAsk { entity: bevy_ecs::entity::Entity, into: &'static str },
    /// The target of `entity` is now `target` (`None`: no target); `previous` is the one it had.
    TargetChanged { entity: bevy_ecs::entity::Entity, target: Option<Who>, previous: Option<Who> },
    /// A creature appeared in the world.
    /// `natural` is set only by the host's natural spawning pass (`spawn_natural`).
    Spawned { entity: bevy_ecs::entity::Entity, natural: bool },
    /// `TamableAnimal.tame`: the player `owner` owns `entity`.
    Tamed { entity: bevy_ecs::entity::Entity, owner: Who },
    /// A blow of `attacker` that landed on `victim` for `damage`.
    HitDealt { attacker: Who, victim: bevy_ecs::entity::Entity, damage: f32 },
    /// The lead came off `entity`.
    Unleashed { entity: bevy_ecs::entity::Entity },
    /// A lethal hit on `entity`, which holds a totem: the host asks `living_use_totem` (only while a mod listens,
    /// `TotemWatched`) and answers with `resolve_totem`. `cause` and `attacker` are the hit's.
    TotemAsk { entity: bevy_ecs::entity::Entity, off_hand: bool, cause: &'static str, attacker: Option<Who> },
    // One variant per lot: the bridge `step` moves each lot resource here.
    Arrow(crate::host_out_arrows::ArrowEvent),
    BoatEffect(crate::host_out_boat_effects::BoatEffectEvent),
    BoatHurt(crate::host_out_boat_hurt::BoatHurtEvent),
    Boat(crate::host_out_boats::BoatEvent),
    CartMove(crate::host_out_cart_move::CartMoveEvent),
    Cart(crate::host_out_cart_variants::CartEvent),
    Equipment(crate::host_out_equipment::EquipmentEvent),
    Falling(crate::host_out_falling::FallingEvent),
    Firework(crate::host_out_firework::FireworkEvent),
    Frame(crate::host_out_frames::FrameEvent),
    General(crate::host_out_general::GeneralEvent),
    Hook(crate::host_out_hook::HookEvent),
    Inside(crate::host_out_inside::InsideEvent),
    Item(crate::host_out_items::ItemEvent),
    Misc(crate::host_out_misc::MiscEvent),
    Seat(crate::host_out_seat::SeatEvent),
    Tnt(crate::host_out_tnt::TntEvent),
    Trident(crate::host_out_trident::TridentEvent),
    Using(crate::host_out_using::UsingEvent),
    VehiclePush(crate::host_out_vehicle_push::VehiclePushEvent),
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
    pub fn chest_open(&mut self, chest: [i32; 3], open: bool, entity: bevy_ecs::entity::Entity) {
        self.out.push(HostEvent::ChestOpen { chest, open, entity });
    }
    pub fn chest_slot(&mut self, chest: [i32; 3], slot: usize, stack: Option<ItemStack>) {
        self.out.push(HostEvent::ChestSlot { chest, slot, stack });
    }
    pub fn golem_state(&mut self, entity: bevy_ecs::entity::Entity, state: u8) {
        self.out.push(HostEvent::GolemState { entity, state });
    }
    pub fn darkness(&mut self, at: [f64; 3], radius: f64) {
        self.out.push(HostEvent::Darkness { at, radius });
    }
    pub fn boss_bar(&mut self, id: u32, name: &'static str, progress: f32, color: u8) {
        self.out.push(HostEvent::BossBar { id, name, progress, color, remove: false });
    }
    pub fn boss_bar_remove(&mut self, id: u32) {
        self.out.push(HostEvent::BossBar { id, name: "", progress: 0.0, color: 0, remove: true });
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
    /// `MobEvent::ParrotToShoulder`: the host puts the creature on the player's shoulder and removes it.
    pub fn shoulder(&mut self, parrot: bevy_ecs::entity::Entity, player: u32) {
        self.out.push(HostEvent::ParrotToShoulder { parrot, player });
    }
    pub fn cloud(&mut self, at: [f64; 3], radius: f32, duration: i32) {
        self.out.push(HostEvent::Cloud { at, radius, duration });
    }
    pub fn display_update(&mut self, entity: bevy_ecs::entity::Entity) {
        self.out.push(HostEvent::DisplayUpdate { entity });
    }
    pub fn level_event(&mut self, event: i32, at: [i32; 3], data: i32) {
        self.out.push(HostEvent::LevelEvent { event, at, data });
    }
    pub fn game_event(&mut self, name: &'static str, at: [f64; 3]) {
        self.out.push(HostEvent::GameEvent { name, at });
    }
    pub fn game_event_by(&mut self, entity: bevy_ecs::entity::Entity, name: &'static str, at: [f64; 3], ground: bool, sneaking: bool, dampens: bool) {
        self.out.push(HostEvent::GameEventBy { entity, name, at, ground, sneaking, dampens });
    }
    pub fn pearl(&mut self, player: u32, to: [f64; 3], damage: f32, endermite: bool) {
        self.out.push(HostEvent::Pearl { player, to, damage, endermite });
    }
    pub fn xp(&mut self, at: [f64; 3], amount: u32) {
        self.out.push(HostEvent::Xp { at, amount });
    }
    pub fn pull_entity(&mut self, entity: bevy_ecs::entity::Entity, velocity: [f64; 3]) {
        self.out.push(HostEvent::PullEntity { entity, velocity });
    }
    pub fn falling(&mut self, ev: crate::host_out_falling::FallingEvent) {
        self.out.push(HostEvent::Falling(ev));
    }
    pub fn frame(&mut self, ev: crate::host_out_frames::FrameEvent) {
        self.out.push(HostEvent::Frame(ev));
    }
    pub fn death(&mut self, entity: bevy_ecs::entity::Entity, cause: &'static str, attacker: Option<Who>, at: [f64; 3]) {
        self.out.push(HostEvent::Death { entity, cause, attacker, at });
    }
    pub fn conversion_ask(&mut self, entity: bevy_ecs::entity::Entity, into: &'static str) {
        self.out.push(HostEvent::ConversionAsk { entity, into });
    }
    pub fn target_changed(&mut self, entity: bevy_ecs::entity::Entity, target: Option<Who>, previous: Option<Who>) {
        self.out.push(HostEvent::TargetChanged { entity, target, previous });
    }
    pub fn spawned(&mut self, entity: bevy_ecs::entity::Entity, natural: bool) {
        self.out.push(HostEvent::Spawned { entity, natural });
    }
    pub fn tamed(&mut self, entity: bevy_ecs::entity::Entity, owner: Who) {
        self.out.push(HostEvent::Tamed { entity, owner });
    }
    pub fn hit_dealt(&mut self, attacker: Who, victim: bevy_ecs::entity::Entity, damage: f32) {
        self.out.push(HostEvent::HitDealt { attacker, victim, damage });
    }
    pub fn unleashed(&mut self, entity: bevy_ecs::entity::Entity) {
        self.out.push(HostEvent::Unleashed { entity });
    }
    pub fn totem_ask(&mut self, entity: bevy_ecs::entity::Entity, off_hand: bool, cause: &'static str, attacker: Option<Who>) {
        self.out.push(HostEvent::TotemAsk { entity, off_hand, cause, attacker });
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
