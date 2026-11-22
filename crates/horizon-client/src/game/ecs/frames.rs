//! The acts of the item frames and paintings that P reports as `FrameEvent`s: the sound, the game events, the comparators, the block
//! change and the drops the host plays. P has decided each act already; one arm per variant, none left to `_`.
use super::super::Game;
use bevy_ecs::entity::Entity;
use horizon_client::entities::{ItemFrame, Painting};
use horizon_client::items::ItemStack;
use horizon_core::nbt::{JavaString, Tag};
use horizon_ecs_proto::bridge::select::Select;
use horizon_ecs_proto::{host_out_frames::FrameEvent, targeting};

/// The sound and item name prefix of a frame: `glow_item_frame` for a glow frame.
fn frame_kind(glow: bool) -> &'static str {
    if glow { "glow_item_frame" } else { "item_frame" }
}

impl Game<'_> {
    pub(in crate::game) fn ecs_frame_events(&mut self, evs: Vec<FrameEvent>) {
        for ev in evs {
            match ev {
                FrameEvent::Placed { who, spot } => {
                    let c = block_centre(spot.at);
                    self.audio.play(&format!("minecraft:entity.{}.place", frame_kind(spot.glow)), Some(c), 1.0, 1.0);
                    let host = self.host_who_opt(who);
                    let ctx = self.who_context(host, None);
                    self.post_game_event("minecraft:entity_place", c, ctx);
                    self.world.modified.insert((spot.at[0] >> 4, spot.at[2] >> 4));
                }
                FrameEvent::PaintingPlaced { who, spot } => {
                    let c = block_centre(spot.at);
                    self.audio.play("minecraft:entity.painting.place", Some(c), 1.0, 1.0);
                    let host = self.host_who_opt(who);
                    let ctx = self.who_context(host, None);
                    self.post_game_event("minecraft:entity_place", c, ctx);
                    self.world.modified.insert((spot.at[0] >> 4, spot.at[2] >> 4));
                }
                FrameEvent::Put { who, spot, stack } => {
                    let c = block_centre(spot.at);
                    self.audio.play(&format!("minecraft:entity.{}.add_item", frame_kind(spot.glow)), Some(c), 1.0, 1.0);
                    self.frame_output_changed(spot.at);
                    let host = self.host_who_opt(who);
                    let ctx = self.who_context(host, None);
                    self.post_game_event("minecraft:block_change", c, ctx);
                    self.world.modified.insert((spot.at[0] >> 4, spot.at[2] >> 4));
                    if matches!(who, Some(targeting::Who::Player(_))) && !self.creative {
                        self.consume_held();
                    }
                    if let Some(id) = stack.data().and_then(|d| d.map_id) {
                        self.update_map(id);
                    }
                }
                FrameEvent::Rotated { who, spot } => {
                    let c = block_centre(spot.at);
                    self.audio.play(&format!("minecraft:entity.{}.rotate_item", frame_kind(spot.glow)), Some(c), 1.0, 1.0);
                    self.frame_output_changed(spot.at);
                    let host = self.host_who_opt(who);
                    let ctx = self.who_context(host, None);
                    self.post_game_event("minecraft:block_change", c, ctx);
                    self.world.modified.insert((spot.at[0] >> 4, spot.at[2] >> 4));
                }
                FrameEvent::Removed { who, spot, stack, drop, drop_at } => {
                    let c = block_centre(spot.at);
                    self.audio.play(&format!("minecraft:entity.{}.remove_item", frame_kind(spot.glow)), Some(c), 1.0, 1.0);
                    self.frame_output_changed(spot.at);
                    if drop {
                        self.spawn_item(stack, drop_at, [0.0; 3], 10);
                    }
                    let host = self.host_who_opt(who);
                    let ctx = self.who_context(host, None);
                    self.post_game_event("minecraft:block_change", c, ctx);
                    self.world.modified.insert((spot.at[0] >> 4, spot.at[2] >> 4));
                }
                FrameEvent::Broke { who, spot, drop, drop_at } => {
                    let c = block_centre(spot.at);
                    self.audio.play(&format!("minecraft:entity.{}.break", frame_kind(spot.glow)), Some(c), 1.0, 1.0);
                    if drop {
                        let item = if spot.glow { "minecraft:glow_item_frame" } else { "minecraft:item_frame" };
                        self.spawn_item(ItemStack::new(item, 1), drop_at, [0.0; 3], 10);
                    }
                    let host = self.host_who_opt(who);
                    let ctx = self.who_context(host, None);
                    self.post_game_event("minecraft:block_change", c, ctx);
                    self.world.modified.insert((spot.at[0] >> 4, spot.at[2] >> 4));
                }
                FrameEvent::PaintingBroke { spot, drop, drop_at, .. } => {
                    let c = block_centre(spot.at);
                    self.audio.play("minecraft:entity.painting.break", Some(c), 1.0, 1.0);
                    if drop {
                        self.spawn_item(ItemStack::new("minecraft:painting", 1), drop_at, [0.0; 3], 10);
                    }
                    self.world.modified.insert((spot.at[0] >> 4, spot.at[2] >> 4));
                }
            }
        }
    }
}

/// The centre of the block at `at`: the host plays from the block's centre.
impl Game<'_> {
    /// The item frame `e` as the client's list holds one, read from its save compound; None without a network id or a block position.
    pub(in crate::game) fn frame_from_ecs(&self, e: Entity) -> Option<ItemFrame> {
        let id = self.ecs.ecs()?.id_of(e)?;
        let c = self.ecs.entity_nbt(e)?;
        let byte = |k: &str| match c.get(&JavaString::from(k)) {
            Some(Tag::Byte(v)) => Some(*v),
            _ => None,
        };
        let Some(Tag::IntArray(p)) = c.get(&JavaString::from("block_pos")) else { return None };
        let pos: [i32; 3] = p.as_slice().try_into().ok()?;
        let glow = matches!(c.get(&JavaString::from("id")), Some(Tag::String(s)) if s.to_string_lossy() == "minecraft:glow_item_frame");
        let drop_chance = match c.get(&JavaString::from("ItemDropChance")) {
            Some(Tag::Float(b)) => f32::from_bits(*b),
            _ => 1.0,
        };
        Some(ItemFrame {
            attachment_lifetime: Default::default(),
            join: Default::default(),
            id,
            pos,
            facing: byte("Facing")?.clamp(0, 5) as u8,
            glow,
            item: c.get(&JavaString::from("Item")).and_then(horizon_items::item_nbt::item_from_nbt),
            rotation: byte("ItemRotation").unwrap_or(0) % 8,
            drop_chance,
            fixed: byte("Fixed").is_some_and(|v| v != 0),
            invisible: byte("Invisible").is_some_and(|v| v != 0),
            check_interval: 0,
            custom_name: None,
            random: horizon_core::random::EntityRandom::own(horizon_client::random_source::entity()),
        })
    }
    /// The painting `e` as the client's list holds one: its variant's size comes from the motive table.
    pub(in crate::game) fn painting_from_ecs(&self, e: Entity) -> Option<Painting> {
        let id = self.ecs.ecs()?.id_of(e)?;
        let c = self.ecs.entity_nbt(e)?;
        let Some(Tag::IntArray(p)) = c.get(&JavaString::from("block_pos")) else { return None };
        let pos: [i32; 3] = p.as_slice().try_into().ok()?;
        let Some(Tag::Byte(facing)) = c.get(&JavaString::from("facing")) else { return None };
        let Some(Tag::String(name)) = c.get(&JavaString::from("variant")) else { return None };
        let variant = self.painting_variants.get(&name.to_string_lossy())?;
        Some(Painting {
            attachment_lifetime: Default::default(),
            join: Default::default(),
            id,
            pos,
            facing: (*facing).clamp(0, 3) as u8,
            variant: variant.id.clone(),
            width: variant.width,
            height: variant.height,
            check_interval: 0,
        })
    }
    /// Every item frame the ECS holds, read on demand (none is kept in a list).
    pub(in crate::game) fn ecs_frames(&self) -> Vec<ItemFrame> {
        ["minecraft:item_frame", "minecraft:glow_item_frame"]
            .into_iter()
            .flat_map(|kind| self.ecs.select(&Select { kind: Some(kind.into()), ..Default::default() }))
            .filter_map(|e| self.frame_from_ecs(e))
            .collect()
    }
    /// Every painting the ECS holds, read on demand (none is kept in a list).
    pub(in crate::game) fn ecs_paintings(&self) -> Vec<Painting> {
        self.ecs
            .select(&Select { kind: Some("minecraft:painting".into()), ..Default::default() })
            .into_iter()
            .filter_map(|e| self.painting_from_ecs(e))
            .collect()
    }
}

fn block_centre(at: [i32; 3]) -> [f64; 3] {
    std::array::from_fn(|i| f64::from(at[i]) + 0.5)
}
