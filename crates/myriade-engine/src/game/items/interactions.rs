//! Server `minecraft:interaction` entities (`Interaction`): invisible boxes a
//! server (Denizen, Nexo) uses for clickable areas. They are pickable, render
//! nothing, and an interact ends at them: `SUCCESS` when they respond (the arm
//! swings), `CONSUME` otherwise (the click still goes to the server, nothing
//! behind them is used).
use super::{Game, ray_box};
use horizon_core::geometry::{Aabb, Vec3};

/// `Interaction`'s synced data and box: `DATA_WIDTH` (8), `DATA_HEIGHT` (9)
/// and `DATA_RESPONSE` (10), from its `defineSynchedData`.
pub(super) struct RemoteInteraction {
    pub position: Vec3,
    pub width: f32,
    pub height: f32,
    /// `Entity.getVehicle`: ModelEngine's hitboxes ride the pivot that
    /// carries them.
    pub vehicle: Option<u32>,
}

impl RemoteInteraction {
    /// `Entity.positionRider`'s move: a passenger is placed on its vehicle's
    /// seat, not moved on its own.
    pub fn ride_to(&mut self, at: glam::DVec3) {
        self.position = Vec3::new(at.x, at.y, at.z);
    }
    /// `Interaction.refreshDimensions`: `width` square about the entity's
    /// x and z, from its feet up `height` (its position is the box's bottom
    /// centre, not the model's).
    pub fn bounds(&self) -> Aabb {
        let half = f64::from(self.width) / 2.0;
        let p = self.position;
        Aabb::new(
            Vec3::new(p.x - half, p.y, p.z - half),
            Vec3::new(
                p.x + half,
                p.y + f64::from(self.height.max(0.0)),
                p.z + half,
            ),
        )
    }
}

impl Game<'_> {
    /// `Interaction` under the crosshair within the entity reach, as its id
    /// and the hit distance along the view.
    pub(super) fn interaction_hit(&self) -> Option<(i32, f64)> {
        let (eye, dir) = self.aim()?;
        let reach = self.entity_reach;
        self.interactions
            .iter()
            .filter_map(|(&id, i)| ray_box(eye, dir, reach, i.bounds()).map(|d| (id, d)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_riding_hitbox_moves_with_its_pivot() {
        let mut i = RemoteInteraction {
            position: Vec3::new(10.0, 64.0, -3.0),
            width: 1.0,
            height: 1.0,
            vehicle: Some(9),
        };
        i.ride_to(glam::DVec3::new(12.0, 65.5, -3.0));
        assert_eq!(i.position, Vec3::new(12.0, 65.5, -3.0));
        let b = i.bounds();
        assert_eq!(
            (b.min_position().components(), b.max_position().components()),
            ([11.5, 65.5, -3.5], [12.5, 66.5, -2.5])
        );
    }
}

/// Whether an entity id names `minecraft:interaction`.
pub(super) fn is_interaction(kind: &str) -> bool {
    matches!(kind, "minecraft:interaction" | "interaction")
}

/// `Interaction.PlayerAction`: the player and the game time of the last
/// attack or use, as `PlayerAction.CODEC` saves it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PlayerAction {
    pub player: [i32; 4],
    pub timestamp: i64,
}

impl PlayerAction {
    fn from_tag(tag: Option<&horizon_core::nbt::Tag>) -> Option<Self> {
        use horizon_core::nbt::Tag;
        let tag = tag?;
        let Tag::IntArray(player) = tag.get("player")? else {
            return None;
        };
        let timestamp = myriade_engine::save::number(tag.get("timestamp"))?;
        Some(Self {
            player: player.as_slice().try_into().ok()?,
            timestamp,
        })
    }
    fn to_tag(self) -> horizon_core::nbt::Tag {
        use horizon_core::nbt::Tag;
        Tag::compound([
            ("player", Tag::IntArray(self.player.to_vec())),
            ("timestamp", Tag::Long(self.timestamp)),
        ])
    }
}

/// A server `Interaction`'s saved settings
/// (`Interaction.readAdditionalSaveData` / `addAdditionalSaveData`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct InteractionState {
    pub width: f32,
    pub height: f32,
    pub response: bool,
    pub attack: Option<PlayerAction>,
    pub interaction: Option<PlayerAction>,
}

impl InteractionState {
    /// `Interaction.readAdditionalSaveData`: the dimensions, optional action records and response.
    pub fn from_tag(tag: &horizon_core::nbt::Tag) -> Self {
        let float = |key| myriade_engine::save::float(tag.get(key)).unwrap_or(1.0) as f32;
        Self {
            width: float("width"),
            height: float("height"),
            response: myriade_engine::entity_nbt::flag(tag, "response"),
            attack: PlayerAction::from_tag(tag.get("attack")),
            interaction: PlayerAction::from_tag(tag.get("interaction")),
        }
    }
    /// `Interaction.addAdditionalSaveData`: the settings, and each action when set.
    pub fn write(&self, c: &mut horizon_core::nbt::Compound) {
        use horizon_core::nbt::Tag;
        c.insert("width".into(), Tag::Float(self.width.to_bits()));
        c.insert("height".into(), Tag::Float(self.height.to_bits()));
        for (key, action) in [("attack", self.attack), ("interaction", self.interaction)] {
            match action {
                Some(action) => {
                    c.insert(key.into(), action.to_tag());
                }
                None => {
                    c.remove(&key.into());
                }
            }
        }
        c.insert("response".into(), Tag::Byte(i8::from(self.response)));
    }
}
