use crate::*;


/// A profession's persistent resource identity.
#[derive(Clone, Debug, PartialEq, Eq, )]
pub struct ProfessionKey(pub Identifier);

/// A point-of-interest resource identity.
#[derive(Clone, Debug, PartialEq, Eq, )]
pub struct PoiKey(pub Identifier);

/// A trade-set resource identity, independent of the profession's name.
#[derive(Clone, Debug, PartialEq, Eq, )]
pub struct TradeSetKey(pub Identifier);

/// Declarative profession metadata, collected once for the owning mod load.
/// References resolve together before any candidate table becomes active.
#[derive(Clone, Debug, PartialEq, Eq, )]
pub struct ProfessionDefinition {
    /// Persistent qualified identity; the minecraft namespace is reserved.
    pub key: ProfessionKey,
    /// Custom POI block-state selectors owned by this profession.
    pub workstations: Vec<ProfessionWorkstation>,
    /// POIs at which an existing job remains valid.
    pub held_poi: Vec<PoiKey>,
    /// POIs from which an unemployed villager may acquire this job.
    pub acquirable_poi: Vec<PoiKey>,
    /// Items requested by this profession; this grants no farmer AI.
    pub requested_items: Vec<Identifier>,
    /// Secondary work-site blocks.
    pub secondary_blocks: Vec<Identifier>,
    /// Optional work sound.
    pub work_sound: Option<Identifier>,
    /// Explicit level (1–5) to trade-set mappings; omitted levels add no offers.
    pub trades: Vec<(u8, TradeSetKey)>,
}

/// A custom workstation selector; unspecified block properties match every value.
#[derive(Clone, Debug, PartialEq, Eq, )]
pub struct ProfessionWorkstation {
    /// Custom POI identity, outside the minecraft namespace.
    pub poi: PoiKey,
    /// Block resource identity.
    pub block: Identifier,
    /// Required state property values.
    pub properties: Vec<(String, String)>,
}
