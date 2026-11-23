//! The outputs toward the host of the lightning, the end crystals, the fangs and the area clouds, read with `EcsWorld::drain_misc`. Merged
//! into `HostEvent::Misc` by the session (host_out.rs is not edited here).
use crate::effects::Kind;
use crate::targeting::Who;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MiscEvent {
    /// A bolt struck at `at`; `cosmetic` for a skeleton trap's, which strikes nothing but shows.
    Lightning { at: [f64; 3], cosmetic: bool },
    /// A bolt summoned by the player `cause` hit `victim` (one event per victim a tick; the host groups them for the advancement).
    LightningVictim { cause: u32, victim: bevy_ecs::entity::Entity },
    /// An end crystal went at `at` (a blow destroyed it).
    CrystalBlast { at: [f64; 3] },
    /// A fang bit at `at` (its bite, hurt or not: the host shows it).
    FangsBite { at: [f64; 3] },
    /// `BeeEnterHiveGoal.start`: the bee `bee` goes into the hive block `hive`. The host stores its saved data in the block
    /// (`addOccupant`) and despawns it; when the block is gone or full it answers with `EcsWorld::bee_forget_hive` and the bee stays.
    BeeEnterHive { bee: bevy_ecs::entity::Entity, hive: [i32; 3] },
    /// `Pufferfish.playerTouch`: the inflated pufferfish `fish` (puff state `size`, `silent`) touches the player `player`; the host hurts it
    /// by `1 + size` (mob attack), then, if the hurt landed, plays the sting and gives poison for `60 * size` ticks.
    PufferSting { fish: bevy_ecs::entity::Entity, player: u32, size: u8, silent: bool },
    /// `LivingDestroyBlockEvent` (only while a mod hears it): the creature `breaker` is about to turn the block `pos` into air; the host asks
    /// the mods and, unless one cancels, writes the air itself.
    DestroyAsk { breaker: bevy_ecs::entity::Entity, pos: [i32; 3], drop: bool },
    /// `DolphinSwimToTreasureGoal.start`: the dolphin `dolphin`, at block `at`, asks for the nearest `#dolphin_located` structure within 50; the
    /// host answers with `EcsWorld::dolphin_treasure` (`None` when there is none) and plays entity event 38 when found.
    TreasureAsk { dolphin: bevy_ecs::entity::Entity, at: [i32; 3] },
    /// `LivingEntity.baseTick`'s drowning of the creature `entity` (once a second): `bubbles` (`LivingDrownEvent.getBubbleCount`, 8 by default) at
    /// `at`; the host draws the bubbles and plays entity event 67 (the drowning sound); the damage is already in the ECS.
    Drowned { entity: bevy_ecs::entity::Entity, at: [f64; 3], bubbles: i32 },
    /// A lead broke by itself (`Leashable.tickLeash`: the holder is gone, or `sound` when the creature went past the snapping distance): the lead
    /// is already off `entity`; the host drops the lead item at `at` (under `entityDrops`) and plays the snap when `sound`.
    /// `knot`: the fence block of the knot that held it, whose last lead may have gone.
    LeashBroke { entity: bevy_ecs::entity::Entity, at: [f64; 3], sound: bool, knot: Option<[i32; 3]> },
    /// A blow was handed to the link of the damage-linked creature `entity` (`damage_link`): the host answers for the link (the heart's
    /// reaction); `by_player` when a player answers for the blow.
    LinkedHurt { entity: bevy_ecs::entity::Entity, by_player: bool },
    /// The creaking `entity` linked to the heart at `pos` asks whether that heart still claims it (`Creaking.tick`: no heart, or another
    /// one's, is `setHealth(0)`). Sent when it loads or wakes and when the block at `pos` changes; the host answers with `kill_orphan`.
    HeartAsked { entity: bevy_ecs::entity::Entity, pos: [i32; 3] },
    /// The creature `entity` finished crumbling (`tear_down`) and is gone: the host shows the crumbling in the box of centre `center` and spread `spread`.
    Crumbled { entity: bevy_ecs::entity::Entity, center: [f64; 3], spread: [f64; 3] },
    /// `Entity.igniteForSeconds` on the player `player` (a burning zombie's blow): the host sets the fire, scaled by the player's burning time.
    PlayerIgnite { player: u32, ticks: i32 },
    /// A timed effect of a potion cloud for the player `target` (a creature's is added in P): `effect` at `amplifier` for `ticks`, `hidden` and
    /// `ambient` as the cloud gives it, `source` and `owner` the cloud's owner (the jar's `addEffect(instance, getOwner())`).
    CloudEffect { target: Who, effect: Kind, amplifier: i32, ticks: i32, hidden: bool, ambient: bool, source: Option<Who>, owner: Option<Who> },
    /// A visible effect given to the player `player` by the mount it rides (`AbstractNautilus.applyEffects`), by its registry name.
    PlayerEffect { player: u32, effect: &'static str, ticks: i32, amplifier: i32, ambient: bool },
}
