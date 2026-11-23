use crate::*;


/// An effect requested once through [`Registrar::effect_key`](EffectsRegistrar::effect_key). The registry is fixed until restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EffectKey(pub u32);

/// An entity type (`minecraft:zombie`) in the built-in entity-type registry, in
/// registration order. Returned by `EntityRef::kind`; Copy/Eq comparisons do not
/// allocate. Request `key.id(host)` for its namespaced name (an owned diagnostic
/// string). A key stays valid until the game exits.
// The derived scalar codec validates host slots; wasm carries the host-issued token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, )]
pub struct EntityTypeKey(pub u32);

impl TryFrom<u32> for EntityTypeKey {
    type Error = &'static str;
    fn try_from(slot: u32) -> Result<Self, Self::Error> {
        // A guest transports the host-issued identity; only the host can validate it.
        #[cfg(target_arch = "wasm32")]
        {
            Ok(Self(slot))
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Ok(Self(slot))
        }
    }
}

/// A persistent entity UUID in the same bit order as the value from `EntityRef::uuid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, )]
pub struct EntityUuid(pub u128);

/// An entity of the open world, for the `Host` methods that read or change one
/// (`entity.position(host)`). It holds the world's epoch and the entity's id: ids
/// are shared by every server level and never reused while the server world is
/// open. An entity that changes dimension on its own (a lone mob, a general entity)
/// is rebuilt under a new id, as in vanilla, so its old handle goes stale; a vehicle a
/// player rides keeps its id, and its handle still resolves in the new level. Follow
/// the entity's UUID across a change of level.
/// On a client the epoch identifies one client level and changes when that level
/// is replaced. Server reads and writes reject a handle from an earlier epoch,
/// of an entity that is gone, or used on a client. `Host::entity` makes a server
/// handle from an event's entity id; `ClientHost::crosshair_target` supplies a
/// client handle for an entity hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EntityRef {
    pub epoch: WorldEpoch,
    pub id: u32,
}

impl EntityRef {
    /// Create a handle for the game.
    // hidden: a game-side helper, not part of the mod surface
    #[doc(hidden)]
    pub fn new(epoch: WorldEpoch, id: u32) -> Self {
        Self { epoch, id }
    }
    /// The entity's id (the network id events carry).
    pub fn id(self) -> u32 {
        self.id
    }
    /// The open of the world the handle belongs to.
    pub fn epoch(self) -> WorldEpoch {
        self.epoch
    }
}

/// A living entity of the open world: a mob, a player, an armor stand or the
/// dragon (vanilla's `LivingEntity`). `host.living(entity)` makes one from an
/// [`EntityRef`] of such an entity, and [`LivingRef::entity`] goes back, so the
/// entity's methods work on it too. Stale handles read as `None`, as an
/// `EntityRef`'s do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LivingRef(pub EntityRef);

impl LivingRef {
    /// Create a handle for the game.
    // hidden: a game-side helper, not part of the mod surface
    #[doc(hidden)]
    pub fn new(entity: EntityRef) -> Self {
        Self(entity)
    }
    /// The same entity as an [`EntityRef`].
    pub fn entity(self) -> EntityRef {
        self.0
    }
    /// The entity's id (the network id events carry).
    pub fn id(self) -> u32 {
        self.0.id()
    }
}

impl From<LivingRef> for EntityRef {
    fn from(living: LivingRef) -> Self {
        living.0
    }
}

/// How an attribute modifier changes its attribute
/// (`AttributeModifier.Operation`). The value is the base plus every
/// `AddValue`, then grows by that sum times each `AddMultipliedBase`, then is
/// multiplied by one plus each `AddMultipliedTotal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, )]
pub enum AttributeOperation {
    /// `add_value`: adds the amount.
    AddValue,
    /// `add_multiplied_base`: adds the amount times the base.
    AddMultipliedBase,
    /// `add_multiplied_total`: multiplies by one plus the amount.
    AddMultipliedTotal,
}

/// An attribute modifier (vanilla's `AttributeModifier`): its id, which is
/// unique on one attribute of one entity, the amount and the operation.
#[derive(Clone, Debug, PartialEq, )]
pub struct AttributeModifier {
    /// The modifier's id (`mymod:sprint_boost`).
    pub id: Identifier,
    /// The amount the operation applies.
    pub amount: f64,
    /// How it applies.
    pub operation: AttributeOperation,
}

impl AttributeModifier {
    /// A modifier with id `id`.
    pub fn new(id: Identifier, amount: f64, operation: AttributeOperation) -> Self {
        Self {
            id,
            amount,
            operation,
        }
    }
}

/// Whether an attribute modifier is saved with its entity
/// (`AttributeInstance.addPermanentModifier`) or lasts until the entity
/// unloads (`addTransientModifier`), as the modifiers of worn items and
/// effects do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, )]
pub enum ModifierLifetime {
    /// Not saved: gone when the entity is saved and loaded again.
    Transient,
    /// Saved with the entity.
    Permanent,
}

/// The answer to `mob_effect_applicable`: whether an effect can land on an
/// entity. An unanswered hook defers to the vanilla tag check; `Allow` and
/// `Deny` override it, as NeoForge's `MobEffectEvent.Applicable.Result`
/// does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectApplicable {
    /// The effect lands, whatever the tags say.
    Allow,
    /// The effect is refused, whatever the tags say.
    Deny,
}

/// A display's `Transformation`: a translation, a left rotation, a scale and a
/// right rotation. Quaternions are `[x, y, z, w]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transformation {
    /// The offset in blocks.
    pub translation: [f32; 3],
    /// The rotation applied after the scale, as a quaternion (x, y, z, w).
    pub left_rotation: [f32; 4],
    /// The scale on each axis.
    pub scale: [f32; 3],
    /// The rotation applied before the scale, as a quaternion (x, y, z, w).
    pub right_rotation: [f32; 4],
}

impl Transformation {
    /// `Transformation.IDENTITY`.
    pub const IDENTITY: Self = Self {
        translation: [0.0; 3],
        left_rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0; 3],
        right_rotation: [0.0, 0.0, 0.0, 1.0],
    };
}

/// `ChatVisiblity`: which chat a client shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatVisibility {
    /// All chat.
    Full,
    /// Only system messages and commands.
    System,
    /// No chat.
    Hidden,
}

/// `HumanoidArm`: a player's main hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainHand {
    /// The left hand.
    Left,
    /// The right hand.
    Right,
}

/// A player's client options, as `ServerboundClientInformationPacket` carries them
/// (vanilla's `ClientInformation`). Raw ids map as vanilla's `ByIdMap` does: chat
/// visibility and particle status wrap (`floorMod(id, 3)`), the main hand reads an
/// unknown id as left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientInfo {
    /// The client's language code, such as `en_us`.
    pub language: String,
    /// The client's view distance in chunks.
    pub view_distance: i8,
    /// Which chat the client shows.
    pub chat_visibility: ChatVisibility,
    /// Whether the client shows chat colours.
    pub chat_colors: bool,
    /// Skin layer bits (`PlayerModelPart`).
    pub model_customisation: u8,
    /// The player's main hand.
    pub main_hand: MainHand,
    /// Whether the client asked for filtered text.
    pub text_filtering: bool,
    /// Whether the player shows in the server list sample.
    pub allows_listing: bool,
    /// How many particles the client draws.
    pub particle_status: ParticleStatus,
}

/// An online player in one open server world, keyed by UUID.
/// This is the player's identity, not a particular body: handles for the same
/// player stay equal across respawn. Respawning does not invalidate a handle.
/// A nested guest turn cannot re-enter a player active in an outer turn;
/// operations requiring that player's swapped state report them unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerRef {
    pub entity: EntityRef,
    pub uuid: u128,
}

impl PlayerRef {
    /// The same player as an entity handle.
    pub fn entity(self) -> EntityRef {
        self.entity
    }
    /// The player's UUID.
    pub fn uuid(self) -> u128 {
        self.uuid
    }
}

impl From<PlayerRef> for EntityRef {
    fn from(player: PlayerRef) -> Self {
        player.entity
    }
}

/// One property of a [`GameProfile`] (authlib's `Property`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileProperty {
    /// The property's name, such as `textures`.
    pub name: String,
    /// Its value, as the session server sent it (base64 for `textures`).
    pub value: String,
    /// The session server's signature, when the value is signed.
    pub signature: Option<String>,
}

impl ProfileProperty {
    /// A property from its name, value and optional signature.
    pub fn new(name: String, value: String, signature: Option<String>) -> Self {
        Self {
            name,
            value,
            signature,
        }
    }
}

/// A change requested by an effect tick, applied after its hooks return.
// hidden: a game-side type, not part of the mod surface
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub enum EffectTickChange {
    Heal(f32),
    Hurt(String, f32),
    AddExhaustion(f32),
    Eat(i32, f32),
}
