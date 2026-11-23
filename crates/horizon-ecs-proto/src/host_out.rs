//! Outputs toward the host: a block write, a sound, particles. The `Mob` pushes `MobEvent::SetBlock`, `Sound` and `LevelParticles`
//! for the host to apply; here a module writes one at the moment of the act (an event, never a scan) and the host reads them all
//! once per tick with `drain`. One queue (a field of `goals::Events`, `events.host`), empty at rest: no allocation and no byte per entity. Fire and forget, as
//! the `Mob`'s: the host's refusal of a block is not reported back.
use horizon_client_lib::items::ItemStack;
use horizon_world::registry::BlockStateId;
use std::borrow::Cow;
use crate::targeting::Who;

/// One output, one for one with the `MobEvent` of the same name.
#[derive(Clone, Debug, PartialEq)]
pub enum HostEvent {
    SetBlock { pos: [i32; 3], state: BlockStateId },
    /// `source`: the sound category index (`SoundSource`); None is the event's own default.
    /// A block's sound names are read from its state (`Cow::Owned`); a literal is `Cow::Borrowed`.
    Sound { name: Cow<'static, str>, at: [f64; 3], volume: f32, pitch: f32, source: Option<usize> },
    Particles { kind: &'static str, at: [f64; 3], spread: [f64; 3], speed: f64, count: u32 },
    /// `BlockParticleOption(state)`: the dust a landing throws up from the block under a mob (`MobEvent::LandingDust`).
    BlockParticles { state: BlockStateId, at: [f64; 3], count: i32, speed: f64 },
    /// A velocity added to a player (`MobEvent::Push`).
    /// `MobEvent::EntityEvent`: the client's entity event code (animation, sound).
    EntityEvent { entity: bevy_ecs::entity::Entity, code: u8 },
    /// `Warden.applyDarknessAround`: darkness for the survival players within `radius` of `at`.
    Darkness { at: [f64; 3], radius: f64 },
    /// A boss bar of entity `id`: its progress (0..1) and colour; `remove` closes it.
    BossBar { id: u32, name: &'static str, progress: f32, color: u8, remove: bool },
    /// The ender dragon `entity` left phase `from` for `to` (`DragonPhaseChanged`, the phase's own log).
    DragonPhase { entity: bevy_ecs::entity::Entity, from: u8, to: u8 },
    PushPlayer { player: u32, velocity: [f64; 3] },
    ParrotToShoulder { parrot: bevy_ecs::entity::Entity, player: u32 },
    /// An area effect cloud appeared (`radius`, `duration` in ticks).
    Cloud { at: [f64; 3], radius: f32, duration: i32 },
    /// A breath cloud's victims this tick: the players and creatures it harms (`amount` instant damage, `amplifier` of its effect).
    CloudHit { at: [f64; 3], amount: f32, amplifier: u32, players: Vec<u32>, mobs: Vec<bevy_ecs::entity::Entity> },
    /// A display's data changed: the host resends it to the players tracking it.
    DisplayUpdate { entity: bevy_ecs::entity::Entity },
    /// A world event (`LevelEvent`: sound and particle codes played by the client).
    LevelEvent { event: i32, at: [i32; 3], data: i32 },
    /// A game event (vibration source), by name.
    GameEvent { name: &'static str, at: [f64; 3] },
    /// A game event the entity makes while it moves (`Entity.gameEvent`: the source is the context): `ground` adds the block under it as the
    /// affected state (step, hit_ground), `sneaking` is `isSteppingCarefully`, `dampens` is `dampensVibrations`.
    GameEventBy { entity: bevy_ecs::entity::Entity, name: &'static str, at: [f64; 3], ground: bool, sneaking: bool, dampens: bool },
    /// `block_place` posted by an entity at the block's centre, with the state placed (`MobEvent::GameEvent` with a source and a state).
    BlockPlace { entity: bevy_ecs::entity::Entity, at: [f64; 3], state: BlockStateId },
    /// A block broken by `level::break_block`: `drop` is the drop, `breaker` the entity that breaks it (`None` for none).
    BlockBreak { at: [i32; 3], drop: bool, breaker: Option<bevy_ecs::entity::Entity> },
    /// Experience dropped as orbs.
    Xp { at: [f64; 3], amount: u32 },
    /// A player's ender pearl landed: teleport the player to `to`, hurt it by `damage` (a fall), and spawn an endermite where it stood if `endermite`.
    Pearl { player: u32, to: [f64; 3], damage: f32, endermite: bool },
    /// A creature pulled by a hook: its velocity is added by the host.
    PullEntity { entity: bevy_ecs::entity::Entity, velocity: [f64; 3] },
    /// `MobEvent::ChestOpen`: a chest's lid moves for the mob `entity`.
    ChestOpen { chest: [i32; 3], open: bool, entity: bevy_ecs::entity::Entity },
    /// `MobEvent::WorkAtComposter`: the villager `entity` works at the composter `pos` (`WorkAtComposter.compostItems`): the host
    /// applies the effect on the block (its level, the extraction at 8, the level events) from the villager's stacks.
    ComposterWork { entity: bevy_ecs::entity::Entity, pos: [i32; 3] },
    /// `MobEvent::ChestSlot`: a chest's slot now holds `stack` (none: empty).
    ChestSlot { chest: [i32; 3], slot: usize, stack: Option<ItemStack> },
    /// A lethal hit: `cause` is the vanilla damage type's name, `attacker` the entity that struck, if any.
    /// `at` is where it died. While the host listens (`DeathPolicy`) the entity is still in the world, `Dying`, when the host reads
    /// this: it calls `loot_dying`, its hook, then `despawn`; else P has removed it already.
    Death { entity: bevy_ecs::entity::Entity, cause: &'static str, attacker: Option<Who>, at: [f64; 3] },
    /// A raider of raid `raid` (the host's raid id) died: `id` its network id, `by` its killer, `wave`, `leader` and `health` as it had
    /// them (`Raider.die`: the host's raid loses the leader, counts the hero, removes the raider from the bar).
    RaiderDied { raid: i32, id: u32, by: Option<Who>, wave: i32, leader: bool, health: f32 },
    /// A creature already in the world joined raid `raid` (`Raid.joinRaid`), by its network id, in `wave`, `leader` of it or not.
    /// `loaded`: it came back with its `RaidId` from a save (`addWaveMob` without counting its health); else its health counts.
    RaiderJoined { raid: i32, id: u32, wave: i32, leader: bool, loaded: bool },
    /// A raider took the ominous banner (`Raider.pickUpItem`): `Raid.setLeader(wave, raider)` at the host.
    RaiderLeads { raid: i32, id: u32, wave: i32 },
    /// A raider of raid `raid` is about to take a hit (`Raider.hurtServer` runs `updateBossbar` first): `before` is its health then.
    RaiderHurt { raid: i32, id: u32, before: f32 },
    /// `ServerLevel.broadcastDamageEvent` of a hit that went through without killing the followed entity `id`: `damage` is the damage
    /// type's name, `cause` the network id of who struck (0: none), `direct` is set when that one struck with its own hand (not a
    /// projectile); `at` is where the victim stands. Ids are resolved here: the entities may be gone when the host reads it.
    /// `attacker` is who the hurt names as its last attacker (`resolveMobResponsibleForDamage`): None for a no-anger blow (a ram).
    Hurt { id: u32, damage: &'static str, cause: u32, direct: bool, at: [f64; 3], attacker: Option<bevy_ecs::entity::Entity> },
    /// `LivingEntity.die` of the followed entity `id` (`broadcastEntityEvent(3)`): the clients around play its death animation.
    DeathAnimation { id: u32, at: [f64; 3] },
    /// `LivingEntity.tickDeath`'s end (`broadcastEntityEvent(60)`, then `remove(KILLED)`): the smoke of the followed entity `id`, whose box
    /// is `width` wide and `height` high; it is already gone when the host reads this.
    DeathSmoke { id: u32, at: [f64; 3], width: f64, height: f64 },
    /// `Mob.convertTo` is about to replace `entity` by a new one of the species `into`; only while the host listens
    /// (`ConversionWatched`). The host answers with `resolve_conversion`.
    ConversionAsk { entity: bevy_ecs::entity::Entity, into: &'static str },
    /// The target of `entity` is now `target` (`None`: no target); `previous` is the one it had.
    TargetChanged { entity: bevy_ecs::entity::Entity, target: Option<Who>, previous: Option<Who> },
    /// `LivingChangeTargetEvent` (only while a mod hears it): the brain of `entity` proposes `proposed`; the host answers with
    /// `resolve_target` (keep, or set another target).
    TargetAsk { entity: bevy_ecs::entity::Entity, proposed: Option<Who> },
    /// A creature appeared in the world.
    /// `natural` is set only by the host's natural spawning pass (`spawn_natural`).
    /// `reason` is the host's (`spawn_reason::SpawnReason`; `Natural` for `spawn_natural`).
    Spawned { entity: bevy_ecs::entity::Entity, natural: bool, reason: crate::spawn_reason::SpawnReason },
    /// `TamableAnimal.tame`: the player `owner` owns `entity`.
    Tamed { entity: bevy_ecs::entity::Entity, owner: Who },
    /// A blow of `attacker` that landed on `victim` for `damage`, of which `absorbed` went to the absorption hearts (`dealt_absorbed`).
    HitDealt { attacker: Who, victim: bevy_ecs::entity::Entity, damage: f32, absorbed: f32 },
    /// The lead came off `entity`.
    Unleashed { entity: bevy_ecs::entity::Entity },
    /// A lethal hit on `entity`, which holds a totem: the host asks `living_use_totem` (only while a mod listens,
    /// `TotemWatched`) and answers with `resolve_totem`. `cause` and `attacker` are the hit's.
    TotemAsk { entity: bevy_ecs::entity::Entity, off_hand: bool, cause: &'static str, attacker: Option<Who> },
    /// `LivingEntity.swing` started on `entity` (`off_hand`: the other hand): the host tells the players tracking it.
    Swing { entity: bevy_ecs::entity::Entity, off_hand: bool },
    /// The item held in the mouth of `entity` is eaten now (`behaviours::eat_held`): the host runs the item's consume effects, then
    /// answers with `EcsWorld::finish_held_use`. `stack` is what was held when the deadline came; `seed` seeds the random of the
    /// effects (the host's is a `LegacyRandom`, the ECS's is not); `silent` mutes the sounds.
    HeldConsume { entity: bevy_ecs::entity::Entity, stack: ItemStack, seed: i64, silent: bool },
    /// One `item` leaves the hand of `player` that holds it, the main hand first (`give_item::take_from_hand`): a gift or a use on an entity.
    HeldUsed { player: u32, item: u32 },
    /// `ItemUtils.createFilledResult`: one `from` leaves the hand of `player` that holds it and `to` takes its place (else goes to the
    /// inventory, else drops); creative keeps `from`.
    HeldSwap { player: u32, from: u32, to: ItemStack },
    /// `gameEvent(event, player)`: a game event whose source is the player `player` (shearing, for one).
    PlayerGameEvent { player: u32, name: &'static str, at: [f64; 3] },
    /// The tool `item` in the hand of `player` wears by `amount` uses (`hurtAndBreak`: shears, axes); nothing in creative.
    HeldWorn { player: u32, item: u32, amount: u32 },
    /// `Player.addItem`: `stack` goes into the inventory of `player` (an item an entity hands back).
    HeldGiven { player: u32, stack: ItemStack },
    /// `CopperGolem.turnToStatue`: the entity becomes the block `block` at `at` (its `pose` of the block's poses, facing as `yaw`), keeping
    /// its custom `name`; the host places the block and its entity, then removes `entity` (P keeps it until then).
    /// `lead`: the entity was leashed (the inner value is its knot's cell); the host drops the lead item under `entityDrops`.
    BecomeBlock { entity: bevy_ecs::entity::Entity, block: &'static str, at: [i32; 3], pose: u8, yaw: f32, name: Option<String>, lead: Option<Option<[i32; 3]>> },
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
    Fight(FightEvent),
}

pub use crate::host_out_fight::FightEvent;

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
    pub fn darkness(&mut self, at: [f64; 3], radius: f64) {
        self.out.push(HostEvent::Darkness { at, radius });
    }
    pub fn boss_bar(&mut self, id: u32, name: &'static str, progress: f32, color: u8) {
        self.out.push(HostEvent::BossBar { id, name, progress, color, remove: false });
    }
    pub fn boss_bar_remove(&mut self, id: u32) {
        self.out.push(HostEvent::BossBar { id, name: "", progress: 0.0, color: 0, remove: true });
    }
    pub fn dragon_phase(&mut self, entity: bevy_ecs::entity::Entity, from: u8, to: u8) {
        self.out.push(HostEvent::DragonPhase { entity, from, to });
    }
    /// `MobEvent::Sound(name, at, volume, pitch)`.
    pub fn sound(&mut self, name: impl Into<Cow<'static, str>>, at: [f64; 3], volume: f32, pitch: f32) {
        self.out.push(HostEvent::Sound { name: name.into(), at, volume, pitch, source: None });
    }
    /// `ServerLevel.playSound` with an explicit `SoundSource` (`source`, its index).
    pub fn sound_from(&mut self, source: usize, name: impl Into<Cow<'static, str>>, at: [f64; 3], volume: f32, pitch: f32) {
        self.out.push(HostEvent::Sound { name: name.into(), at, volume, pitch, source: Some(source) });
    }
    /// `MobEvent::LandingDust`: `count` crumbs of `state` at `at`, each leaving at `speed`.
    pub fn block_particles(&mut self, state: BlockStateId, at: [f64; 3], count: i32, speed: f64) {
        self.out.push(HostEvent::BlockParticles { state, at, count, speed });
    }
    /// `MobEvent::LevelParticles`.
    pub fn particles(&mut self, kind: &'static str, at: [f64; 3], spread: [f64; 3], speed: f64, count: u32) {
        self.out.push(HostEvent::Particles { kind, at, spread, speed, count });
    }
    pub fn composter_work(&mut self, entity: bevy_ecs::entity::Entity, pos: [i32; 3]) {
        self.out.push(HostEvent::ComposterWork { entity, pos });
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
    pub fn cloud_hit(&mut self, at: [f64; 3], amount: f32, amplifier: u32, players: Vec<u32>, mobs: Vec<bevy_ecs::entity::Entity>) {
        self.out.push(HostEvent::CloudHit { at, amount, amplifier, players, mobs });
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
    pub fn block_place(&mut self, entity: bevy_ecs::entity::Entity, at: [f64; 3], state: BlockStateId) {
        self.out.push(HostEvent::BlockPlace { entity, at, state });
    }
    pub fn block_break(&mut self, at: [i32; 3], drop: bool, breaker: Option<bevy_ecs::entity::Entity>) {
        self.out.push(HostEvent::BlockBreak { at, drop, breaker });
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
    pub fn raider_died(&mut self, raid: i32, id: u32, by: Option<Who>, wave: i32, leader: bool, health: f32) {
        self.out.push(HostEvent::RaiderDied { raid, id, by, wave, leader, health });
    }
    pub fn raider_joined(&mut self, raid: i32, id: u32, wave: i32, leader: bool, loaded: bool) {
        self.out.push(HostEvent::RaiderJoined { raid, id, wave, leader, loaded });
    }
    pub fn raider_leads(&mut self, raid: i32, id: u32, wave: i32) {
        self.out.push(HostEvent::RaiderLeads { raid, id, wave });
    }
    pub fn raider_hurt(&mut self, raid: i32, id: u32, before: f32) {
        self.out.push(HostEvent::RaiderHurt { raid, id, before });
    }
    pub fn death(&mut self, entity: bevy_ecs::entity::Entity, cause: &'static str, attacker: Option<Who>, at: [f64; 3]) {
        self.out.push(HostEvent::Death { entity, cause, attacker, at });
    }
    pub fn hurt(&mut self, id: u32, damage: &'static str, cause: u32, direct: bool, at: [f64; 3], attacker: Option<bevy_ecs::entity::Entity>) {
        self.out.push(HostEvent::Hurt { id, damage, cause, direct, at, attacker });
    }
    pub fn death_animation(&mut self, id: u32, at: [f64; 3]) {
        self.out.push(HostEvent::DeathAnimation { id, at });
    }
    pub fn death_smoke(&mut self, id: u32, at: [f64; 3], width: f64, height: f64) {
        self.out.push(HostEvent::DeathSmoke { id, at, width, height });
    }
    pub fn conversion_ask(&mut self, entity: bevy_ecs::entity::Entity, into: &'static str) {
        self.out.push(HostEvent::ConversionAsk { entity, into });
    }
    pub fn target_changed(&mut self, entity: bevy_ecs::entity::Entity, target: Option<Who>, previous: Option<Who>) {
        self.out.push(HostEvent::TargetChanged { entity, target, previous });
    }
    pub fn target_ask(&mut self, entity: bevy_ecs::entity::Entity, proposed: Option<Who>) {
        self.out.push(HostEvent::TargetAsk { entity, proposed });
    }
    pub fn spawned(&mut self, entity: bevy_ecs::entity::Entity, natural: bool, reason: crate::spawn_reason::SpawnReason) {
        self.out.push(HostEvent::Spawned { entity, natural, reason });
    }
    pub fn tamed(&mut self, entity: bevy_ecs::entity::Entity, owner: Who) {
        self.out.push(HostEvent::Tamed { entity, owner });
    }
    pub fn hit_dealt(&mut self, attacker: Who, victim: bevy_ecs::entity::Entity, damage: f32, absorbed: f32) {
        self.out.push(HostEvent::HitDealt { attacker, victim, damage, absorbed });
    }
    pub fn unleashed(&mut self, entity: bevy_ecs::entity::Entity) {
        self.out.push(HostEvent::Unleashed { entity });
    }
    pub fn totem_ask(&mut self, entity: bevy_ecs::entity::Entity, off_hand: bool, cause: &'static str, attacker: Option<Who>) {
        self.out.push(HostEvent::TotemAsk { entity, off_hand, cause, attacker });
    }
    pub fn swing(&mut self, entity: bevy_ecs::entity::Entity, off_hand: bool) {
        self.out.push(HostEvent::Swing { entity, off_hand });
    }
    pub fn held_consume(&mut self, entity: bevy_ecs::entity::Entity, stack: ItemStack, seed: i64, silent: bool) {
        self.out.push(HostEvent::HeldConsume { entity, stack, seed, silent });
    }
    pub fn held_used(&mut self, player: u32, item: u32) {
        self.out.push(HostEvent::HeldUsed { player, item });
    }
    pub fn held_swap(&mut self, player: u32, from: u32, to: ItemStack) {
        self.out.push(HostEvent::HeldSwap { player, from, to });
    }
    pub fn player_game_event(&mut self, player: u32, name: &'static str, at: [f64; 3]) {
        self.out.push(HostEvent::PlayerGameEvent { player, name, at });
    }
    pub fn held_worn(&mut self, player: u32, item: u32, amount: u32) {
        self.out.push(HostEvent::HeldWorn { player, item, amount });
    }
    pub fn held_given(&mut self, player: u32, stack: ItemStack) {
        self.out.push(HostEvent::HeldGiven { player, stack });
    }
    pub fn become_block(&mut self, entity: bevy_ecs::entity::Entity, block: &'static str, at: [i32; 3], pose: u8, yaw: f32, name: Option<String>, lead: Option<Option<[i32; 3]>>) {
        self.out.push(HostEvent::BecomeBlock { entity, block, at, pose, yaw, name, lead });
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
