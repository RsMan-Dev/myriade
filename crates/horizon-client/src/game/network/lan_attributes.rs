//! The attribute instances a server syncs for the living entities it tracks
//! (`ClientboundUpdateAttributesPacket`, lan/attributes.rs): the ones
//! vanilla's `AttributeMap` would have created by then, since only those are
//! sent, never the type's other defaults.
use super::Game;
use horizon_client::lan::attributes::{instance, syncable};
use horizon_protocol::packets::play::{AttributeModifier, AttributeSnapshot};

impl Game<'_> {
    /// The registry id of a client-syncable attribute the entity type has
    /// (`AttributeMap.getInstance` is null for the others), by name.
    pub(in crate::game) fn synced_attribute(&self, entity: &str, name: &str) -> Option<i32> {
        let catalog = &self.motion.attributes;
        let id = catalog.id(name)?;
        (catalog.attribute(id).syncable && catalog.default_base(entity, id).is_some())
            .then(|| id.registry_id())
    }

    /// The type's base value of an attribute, as the instance starts.
    fn attribute_default(&self, entity: &str, name: &str) -> f64 {
        let catalog = &self.motion.attributes;
        catalog
            .id(name)
            .and_then(|id| catalog.default_base(entity, id))
            .unwrap_or(0.0)
    }

    /// The synced instances of the ECS entity `e` of type `kind`, read through the bridge: what its `AttrMods`, worn items and armour
    /// hold, with the type's default base where none was written. Only the syncable attributes the type has (`getSyncableAttributes`).
    pub(in crate::game) fn ecs_attributes(&self, e: bevy_ecs::entity::Entity, kind: &str) -> Vec<AttributeSnapshot> {
        use horizon_ecs_proto::attr_mod::Operation as Op;
        self.ecs
            .attribute_instances(e)
            .into_iter()
            .filter_map(|(name, base, mods)| {
                let full = format!("minecraft:{name}");
                let attribute = self.synced_attribute(kind, &full)?;
                Some(AttributeSnapshot {
                    attribute,
                    base: base.unwrap_or_else(|| self.attribute_default(kind, &full)),
                    modifiers: mods
                        .into_iter()
                        .map(|m| AttributeModifier {
                            id: m.id,
                            amount: m.amount,
                            operation: match m.operation {
                                Op::AddValue => 0,
                                Op::AddMultipliedBase => 1,
                                Op::AddMultipliedTotal => 2,
                            },
                        })
                        .collect(),
                })
            })
            .collect()
    }

    /// An armor stand's synced instances: movement speed (its first
    /// `aiStep`), and the live map's — its equipment's per-stack modifiers
    /// and what `/attribute` changed — built by the read if this is its
    /// first use.
    pub(super) fn stand_attributes(
        &self,
        live: &horizon_client::living_attributes::LivingAttributes,
    ) -> Vec<AttributeSnapshot> {
        const KIND: &str = "minecraft:armor_stand";
        let mut out = Vec::new();
        if let Some(a) = self.synced_attribute(KIND, "minecraft:movement_speed") {
            instance(
                &mut out,
                a,
                self.attribute_default(KIND, "minecraft:movement_speed"),
            );
        }
        let catalog = self.motion.attributes.clone();
        let _ = live.read(
            &catalog,
            KIND,
            "minecraft:movement_speed",
            horizon_world::attribute_map::ValueKind::Computed,
            Vec::new,
        );
        if let Some(map) = live.built() {
            for s in syncable(map) {
                let (attribute, base) = (s.attribute, s.base);
                *instance(&mut out, attribute, base) = s;
            }
        }
        out
    }

    /// The instances a server player's tick creates, and
    /// `ServerPlayer.updatePlayerAttributes`: a creative player reaches 0.5
    /// further to blocks and 2 to entities. Its client reads the reach from
    /// these synced modifiers. (The crouching player's waypoint range is
    /// server-side only: lan_waypoints.rs hides it.)
    pub(super) fn update_player_attributes(&mut self) {
        let creative = self.mode_id() == 1;
        let Some(p) = self.player.as_mut() else {
            return;
        };
        // LivingEntity.aiStep's removeFrost reads the movement speed instance
        // every tick, which creates it (and its first sync).
        if let Some(key) = p.attributes.key("minecraft:movement_speed") {
            let _ = p.attributes.get_instance(Some(key));
        }
        for (attribute, id, amount) in [
            (
                "minecraft:block_interaction_range",
                "minecraft:creative_mode_block_range",
                0.5,
            ),
            (
                "minecraft:entity_interaction_range",
                "minecraft:creative_mode_entity_range",
                2.0,
            ),
        ] {
            let Some(key) = p.attributes.key(attribute) else {
                continue;
            };
            // getAttribute creates the instance, a creative player's or not.
            if !matches!(p.attributes.get_instance(Some(key.clone())), Ok(Some(_))) {
                continue;
            }
            // addOrUpdateTransientModifier with the same modifier and
            // removeModifier of a missing one leave the instance clean.
            let has = p.attributes.has_modifier(Some(key), id).unwrap_or(false);
            if has != creative {
                let _ = p.set_modifier(attribute, id, amount, "add_value", creative);
            }
        }
    }
}
