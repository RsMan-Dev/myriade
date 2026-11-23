//! Dispenser behaviours: `DispenserBlock.getDispenseMethod(..).dispense` with
//! every `DispenseItemBehavior` registration of vanilla that Horizon has the
//! entities for. A behaviour returns what is left in the slot and the level
//! event it plays (`DefaultDispenseItemBehavior.dispense` then plays that
//! event and the smoke animation, event 2000).
use crate::game::cosmetic_random::cosmetic_random;
use super::level_sounds::LevelSound;
use super::{Game, item_abilities::ability};
use myriade_engine::{
    Result,
    collisions::SceneCollisions,
    containers::BlockEntity,
    items::ItemStack,
    mobs::{Species, equip::equippable, sulfur_cube},
    projectiles::{Projectile, ProjectileKind},
};
use myriade_types::SoundSource;
// Its old home, for mixin code that names it here.
use horizon_core::{
    geometry::{Aabb, Direction, Vec3},
    position::BlockPos,
    random::RandomSource,
};
use horizon_ecs_proto::goals;
use horizon_world::{
    fluid::FluidKind,
    level::{PlaceContext, PlaceOutcome},
    registry::BlockStateId,
};
use std::sync::Arc;

pub(super) type Pos = (i32, i32, i32);

/// The sound event of a plain click, a failed dispense and a launched projectile.
pub(super) const CLICK: i32 = 1000;
pub(super) const FAIL: i32 = 1001;
const LAUNCH: i32 = 1002;

fn step(d: Direction) -> [i32; 3] {
    match d {
        Direction::Down => [0, -1, 0],
        Direction::Up => [0, 1, 0],
        Direction::North => [0, 0, -1],
        Direction::South => [0, 0, 1],
        Direction::West => [-1, 0, 0],
        Direction::East => [1, 0, 0],
    }
}
pub(super) fn offset(p: Pos, d: Direction) -> Pos {
    let s = step(d);
    (p.0 + s[0], p.1 + s[1], p.2 + s[2])
}
/// `Direction.get3DDataValue`.
fn data_value(d: Direction) -> i32 {
    match d {
        Direction::Down => 0,
        Direction::Up => 1,
        Direction::North => 2,
        Direction::South => 3,
        Direction::West => 4,
        Direction::East => 5,
    }
}
/// `Direction.toYRot` (vertical directions give 270 through `data2d = -1`).
fn y_rot(d: Direction) -> f32 {
    match d {
        Direction::South => 0.0,
        Direction::West => 90.0,
        Direction::North => 180.0,
        Direction::East => 270.0,
        _ => 270.0,
    }
}
/// `DirectionalPlaceContext.getRotation`: `get2DDataValue() * 90`, so a
/// vertical direction (`data2d = -1`) gives -90 where `toYRot` gives 270: the
/// same horizontal facing, but the raw value a mod's placement hook reads.
fn place_rotation(d: Direction) -> f32 {
    match d {
        Direction::Up | Direction::Down => -90.0,
        _ => y_rot(d),
    }
}
fn block_box(p: Pos) -> Aabb {
    Aabb::new(
        Vec3::new(f64::from(p.0), f64::from(p.1), f64::from(p.2)),
        Vec3::new(f64::from(p.0 + 1), f64::from(p.1 + 1), f64::from(p.2 + 1)),
    )
}
/// `EntitySectionStorage.forEachAccessibleNonEmptySection`: x ascending, then
/// the packed key (`SectionPos.asLong`: z as 22 unsigned bits, then y as 20);
/// within a section the entities go in the order they joined, which ids follow.
fn section_order(at: Vec3) -> (i32, u32, u32) {
    let s = |v: f64| (v.floor() as i32) >> 4;
    let (x, y, z) = (s(at.x), s(at.y), s(at.z));
    (x, z as u32 & 0x3F_FFFF, y as u32 & 0xF_FFFF)
}
fn overlaps(a: Aabb, b: Aabb) -> bool {
    let (a0, a1, b0, b1) = (
        a.min_position(),
        a.max_position(),
        b.min_position(),
        b.max_position(),
    );
    a0.x < b1.x && b0.x < a1.x && a0.y < b1.y && b0.y < a1.y && a0.z < b1.z && b0.z < a1.z
}
/// `ItemStack.shrink(1)`; the slot is empty at zero.
fn shrink(mut stack: ItemStack) -> Option<ItemStack> {
    stack.count = stack.count.saturating_sub(1);
    (stack.count > 0).then_some(stack)
}
/// A dispenser's answer: the slot's new content and the level event played.
pub(super) type Dispensed = (Option<ItemStack>, i32);

thread_local! {
    /// `OptionalDispenseItemBehavior.success` of the registered behaviours
    /// that leave it unset on some paths (brush, TNT, honeycomb, chest).
    /// Vanilla keeps one behaviour instance per item for the whole run, so
    /// there the flag carries over from that item's previous dispense.
    static SUCCESS: std::cell::RefCell<std::collections::HashMap<&'static str, bool>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}
fn set_success(behaviour: &'static str, success: bool) {
    SUCCESS.with(|s| s.borrow_mut().insert(behaviour, success));
}
/// The level event `OptionalDispenseItemBehavior.playSound` plays for the flag.
fn optional_event(behaviour: &'static str) -> i32 {
    if SUCCESS.with(|s| s.borrow().get(behaviour).copied().unwrap_or(true)) {
        CLICK
    } else {
        FAIL
    }
}

/// Entities `getEntitiesOfClass(.., AABB(pos))` can find.
#[derive(Clone, Copy)]
enum Target {
    Player,
    /// A server player, by connection: the integrated owner and every guest.
    Guest(u32),
    Stand(u32),
    /// An entity of the ECS level, by network id.
    Ecs(u32),
}

/// `EquipmentSlot`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Slot {
    MainHand,
    OffHand,
    Feet,
    Legs,
    Chest,
    Head,
    Body,
    Saddle,
}
impl Slot {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::MainHand => "mainhand",
            Self::OffHand => "offhand",
            Self::Feet => "feet",
            Self::Legs => "legs",
            Self::Chest => "chest",
            Self::Head => "head",
            Self::Body => "body",
            Self::Saddle => "saddle",
        }
    }
    pub(super) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "mainhand" => Self::MainHand,
            "offhand" => Self::OffHand,
            "feet" => Self::Feet,
            "legs" => Self::Legs,
            "chest" => Self::Chest,
            "head" => Self::Head,
            "body" => Self::Body,
            "saddle" => Self::Saddle,
            _ => return None,
        })
    }
    /// The same slot in P's `goals::Slot`.
    pub(super) fn goal(self) -> goals::Slot {
        match self {
            Self::MainHand => goals::Slot::Main,
            Self::OffHand => goals::Slot::Off,
            Self::Feet => goals::Slot::Feet,
            Self::Legs => goals::Slot::Legs,
            Self::Chest => goals::Slot::Chest,
            Self::Head => goals::Slot::Head,
            Self::Body => goals::Slot::Body,
            Self::Saddle => goals::Slot::Saddle,
        }
    }
    /// The player's armour array index (head, chest, legs, feet).
    fn armor_index(self) -> Option<usize> {
        match self {
            Self::Head => Some(0),
            Self::Chest => Some(1),
            Self::Legs => Some(2),
            Self::Feet => Some(3),
            _ => None,
        }
    }
    /// `ArmorStand` equipment index (mainhand, offhand, feet, legs, chest, head).
    fn stand_index(self) -> Option<usize> {
        match self {
            Self::MainHand => Some(0),
            Self::OffHand => Some(1),
            Self::Feet => Some(2),
            Self::Legs => Some(3),
            Self::Chest => Some(4),
            Self::Head => Some(5),
            _ => None,
        }
    }
}

fn equine(s: Species) -> bool {
    matches!(
        s,
        Species::Horse
            | Species::Donkey
            | Species::Mule
            | Species::SkeletonHorse
            | Species::ZombieHorse
            | Species::Camel
            | Species::CamelHusk
            | Species::Llama
            | Species::TraderLlama
    )
}
/// `LivingEntity.canUseSlot` and its overrides, from the species, whether it is grown and whether it is tame.
fn can_use_slot(s: Species, grown: bool, tame: bool, slot: Slot) -> bool {
    match s {
        Species::Pig | Species::Strider if slot == Slot::Saddle => grown,
        Species::Horse
        | Species::SkeletonHorse
        | Species::ZombieHorse
        | Species::Llama
        | Species::TraderLlama => true,
        _ if equine(s) && slot == Slot::Saddle => grown && tame,
        _ if myriade_engine::mobs::nautilus::is_nautilus(s)
            && matches!(slot, Slot::Saddle | Slot::Body) =>
        {
            grown && tame
        }
        Species::SulfurCube | Species::HappyGhast if slot == Slot::Body => grown,
        _ => true,
    }
}
/// `canDispenserEquipIntoSlot` and its overrides, from the species, the loot pickup and whether it is tame.
fn can_dispenser_equip(s: Species, loot: bool, tame: bool, slot: Slot) -> bool {
    match s {
        Species::Pig | Species::Strider => slot == Slot::Saddle || loot,
        _ if equine(s) => matches!(slot, Slot::Body | Slot::Saddle) && tame || loot,
        _ if myriade_engine::mobs::nautilus::is_nautilus(s) => {
            matches!(slot, Slot::Body | Slot::Saddle) || loot
        }
        Species::SulfurCube | Species::HappyGhast => slot == Slot::Body,
        Species::Dolphin | Species::Panda | Species::Fox => slot == Slot::MainHand && loot,
        Species::Allay => false,
        _ => loot,
    }
}
/// `getEquipSound` from the species and whether the eyes are under water.
pub(super) fn equip_sound_of(s: Species, underwater: bool, slot: Slot, default: Option<String>) -> Option<String> {
    if slot != Slot::Saddle {
        return default;
    }
    Some(match s {
        Species::Pig => "minecraft:entity.pig.saddle".into(),
        Species::Strider => "minecraft:entity.strider.saddle".into(),
        Species::Camel => "minecraft:entity.camel.saddle".into(),
        Species::CamelHusk => "minecraft:entity.camel_husk.saddle".into(),
        _ if equine(s) => "minecraft:entity.horse.saddle".into(),
        _ if myriade_engine::mobs::nautilus::is_nautilus(s) => {
            if underwater {
                "minecraft:item.nautilus_saddle_underwater_equip".into()
            } else {
                "minecraft:item.nautilus_saddle_equip".into()
            }
        }
        _ => return default,
    })
}

impl Game<'_> {
    fn prop(&self, state: BlockStateId, key: &str) -> Option<String> {
        self.pack
            .assets
            .registry
            .state(state)?
            .properties
            .get(key)
            .cloned()
    }
    /// The same block with other properties (`withProperty`).
    pub(super) fn with_props(
        &self,
        state: BlockStateId,
        props: &[(&str, &str)],
    ) -> Option<BlockStateId> {
        let mut s = self.pack.assets.registry.state(state)?.clone();
        for (k, v) in props {
            s.properties.insert((*k).to_owned(), (*v).to_owned());
        }
        self.pack.assets.registry.resolve(&s).ok()
    }
    fn block_at(&self, p: Pos) -> BlockStateId {
        self.world.block(p.0, p.1, p.2)
    }
    fn dispenser_block_name(&self, p: Pos) -> std::sync::Arc<str> {
        self.res.facts().get(self.block_at(p)).name.clone()
    }
    fn in_block_tag(&self, tag: &str, block: &str) -> bool {
        self.res
            .items()
            .block_tags
            .get(tag)
            .is_some_and(|s| s.contains(block))
    }
    fn water_at(&self, p: Pos) -> bool {
        matches!(
            self.res.behaviours().fluid_kind(self.block_at(p)),
            FluidKind::Water | FluidKind::FlowingWater
        )
    }
    fn center(p: Pos) -> [f64; 3] {
        [
            f64::from(p.0) + 0.5,
            f64::from(p.1) + 0.5,
            f64::from(p.2) + 0.5,
        ]
    }
    pub(super) fn pos_of(p: Pos) -> BlockPos {
        BlockPos::new(p.0, p.1, p.2)
    }

    // ---- level events (LevelEventHandler) ----

    /// The dispenser level events: 1000 click, 1001 fail, 1002 launch, 1004
    /// firework shoot, 1018 blaze shoot and 2000 smoke shot out of the front.
    pub(super) fn dispenser_event(&mut self, id: i32, pos: Pos, _data: i32) {
        let c = Self::center(pos);
        match id {
            1000 => self
                .audio
                .play("minecraft:block.dispenser.dispense", Some(c), 1.0, 1.0),
            1001 => self
                .audio
                .play("minecraft:block.dispenser.fail", Some(c), 1.0, 1.2),
            1002 => self
                .audio
                .play("minecraft:block.dispenser.launch", Some(c), 1.0, 1.2),
            1004 => self
                .audio
                .play("minecraft:entity.firework_rocket.shoot", Some(c), 1.0, 1.2),
            // LevelEventHandler runs on the client: its pitch draws come from
            // the client level's random, never the server stream.
            1018 => {
                let mut r = cosmetic_random();
                let pitch = (r.next_float() - r.next_float()) * 0.2 + 1.0;
                self.audio
                    .play("minecraft:entity.blaze.shoot", Some(c), 2.0, pitch);
            }
            1051 => {
                let pitch = 0.4 / (cosmetic_random().next_float() * 0.4 + 0.8);
                self.audio
                    .play("minecraft:entity.wind_charge.throw", Some(c), 0.5, pitch);
            }
            1049 => self
                .audio
                .play("minecraft:block.crafter.craft", Some(c), 1.0, 1.0),
            1050 => self
                .audio
                .play("minecraft:block.crafter.fail", Some(c), 1.0, 1.0),
            _ => {}
        }
    }

    // ---- DefaultDispenseItemBehavior ----

    /// `DispenserBlock.getDispensePosition(source, scale, offset)`; vanilla adds
    /// `scale * step + offset` first, then adds that sum to the block centre.
    fn dispense_position(pos: Pos, facing: Direction, scale: f64, offset: [f64; 3]) -> Vec3 {
        let s = step(facing);
        let c = Self::center(pos);
        Vec3::new(
            c[0] + (scale * f64::from(s[0]) + offset[0]),
            c[1] + (scale * f64::from(s[1]) + offset[1]),
            c[2] + (scale * f64::from(s[2]) + offset[2]),
        )
    }
    /// `DefaultDispenseItemBehavior.spawnItem` at `getDispensePosition`.
    pub(super) fn spawn_dispensed(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
        accuracy: i32,
    ) {
        let s = step(facing);
        let p = Self::dispense_position(pos, facing, 0.7, [0.0; 3]);
        let mut at = [p.x, p.y, p.z];
        at[1] -= if s[1] != 0 { 0.125 } else { 0.156_25 };
        let r = &mut self.world.random;
        let pow = r.next_double() * 0.1 + 0.2;
        let spread = 0.017_227_5 * f64::from(accuracy);
        let v = [
            r.triangle(f64::from(s[0]) * pow, spread),
            r.triangle(0.2, spread),
            r.triangle(f64::from(s[2]) * pow, spread),
        ];
        self.spawn_item_thrown(stack, at, v);
    }
    /// `DefaultDispenseItemBehavior.execute`: drop one item.
    fn default_execute(
        &mut self,
        pos: Pos,
        facing: Direction,
        mut stack: ItemStack,
    ) -> Option<ItemStack> {
        let one = ItemStack {
            count: 1,
            ..stack.clone()
        };
        stack.count -= 1;
        self.spawn_dispensed(pos, facing, one, 6);
        (stack.count > 0).then_some(stack)
    }
    /// `new DefaultDispenseItemBehavior().dispense(..)` including its events.
    pub(super) fn dispense_default_in_level(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Option<ItemStack> {
        let left = self.default_execute(pos, facing, stack);
        self.dispenser_event(CLICK, pos, 0);
        self.dispenser_event(2000, pos, data_value(facing));
        left
    }
    /// `DefaultDispenseItemBehavior.consumeWithRemainder`: the remainder goes
    /// into the dispenser (`insertItem`) or is dropped.
    pub(super) fn consume_with_remainder(
        &mut self,
        pos: Pos,
        facing: Direction,
        mut stack: ItemStack,
        remainder: ItemStack,
    ) -> Option<ItemStack> {
        stack.count -= 1;
        if stack.count == 0 {
            return Some(remainder);
        }
        let mut rest = remainder;
        if let Some(entity) = self.world.block_entities.get_mut(&pos) {
            let max = self
                .res
                .items()
                .get(&rest.item)
                .map_or(64, |i| i.max_stack)
                .max(1);
            for cell in entity.slots_mut() {
                match cell {
                    None => {
                        let n = rest.count.min(max);
                        *cell = Some(ItemStack {
                            count: n,
                            ..rest.clone()
                        });
                        rest.count -= n;
                    }
                    Some(e) if e.same_item_same_components(&rest) => {
                        let n = rest.count.min(max.saturating_sub(e.count));
                        e.count += n;
                        rest.count -= n;
                    }
                    Some(_) => {}
                }
                if rest.count == 0 {
                    break;
                }
            }
        }
        if rest.count > 0 {
            self.spawn_dispensed(pos, facing, rest, 6);
            self.dispenser_event(CLICK, pos, 0);
            self.dispenser_event(2000, pos, data_value(facing));
        }
        Some(stack)
    }
    /// `ItemStack.hurtAndBreak(amount)` without an entity.
    fn hurt_and_break(&mut self, stack: ItemStack, amount: u32) -> Option<ItemStack> {
        self.damage_stack(&stack, amount.min(i32::MAX as u32) as i32, false)
            .0
    }

    // ---- the registry ----

    /// `DispenserBlock.getDispenseMethod(..).dispense(source, stack)`: what the
    /// slot holds afterwards.
    pub(super) fn dispense_item(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Option<ItemStack>> {
        let item = stack.item.to_string();
        // A stack able to `shears_harvest` is dispensed as shears are, whatever its item.
        let shears = self.stack_shears(&stack, ability::SHEARS_HARVEST);
        let (left, event) = match item.as_str() {
            _ if shears => self.dispense_shears(pos, facing, stack)?,

            "minecraft:arrow"
            | "minecraft:tipped_arrow"
            | "minecraft:spectral_arrow"
            | "minecraft:egg"
            | "minecraft:blue_egg"
            | "minecraft:brown_egg"
            | "minecraft:snowball"
            | "minecraft:experience_bottle"
            | "minecraft:splash_potion"
            | "minecraft:lingering_potion"
            | "minecraft:firework_rocket"
            | "minecraft:fire_charge"
            | "minecraft:wind_charge" => self.dispense_projectile(pos, facing, stack),
            "minecraft:armor_stand" => self.dispense_armor_stand(pos, facing, stack),
            "minecraft:chest" => self.dispense_chest(pos, facing, stack),
            "minecraft:lava_bucket"
            | "minecraft:water_bucket"
            | "minecraft:powder_snow_bucket"
            | "minecraft:salmon_bucket"
            | "minecraft:cod_bucket"
            | "minecraft:pufferfish_bucket"
            | "minecraft:tropical_fish_bucket"
            | "minecraft:axolotl_bucket"
            | "minecraft:tadpole_bucket"
            | "minecraft:sulfur_cube_bucket" => self.dispense_filled_bucket(pos, facing, stack)?,
            "minecraft:bucket" => self.dispense_empty_bucket(pos, facing, stack)?,
            "minecraft:flint_and_steel" => self.dispense_flint_and_steel(pos, facing, stack)?,
            "minecraft:bone_meal" => self.dispense_bone_meal(pos, facing, stack)?,
            "minecraft:tnt" => self.dispense_tnt(pos, facing, stack),
            "minecraft:wither_skeleton_skull" => self.dispense_wither_skull(pos, facing, stack)?,
            "minecraft:carved_pumpkin" => self.dispense_carved_pumpkin(pos, facing, stack)?,
            "minecraft:glass_bottle" => self.dispense_glass_bottle(pos, facing, stack)?,
            "minecraft:glowstone" => self.dispense_glowstone(pos, facing, stack)?,
            "minecraft:brush" => self.dispense_brush(pos, facing, stack),
            "minecraft:honeycomb" => self.dispense_honeycomb(pos, facing, stack)?,
            "minecraft:potion" => self.dispense_potion(pos, facing, stack)?,
            _ if item.ends_with("shulker_box") => self.dispense_shulker_box(pos, facing, stack)?,
            _ if super::boats::any_boat_wood(&item).is_some() => {
                self.dispense_boat(pos, facing, stack)
            }
            _ if super::minecarts::CartKind::from_id(&item).is_some() => {
                self.dispense_minecart(pos, facing, stack)
            }
            _ => self.dispense_default_method(pos, facing, stack)?,
        };
        self.dispenser_event(event, pos, 0);
        self.dispenser_event(2000, pos, data_value(facing));
        Ok(left)
    }
    /// `DispenserBlock.getDefaultDispenseMethod`: equipment, spawn eggs, or a drop.
    fn dispense_default_method(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        // `ItemStack.has(DataComponents.EQUIPPABLE)`: the stack's own, a removed one included.
        if equippable(self.items(), &stack).is_some() {
            let mut stack = stack;
            if self.dispense_equipment(pos, facing, &mut stack) {
                return Ok((Some(stack).filter(|s| s.count > 0), CLICK));
            }
            return Ok((self.default_execute(pos, facing, stack), CLICK));
        }
        let swallowable = self
            .res
            .items()
            .item_tags
            .get("minecraft:sulfur_cube_swallowable")
            .is_some_and(|t| t.contains(&*stack.item));
        if swallowable {
            let front = offset(pos, facing);
            if self.sulfur_cube_swallow(front, &stack) {
                return Ok((shrink(stack), CLICK));
            }
            return Ok((self.default_execute(pos, facing, stack), CLICK));
        }
        // DispenserBlock.getDefaultDispenseMethod: an egg without ENTITY_DATA drops as an item.
        if stack.item.ends_with("_spawn_egg")
            && stack
                .data()
                .is_none_or(|data| data.other("!minecraft:entity_data").is_none())
        {
            return self.dispense_spawn_egg(pos, facing, stack);
        }
        Ok((self.default_execute(pos, facing, stack), CLICK))
    }

    // ---- entities ----

    /// Entities whose box overlaps the block at `p`, in `getEntitiesOfClass`
    /// order ([`section_order`]): players are entities of the level like any
    /// other, so they sort with the mobs and stands.
    fn targets_at(&self, p: Pos) -> Vec<Target> {
        let block = block_box(p);
        let key = |at: Vec3, id: u32| (section_order(at), id);
        // The ECS level's entities, found by its spatial index near the block, then the strict overlap.
        let mut found: Vec<(_, Target)> = self
            .ecs_in_box(block)
            .into_iter()
            .filter(|(_, _, bounds)| overlaps(*bounds, block))
            .map(|(at, id, _)| (key(at, id), Target::Ecs(id)))
            .collect();
        found.extend(
            self.armor_stand_list()
                .iter()
                .filter(|s| overlaps(s.bounds(), block))
                .map(|s| (key(s.position, s.id), Target::Stand(s.id))),
        );
        // `ServerLevel.getEntities`: every player placed in this level, the
        // integrated owner included (a server has no player of its own), its
        // client loaded or not (it joins the level at placeNewPlayer/respawn).
        if let Some(host) = &self.lan {
            found.extend(
                (host.hub.guests.iter())
                    .filter(|g| host.hub.here(g) && !g.dead && !g.spectator())
                    .filter(|g| overlaps(g.bounds(), block))
                    .map(|g| {
                        (
                            key(
                                Vec3::new(g.position[0], g.position[1], g.position[2]),
                                g.entity_id as u32,
                            ),
                            Target::Guest(g.conn),
                        )
                    }),
            );
        }
        if let Some(v) = self.player_view()
            && v.alive
            && !self.spectator()
            && overlaps(v.bounds, block)
        {
            // The local player joins before the chunks around it load their
            // entities (prepareLevels loads only saved tickets): the lowest id,
            // 0 as `own_entity_id` has it.
            found.push((key(v.position, 0), Target::Player));
        }
        found.sort_by_key(|(k, _)| *k);
        found.into_iter().map(|(_, t)| t).collect()
    }
    /// `EntityType.getYOffset(level, pos, false, box)`: [`Self::drop_offset_from`] without
    /// `movedUp`.
    pub(super) fn drop_offset(&self, width: f64, height: f64, p: Pos) -> f64 {
        self.drop_offset_from(width, height, p, false)
    }
    /// `EntityType.getYOffset(level, pos, movedUp, box)`: the entity box set on
    /// top of the block (`setPos(x + 0.5, y + 1, z + 0.5)`) falls one block
    /// against the collision shapes overlapping that block, or, `moved_up`, two
    /// blocks against those of the block and the one under it.
    pub(super) fn drop_offset_from(&self, width: f64, height: f64, p: Pos, moved_up: bool) -> f64 {
        use horizon_world::{
            block_collision_scan::BlockCollisionScan, collision_context::CollisionContext,
        };
        let h = width / 2.0;
        let (x, z) = (f64::from(p.0) + 0.5, f64::from(p.2) + 0.5);
        let y = f64::from(p.1) + 1.0;
        let entity = Aabb::new(
            Vec3::new(x - h, y, z - h),
            Vec3::new(x + h, y + height, z + h),
        );
        let cell = Aabb::new(
            Vec3::new(
                f64::from(p.0),
                f64::from(p.1) - if moved_up { 1.0 } else { 0.0 },
                f64::from(p.2),
            ),
            Vec3::new(
                f64::from(p.0) + 1.0,
                f64::from(p.1) + 1.0,
                f64::from(p.2) + 1.0,
            ),
        );
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        let mut amount = if moved_up { -2.0 } else { -1.0 };
        // Level.getCollisions(null, aabb): CollisionContext.empty().
        if let Ok(scan) = BlockCollisionScan::new(
            &mut scene,
            CollisionContext::empty(false),
            cell,
            false,
            1_000_000,
        ) {
            for hit in scan.flatten() {
                amount = hit
                    .shape
                    .collide(horizon_core::geometry::Axis::Y, entity, amount);
            }
        }
        1.0 + amount
    }
    /// `EntityType.spawn(.., pos, reason, tryMoveDown, false)` for a mob.
    pub(super) fn spawn_mob_at(&mut self, kind: &str, p: Pos, try_move_down: bool) -> Option<u32> {
        self.spawn_mob_finalized_at(
            kind,
            p,
            try_move_down,
            myriade_engine::mobs::spawn_rules::Reason::Dispenser,
        )
    }
    /// `MobBucketItem.spawn`: `EntityType.create(.., EntitySpawnReason.BUCKET, true, false)`.
    /// `Axolotl.finalizeSpawn` returns first for that reason, without a draw.
    pub(super) fn spawn_bucket_mob(&mut self, kind: &str, p: Pos) -> Option<u32> {
        self.spawn_mob_finalized_at(
            kind,
            p,
            true,
            myriade_engine::mobs::spawn_rules::Reason::Bucket,
        )
    }
    fn spawn_mob_finalized_at(
        &mut self,
        kind: &str,
        p: Pos,
        try_move_down: bool,
        reason: myriade_engine::mobs::spawn_rules::Reason,
    ) -> Option<u32> {
        let name = if kind.contains(':') {
            kind.to_owned()
        } else {
            format!("minecraft:{kind}")
        };
        // Ender Dragons are simulated separately from `Mob` and therefore do
        // not have a mob-type registry entry for the generic dispenser path.
        if name == "minecraft:ender_dragon" {
            // `EntityType.getYOffset` uses the created entity's full 16 by 8
            // bounding box when the dispenser asks to move it down.
            let y_off = if try_move_down {
                self.drop_offset(16.0, 8.0, p)
            } else {
                0.0
            };
            let at = [
                f64::from(p.0) + 0.5,
                f64::from(p.1) + y_off,
                f64::from(p.2) + 0.5,
            ];
            return self.spawn_dragon_spawned(at, super::creatures::SpawnSite::Finalizing(reason));
        }
        let t = self.res.mob_types().get(&name)?.clone();
        let y_off = if try_move_down {
            self.drop_offset(t.width, t.height, p)
        } else {
            0.0
        };
        let at = [
            f64::from(p.0) + 0.5,
            f64::from(p.1) + y_off,
            f64::from(p.2) + 0.5,
        ];
        // Every entity but the players is an ECS entity: the level spawns it (its yaw and the bucket's data are not drawn here).
        self.ecs_spawn(&name, at, None)
    }
    /// `Player.setItemSlot` of a dispensed piece for the player in place:
    /// `onEquipItem` plays the sound and the event; the modifiers follow.
    fn put_on_player(&mut self, slot: Slot, one: &ItemStack) {
        let name = match slot.armor_index() {
            Some(i) => {
                self.armor[i] = Some(one.clone());
                ["head", "chest", "legs", "feet"][i]
            }
            None => {
                self.offhand = Some(one.clone());
                "offhand"
            }
        };
        self.player_equipped(name, Some(one));
        self.apply_equipment();
    }
    /// `EquipmentDispenseItemBehavior.dispenseEquipment`: the first living
    /// entity in front that `canEquipWithDispenser` gets one of the item.
    fn dispense_equipment(&mut self, pos: Pos, facing: Direction, stack: &mut ItemStack) -> bool {
        // The stack's own `equippable`: its slot and allowed entities, and `dispensable`.
        let Some(equippable) = equippable(self.items(), stack) else {
            return false;
        };
        let default = (self.items().get(&stack.item)).is_some_and(|info| info.equip_dispensable);
        let dispensable = myriade_engine::item_sync::dispensable(stack, default);
        let Some(slot) = Slot::from_name(equippable.slot).filter(|_| dispensable) else {
            return false;
        };
        // Equippable.equipSound defaults to the generic armour sound.
        let sound = (self.items().get(&stack.item))
            .and_then(|info| info.equip_sound.clone())
            .or_else(|| Some("minecraft:item.armor.equip_generic".into()));
        let front = offset(pos, facing);
        let one = ItemStack {
            count: 1,
            ..stack.clone()
        };
        // `Equippable.canBeEquippedBy`, then the item's parts (`canEquip`) with the stack converted
        // once, for the entity in front.
        let targets = self.targets_at(front);
        let observed = false;
        let mut source_slot = observed.then(|| Some(stack.clone()));
        let mut source = source_slot.as_mut().map(|slot| {
            myriade_engine::mobs::equip::EquipDecision::new(stack as *const _ as usize as u64, slot)
        });
        let kinds = if observed {
            targets
                .iter()
                .map(|target| match target {
                    Target::Ecs(id) => self.ecs_mob(*id).map_or("", |m| m.kind),
                    Target::Stand(_) => "minecraft:armor_stand",
                    _ => "minecraft:player",
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let entities = if observed {
            targets
                .iter()
                .filter_map(|target| match target {
                    Target::Player => Some(self.own_entity_id() as u32),
                    Target::Guest(conn) => self
                        .lan
                        .as_ref()?
                        .hub
                        .guest(*conn)
                        .map(|guest| guest.entity_id as u32),
                    Target::Stand(i) => Some(*i),
                    Target::Ecs(id) => Some(*id),
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let input = source.as_deref().map_or(&*stack, |source| source);
        let (tags, prepared) = (
            &self.stations().enchantments.entity_tags,
            self.equip_stack(input, &kinds, &entities),
        );
        let stamp = source.as_ref().and_then(|source| source.stamp());
        let allows = |entity: u32, id: &str| {
            let data = equippable.allows(id, tags);
            self.can_equip(&prepared, equippable.slot, entity, id, data)
        };
        // LivingEntity.canEquipWithDispenser: alive, the slot usable, the
        // item's allowed entities, the slot empty and the dispenser rule.
        let target = targets.into_iter().find(|t| match *t {
            // Player.canUseSlot: every slot; the armour and the off hand are
            // the ones an equippable item can reach.
            Target::Player => {
                allows(self.own_entity_id() as u32, "minecraft:player")
                    && match slot {
                        Slot::OffHand => self.offhand.is_none(),
                        _ => slot.armor_index().is_some_and(|i| self.armor[i].is_none()),
                    }
            }
            Target::Guest(conn) => self
                .lan
                .as_ref()
                .and_then(|h| h.hub.guest(conn))
                .is_some_and(|g| {
                    allows(g.entity_id as u32, "minecraft:player")
                        && match slot {
                            Slot::OffHand => g.offhand.is_none(),
                            _ => slot.armor_index().is_some_and(|i| g.armor[i].is_none()),
                        }
                }),
            // ArmorStand.canUseSlot: not a locked slot, and a hand needs arms.
            Target::Stand(i) => slot.stand_index().is_some_and(|s| {
                let Some(stand) = self.armor_stand_saved(i) else { return false };
                allows(stand.id, "minecraft:armor_stand")
                    && !stand.is_disabled(s)
                    && stand.equipment[s].is_none()
            }),
            // The same rules read from the components: `Health`, `GrowAt`, `Owner`, `Equipment`.
            Target::Ecs(id) => self.ecs_mob(id).is_some_and(|m| {
                m.alive
                    && m.species.is_some_and(|s| {
                        can_use_slot(s, m.alive && !m.baby, m.tame, slot)
                            && allows(id, m.kind)
                            && self.ecs_item(id, slot.goal()).is_none()
                            && self.ecs.can_pick_up_loot(id).is_some_and(|loot| can_dispenser_equip(s, loot, m.tame, slot))
                    })
            }),
        });
        let Some(target) = target else {
            return false;
        };
        let entity = match target {
            Target::Player => Some(self.own_entity_id() as u32),
            Target::Guest(conn) => self
                .lan
                .as_ref()
                .and_then(|lan| lan.hub.guest(conn))
                .map(|guest| guest.entity_id as u32),
            Target::Stand(i) => Some(i),
            Target::Ecs(id) => Some(id),
        };
        if !entity.is_some_and(|id| self.equip_can_apply(&prepared, id)) {
            return false;
        }
        // An ECS entity is checked before the stack goes down: the stack only loses its piece once the slot can be set.
        if let Target::Ecs(id) = target {
            if self.ecs_mob(id).is_none() {
                return false;
            }
        }
        if let Some(source) = source.as_mut() {
            if !stamp.is_some_and(|stamp| source.matches(stamp)) {
                return false;
            }
            *source.count_mut().expect("live dispenser source") -= 1;
            stack.count = source.count;
        } else {
            stack.count -= 1;
        }
        // setItemSlot: onEquipItem plays the equip sound and the equip event.
        let (at, sound) = match target {
            Target::Guest(conn) => {
                // The guest's own turn: their state in place, so the slot, the
                // sound, the game event and the equipment sync run as for the host.
                let done = self
                    .for_player(super::commands::Target::Guest(conn), |g| {
                        g.put_on_player(slot, &one)
                    })
                    .is_some();
                if !done {
                    stack.count += 1;
                    return false;
                }
                return true;
            }
            Target::Player => {
                self.put_on_player(slot, &one);
                return true;
            }
            Target::Stand(i) => {
                let position = self.armor_stand_saved(i).map(|a| a.position);
                if let Some(s) = slot.stand_index() {
                    self.armor_stand_edit(i, |a| a.equipment[s] = Some(one));
                }
                (position, sound)
            }
            Target::Ecs(id) => {
                // The slot and the persistence go by the bridge; the sound and the game event stay here.
                let Some(m) = self.ecs_mob(id) else { return false };
                self.ecs_set_slot(id, slot.goal(), &one);
                let sound = m.species.and_then(|s| equip_sound_of(s, false, slot, sound));
                (Some(m.at), sound)
            }
        };
        if let Some(at) = at {
            if let Some(sound) = sound {
                self.audio.play(&sound, Some([at.x, at.y, at.z]), 1.0, 1.0);
            }
            let who = match target {
                Target::Ecs(id) => Some(myriade_engine::mobs::Who::Mob(id)),
                Target::Player | Target::Guest(_) | Target::Stand(_) => None,
            };
            let ctx = self.who_context(who, None);
            self.post_game_event("minecraft:equip", [at.x, at.y, at.z], ctx);
        }
        true
    }
    /// After `EntityType.spawn` / `MobBucketItem.spawn`: `mob.playAmbientSound()`.
    pub(super) fn mob_ambient(&mut self, id: u32) -> Result<()> {
        if self.dragon_spawn_ambient(id) {
            return Ok(());
        }
        self.ecs_ambient(id);
        Ok(())
    }
    /// A mob a bucket emptied (`MobBucketItem.spawn`): the bucket's components
    /// and entity data (`loadFromBucketTag`), then `setFromBucket(true)`, which
    /// `requiresCustomPersistence` reads, then it calls out. Only the bucket's
    /// data can make it persistent, not the spawn: `spawn_mob_at` creates a
    /// persistent mob. A tadpole's setter does nothing: its `fromBucket` is
    /// always true.
    pub(super) fn mob_placed(&mut self, id: u32, stack: &ItemStack) -> Result<()> {
        // The bucket's components, its entity data (`updateCustomEntityTag`) and its bucket data (`loadFromBucketTag`), then the flag.
        if let Some(kind) = self.ecs.kind(id) {
            let op = self.acting_player_is_op();
            let data = match myriade_engine::block_entity_item::spawn_egg_entity_data(stack, &kind, op) {
                Some(horizon_core::nbt::Tag::Compound(c)) => Some(c),
                _ => None,
            };
            let bucket = match stack.data().and_then(|c| c.other("minecraft:bucket_entity_data")) {
                Some(horizon_core::nbt::Tag::Compound(c)) => Some(c),
                _ => None,
            };
            self.ecs.apply_egg_components(id, stack, data, bucket);
        }
        self.ecs.set_from_bucket(id);
        self.mob_ambient(id)
    }
    /// `SpawnEggItemBehavior.execute`: spawn the type selected by the stack's entity data.
    fn dispense_spawn_egg(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        let (kind, _) = myriade_engine::block_entity_item::spawn_egg_on_spawner(&stack, false);
        let front = offset(pos, facing);
        let Some(id) = self.spawn_mob_at(&kind, front, facing != Direction::Up) else {
            return Ok((Some(stack), CLICK));
        };
        // EntityType.create: `finalizeSpawn` first, the egg's components after.
        // SpawnEggItemBehavior's `EntityType.spawn` never calls `setPersistenceRequired`, as a spawn egg used by hand doesn't;
        // only the elder guardian's and copper golem's constructors do.
        if let Some(s) = self.ecs_mob(id).and_then(|m| m.species) {
            if matches!(s, Species::ElderGuardian | Species::CopperGolem) {
                self.ecs_persist(id);
            }
            let op = self.acting_player_is_op();
            let data = match myriade_engine::block_entity_item::spawn_egg_entity_data(&stack, &kind, op) {
                Some(horizon_core::nbt::Tag::Compound(c)) => Some(c),
                _ => None,
            };
            self.ecs.apply_egg_components(id, &stack, data, None);
        }
        // EntityType.updateCustomEntityTag: a configured egg
        // (`/give @s pig_spawn_egg[minecraft:entity_data={…}]`) spawns the mob
        // it describes from a dispenser too, not only from a player's hand.
        self.apply_spawn_egg_entity_data(id, &stack, true);
        self.mob_ambient(id)?;
        // SpawnEggItemBehavior: the entity was placed by the dispenser.
        self.post_block_game_event("minecraft:entity_place", [pos.0, pos.1, pos.2], None, None);
        Ok((shrink(stack), CLICK))
    }

    // ---- ProjectileDispenseBehavior ----

    fn dispense_projectile(&mut self, pos: Pos, facing: Direction, stack: ItemStack) -> Dispensed {
        let item = stack.item.to_string();
        let s = step(facing);
        let dir = [f64::from(s[0]), f64::from(s[1]), f64::from(s[2])];
        // ProjectileItem.DispenseConfig per item: position, uncertainty, power, event.
        let (at, uncertainty, power, event) = match item.as_str() {
            "minecraft:firework_rocket" => (
                Self::dispense_position(pos, facing, 0.500_009_999_999_747_4, [0.0; 3]),
                1.0,
                0.5,
                1004,
            ),
            "minecraft:fire_charge" => (
                Self::dispense_position(pos, facing, 1.0, [0.0; 3]),
                6.666_666_5,
                1.0,
                1018,
            ),
            "minecraft:wind_charge" => (
                Self::dispense_position(pos, facing, 1.0, [0.0; 3]),
                6.666_666_5,
                1.0,
                1051,
            ),
            "minecraft:experience_bottle"
            | "minecraft:splash_potion"
            | "minecraft:lingering_potion" => (
                Self::dispense_position(pos, facing, 0.7, [0.0, 0.1, 0.0]),
                3.0,
                1.375,
                LAUNCH,
            ),
            _ => (
                Self::dispense_position(pos, facing, 0.7, [0.0, 0.1, 0.0]),
                6.0,
                1.1,
                LAUNCH,
            ),
        };
        self.shoot_projectile_item(at, dir, &stack, power, uncertainty);
        (shrink(stack), event)
    }

    /// `ProjectileItem.asProjectile` and `Projectile.spawnProjectileUsingShoot`:
    /// one of `stack` fired from `at` along `dir`.
    pub(super) fn shoot_projectile_item(
        &mut self,
        at: Vec3,
        dir: [f64; 3],
        stack: &ItemStack,
        power: f32,
        uncertainty: f32,
    ) {
        let item = stack.item.to_string();
        let one = ItemStack {
            count: 1,
            ..stack.clone()
        };
        // A rocket makes its own random in `launch_angled_rocket`: the constructor's draws
        // and then the shot's spread come from that one.
        if item == "minecraft:firework_rocket" {
            self.launch_angled_rocket(at, &one, None, |own| {
                let mut shot = Projectile::new(ProjectileKind::Snowball, at, None);
                shot.shoot(own, dir, power, uncertainty);
                shot.velocity
            });
            return;
        }
        // Projectile.shoot spreads the shot with the projectile's own random.
        match item.as_str() {
            "minecraft:experience_bottle"
            | "minecraft:splash_potion"
            | "minecraft:lingering_potion" => {
                let mut shot = super::potions::thrown_projectile(one, at, false);
                shot.shoot_own(dir, power, uncertainty);
                self.ecs_launch_item(
                    shot.entity_type(),
                    [shot.position.x, shot.position.y, shot.position.z],
                    [shot.velocity.x, shot.velocity.y, shot.velocity.z],
                    self.ecs_owner(shot.owner),
                    shot.item.clone(),
                );
            }
            _ => {
                let kind = match item.as_str() {
                    "minecraft:arrow" => ProjectileKind::Arrow {
                        base_damage: 2.0,
                        critical: false,
                        effect: None,
                    },
                    "minecraft:spectral_arrow" => ProjectileKind::Arrow {
                        base_damage: 2.0,
                        critical: false,
                        effect: Some(("minecraft:glowing", 200)),
                    },
                    "minecraft:tipped_arrow" => ProjectileKind::Arrow {
                        base_damage: 2.0,
                        critical: false,
                        effect: tipped_effect(stack),
                    },
                    "minecraft:fire_charge" | "minecraft:wind_charge" => {
                        // asProjectile aims with the level's random first; the
                        // shot along the facing then replaces that motion.
                        let r = &mut self.world.random;
                        let _ = (
                            r.triangle(dir[0], 0.114_85),
                            r.triangle(dir[1], 0.114_85),
                            r.triangle(dir[2], 0.114_85),
                        );
                        if item == "minecraft:wind_charge" {
                            ProjectileKind::WindCharge
                        } else {
                            ProjectileKind::SmallFireball
                        }
                    }
                    "minecraft:egg" | "minecraft:blue_egg" | "minecraft:brown_egg" => {
                        ProjectileKind::Egg
                    }
                    _ => ProjectileKind::Snowball,
                };
                let mut shot = Projectile::new(kind, at, None);
                shot.set_egg_item(&one);
                // ArrowItem/SpectralArrowItem.asProjectile: dispensed arrows
                // may be picked up.
                if let Some(a) = shot.arrow.as_mut() {
                    a.set_item(&one);
                    a.pickup = myriade_engine::arrows::Pickup::Allowed;
                }
                shot.shoot_own(dir, power, uncertainty);
                self.spawn_projectile(shot, &one);
            }
        }
    }

    // ---- entities placed by items ----

    fn dispense_armor_stand(&mut self, pos: Pos, facing: Direction, stack: ItemStack) -> Dispensed {
        let front = offset(pos, facing);
        let foot = Vec3::new(
            f64::from(front.0) + 0.5,
            f64::from(front.1),
            f64::from(front.2) + 0.5,
        );
        // EntityType.create rolls a yaw from the level random before the stack config turns it.
        let _ = self.world.random.next_float();
        let (_, uuid) = myriade_engine::mobs::living_random();
        let stand = myriade_engine::entities::ArmorStand::new(0, foot, y_rot(facing));
        let horizon_core::nbt::Tag::Compound(mut saved) = myriade_engine::entity_nbt::armor_stand_to_nbt(&stand, [0; 4], None) else {
            return (Some(stack), FAIL);
        };
        saved.remove(&"UUID".into());
        let Some(id) = self.ecs_load(&saved) else {
            return (Some(stack), FAIL);
        };
        self.persist.uuids.insert(id, uuid);
        self.world.modified.insert((front.0 >> 4, front.2 >> 4));
        // EntityType.appendDefaultStackConfig: the dispenser's yaw first, then
        // the item's createDefaultStackConfig with no user.
        self.apply_item_stack_config(
            super::commands::Target::Stand(id),
            "minecraft:armor_stand",
            &stack,
            false,
        );
        (shrink(stack), CLICK)
    }
    /// A tame donkey, mule or llama in front without a chest gets it.
    fn dispense_chest(&mut self, pos: Pos, facing: Direction, stack: ItemStack) -> Dispensed {
        let front = offset(pos, facing);
        for target in self.targets_at(front) {
            let Target::Ecs(id) = target else { continue };
            // `ChestedHorse` is the saved flag; the level's entity keeps it, the host only writes it.
            let Some(m) = self.ecs_mob(id) else { continue };
            if m.species.is_some_and(|s| {
                matches!(s, Species::Donkey | Species::Mule | Species::Llama | Species::TraderLlama)
            }) && m.alive
                && m.tame
                && self.ecs_flag(id, "ChestedHorse") == Some(false)
            {
                self.ecs_put_flag(id, "ChestedHorse", true);
                set_success("chest", true);
                return (shrink(stack), CLICK);
            }
        }
        (
            self.default_execute(pos, facing, stack),
            optional_event("chest"),
        )
    }
    /// `BoatDispenseItemBehavior`.
    fn dispense_boat(&mut self, pos: Pos, facing: Direction, stack: ItemStack) -> Dispensed {
        let Some((wood, chest)) =
            super::boats::any_boat_wood(&stack.item).map(|(w, c)| (w.to_owned(), c))
        else {
            return (self.dispense_default_in_level(pos, facing, stack), CLICK);
        };
        let front = offset(pos, facing);
        let s = step(facing);
        let c = Self::center(pos);
        // Boat width 1.375.
        let outside = 0.5625 + 1.375 / 2.0;
        let (x, y, z) = (
            c[0] + f64::from(s[0]) * outside,
            c[1] + f64::from(s[1]) * f64::from(1.125f32),
            c[2] + f64::from(s[2]) * outside,
        );
        let below = (front.0, front.1 - 1, front.2);
        let y_offset = if self.water_at(front) {
            1.0
        } else if self.res.facts().is_air(self.block_at(front)) && self.water_at(below) {
            0.0
        } else {
            return (self.dispense_default_in_level(pos, facing, stack), CLICK);
        };
        let at = [x, y + y_offset, z];
        let mut template = super::boats::Boat::new(0, wood.as_str().into(), Vec3::new(at[0], at[1], at[2]), 0.0);
        if chest {
            template = template.with_chest();
        }
        let kind = template.entity_type();
        let Some(id) = self.spawn_boat_ecs(&wood, chest, at, y_rot(facing)) else {
            return (self.dispense_default_in_level(pos, facing, stack), CLICK);
        };
        self.world.modified.insert((front.0 >> 4, front.2 >> 4));
        // BoatDispenseItemBehavior: createDefaultStackConfig with no user.
        self.apply_item_stack_config(super::commands::Target::Boat(id), &kind, &stack, false);
        (shrink(stack), CLICK)
    }
    /// `MinecartDispenseItemBehavior`.
    fn dispense_minecart(&mut self, pos: Pos, facing: Direction, stack: ItemStack) -> Dispensed {
        let Some(kind) = super::minecarts::CartKind::from_id(&stack.item) else {
            return (self.dispense_default_in_level(pos, facing, stack), CLICK);
        };
        let s = step(facing);
        let c = Self::center(pos);
        let (x, base_y, z) = (
            c[0] + f64::from(s[0]) * 1.125,
            c[1].floor() + f64::from(s[1]),
            c[2] + f64::from(s[2]) * 1.125,
        );
        let front = offset(pos, facing);
        let y_offset = if let Some(rail) = self.rail(None, front.0, front.1, front.2) {
            if rail.shape.is_slope() { 0.6 } else { 0.1 }
        } else {
            if !self.res.facts().is_air(self.block_at(front)) {
                return (self.dispense_default_in_level(pos, facing, stack), CLICK);
            }
            let below = (front.0, front.1 - 1, front.2);
            let Some(rail) = self.rail(None, below.0, below.1, below.2) else {
                return (self.dispense_default_in_level(pos, facing, stack), CLICK);
            };
            if facing != Direction::Down && rail.shape.is_slope() {
                -0.4
            } else {
                -0.9
            }
        };
        let at = [x, base_y + y_offset, z];
        let next = self.next_entity + 1;
        self.world.modified.insert((front.0 >> 4, front.2 >> 4));
        let Some(id) = self.ecs_with(|ecs, mw| ecs.spawn_cart_id(mw, kind.id(), at, 0.0, &mut || next)) else {
            return (self.dispense_default_in_level(pos, facing, stack), CLICK);
        };
        self.next_entity = next;
        // AbstractMinecart.createMinecart: createDefaultStackConfig with no user.
        self.apply_item_stack_config(super::commands::Target::Cart(id), kind.id(), &stack, false);
        (shrink(stack), CLICK)
    }

    // ---- buckets ----

    /// The empty bucket: `BucketPickup.pickupBlock`.
    fn dispense_empty_bucket(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        let target = Self::pos_of(offset(pos, facing));
        let picked =
            self.with_level(|b, level, ticks| b.bucket_pickup(level, ticks, target, false));
        self.process_level_events()?;
        match picked {
            Some(filled) => {
                self.post_block_game_event(
                    "minecraft:fluid_pickup",
                    [target.x, target.y, target.z],
                    None,
                    None,
                );
                let filled = ItemStack::new(filled, 1);
                Ok((
                    self.consume_with_remainder(pos, facing, stack, filled),
                    CLICK,
                ))
            }
            None => Ok((self.default_execute(pos, facing, stack), CLICK)),
        }
    }

    // ---- fire, bone meal and TNT ----

    fn dispense_flint_and_steel(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        let target = offset(pos, facing);
        let target_pos = Self::pos_of(target);
        let state = self.block_at(target);
        let name = self.dispenser_block_name(target);
        let mut success = true;
        // tryIgniteExplosiveEntities: the first sulfur cube that can explode is primed, by the bridge: `prime(false)`, the sound and the game event here.
        let ecs_cube = self.targets_at(target).into_iter().find_map(|t| match t {
            Target::Ecs(id) if self.ecs_mob(id).is_some_and(|m| m.species == Some(Species::SulfurCube) && m.alive) => Some(id),
            _ => None,
        });
        if let Some(id) = ecs_cube {
            if self.session.rules.bool_at(myriade_engine::gamerules::vanilla::TNT_EXPLODES) && self.ecs.prime(id, None) {
                let at = self.ecs_mob(id).map_or([0.0; 3], |m| [m.at.x, m.at.y, m.at.z]);
                self.audio.play("entity.tnt.primed", Some(at), 1.0, 1.0);
                let ctx = self.who_context(Some(myriade_engine::mobs::Who::Mob(id)), None);
                self.post_game_event("minecraft:prime_fuse", at, ctx);
            }
            return Ok((self.hurt_and_break(stack, 1), CLICK));
        }
        if self
            .with_level(|b, level, ticks| b.fire_can_be_placed_at(level, ticks, target_pos, facing))
        {
            let fire =
                self.with_level(|b, level, ticks| b.fire_state_for(level, ticks, target_pos));
            self.level_set([target.0, target.1, target.2], fire)?;
            self.post_block_game_event(
                "minecraft:block_place",
                [target.0, target.1, target.2],
                None,
                None,
            );
        } else if self.can_light(state, &name) {
            if let Some(lit) = self.with_props(state, &[("lit", "true")]) {
                self.level_set([target.0, target.1, target.2], lit)?;
            }
            self.post_block_game_event(
                "minecraft:block_change",
                [target.0, target.1, target.2],
                None,
                None,
            );
        } else if &*name == "minecraft:tnt" {
            if self
                .session
                .rules
                .bool_at(myriade_engine::gamerules::vanilla::TNT_EXPLODES)
                && self.tnt_prime_allowed(target_pos)
            {
                self.spawn_primed_tnt(target_pos, false, None, None);
                self.with_level(|b, level, ticks| b.remove_block(level, ticks, target_pos, false));
                self.process_level_events()?;
            } else {
                success = false;
            }
        }
        let event = if success { CLICK } else { FAIL };
        if success {
            return Ok((self.hurt_and_break(stack, 1), event));
        }
        Ok((Some(stack), event))
    }
    /// `CampfireBlock.canLight || CandleBlock.canLight || CandleCakeBlock.canLight`.
    pub(super) fn can_light(&self, state: BlockStateId, name: &str) -> bool {
        let lit = self.prop(state, "lit");
        let waterlogged = self.prop(state, "waterlogged");
        let dry_unlit = lit.as_deref() == Some("false") && waterlogged.as_deref() == Some("false");
        ((self.in_block_tag("minecraft:campfires", name)
            || self.in_block_tag("minecraft:candles", name))
            && dry_unlit)
            || (self.in_block_tag("minecraft:candle_cakes", name)
                && lit.as_deref() == Some("false"))
    }
    fn dispense_bone_meal(
        &mut self,
        pos: Pos,
        facing: Direction,
        mut stack: ItemStack,
    ) -> Result<Dispensed> {
        let target = offset(pos, facing);
        let target_pos = Self::pos_of(target);
        let ask = self.bonemeal(target_pos, false, &mut stack);
        let grown = self.with_level(|b, level, ticks| {
            ask.crop
                .unwrap_or_else(|| b.grow_crop(level, ticks, target_pos, ask.check))
                || b.grow_water_plant(level, ticks, target_pos, None)
        });
        if grown {
            self.level_events
                .push(horizon_world::level::LevelEvent::LevelEvent {
                    id: 1505,
                    pos: target_pos,
                    data: 15,
                    source: None,
                });
        }
        self.process_level_events()?;
        // A canceled `BonemealEvent` that succeeded uses none of the meal.
        let left = if grown && ask.crop != Some(true) {
            shrink(stack)
        } else {
            (stack.count > 0).then_some(stack)
        };
        Ok((left, if grown { CLICK } else { FAIL }))
    }
    fn dispense_tnt(&mut self, pos: Pos, facing: Direction, stack: ItemStack) -> Dispensed {
        if !self
            .session
            .rules
            .bool_at(myriade_engine::gamerules::vanilla::TNT_EXPLODES)
        {
            set_success("tnt", false);
            return (Some(stack), FAIL);
        }
        let front = offset(pos, facing);
        if self.sulfur_cube_swallow(front, &stack) {
            return (shrink(stack), optional_event("tnt"));
        }
        let target = Self::pos_of(front);
        if !self.tnt_prime_allowed(target) {
            set_success("tnt", false);
            return (Some(stack), FAIL);
        }
        set_success("tnt", true);
        // new PrimedTnt(level, x + 0.5, y, z + 0.5, null), its sound and ENTITY_PLACE.
        let rot = self.world.random.next_double() * f64::from(std::f32::consts::TAU);
        self.add_primed_tnt(target, rot, 80, None, None);
        self.audio.play(
            "minecraft:entity.tnt.primed",
            Some([
                f64::from(target.x) + 0.5,
                f64::from(target.y),
                f64::from(target.z) + 0.5,
            ]),
            1.0,
            1.0,
        );
        self.post_block_game_event(
            "minecraft:entity_place",
            [front.0, front.1, front.2],
            None,
            None,
        );
        (shrink(stack), CLICK)
    }

    // ---- blocks placed by dispensers ----

    fn dispense_wither_skull(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        let target = offset(pos, facing);
        let free = self.res.facts().is_air(self.block_at(target))
            && target.1 >= self.world.bottom_y() + 2
            && self.session.difficulty != 0
            && self.can_build_wither(target);
        if free {
            let rotation = match facing {
                Direction::Up | Direction::Down => 0,
                other => {
                    // RotationSegment.convertToSegment: the 2D data value * 4.
                    (match other {
                        Direction::South => 0,
                        Direction::West => 1,
                        Direction::North => 2,
                        _ => 3,
                    }) * 4
                }
            };
            let skull = self
                .pack
                .assets
                .registry
                .default_state("minecraft:wither_skeleton_skull")
                .and_then(|s| self.with_props(s, &[("rotation", &rotation.to_string())]));
            if let Some(skull) = skull {
                self.level_set([target.0, target.1, target.2], skull)?;
                if let Some(entity) = BlockEntity::for_block("minecraft:wither_skeleton_skull") {
                    self.world.block_entities.insert(target, entity);
                }
                self.post_block_game_event(
                    "minecraft:block_place",
                    [target.0, target.1, target.2],
                    None,
                    None,
                );
                self.check_wither_spawn([target.0, target.1, target.2])?;
                return Ok((shrink(stack), CLICK));
            }
        }
        let mut stack = stack;
        let equipped = self.dispense_equipment(pos, facing, &mut stack);
        Ok((
            Some(stack).filter(|s| s.count > 0),
            if equipped { CLICK } else { FAIL },
        ))
    }
    /// `WitherSkullBlock.getOrCreateWitherBase().find(level, pos)`: a soul
    /// sand T for the skull row, in any orientation.
    fn can_build_wither(&self, head: Pos) -> bool {
        self.find_wither_pattern(
            myriade_engine::block_pattern::WITHER_BASE,
            [head.0, head.1, head.2],
        )
        .is_some()
    }
    fn dispense_carved_pumpkin(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        let target = offset(pos, facing);
        if self.res.facts().is_air(self.block_at(target))
            && self.can_spawn_golem([target.0, target.1, target.2])
        {
            if let Some(pumpkin) = self
                .pack
                .assets
                .registry
                .default_state("minecraft:carved_pumpkin")
            {
                let head = [target.0, target.1, target.2];
                self.set_golem_head(head, pumpkin)?;
                self.post_block_game_event("minecraft:block_place", head, None, None);
            }
            return Ok((shrink(stack), CLICK));
        }
        let mut stack = stack;
        let equipped = self.dispense_equipment(pos, facing, &mut stack);
        Ok((
            Some(stack).filter(|s| s.count > 0),
            if equipped { CLICK } else { FAIL },
        ))
    }
    /// `ShulkerBoxDispenseBehavior`: place it like a `DirectionalPlaceContext`.
    fn dispense_shulker_box(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        let front = offset(pos, facing);
        let below_empty = self
            .res
            .facts()
            .is_air(self.block_at((front.0, front.1 - 1, front.2)));
        let clicked_face = if below_empty { facing } else { Direction::Up };
        let ctx = PlaceContext {
            clicked_pos: Self::pos_of(front),
            face: clicked_face,
            hit: Vec3::new(
                f64::from(front.0) + 0.5,
                f64::from(front.1),
                f64::from(front.2) + 0.5,
            ),
            inside: false,
            yaw: place_rotation(facing),
            pitch: 0.0,
            sneaking: false,
            item: stack.item.to_string(),
            can_use_game_master_blocks: false,
        };
        // DirectionalPlaceContext places at the front block or nowhere.
        if !self.with_level(|b, level, ticks| b.directional_can_place(level, ticks, &ctx)) {
            return Ok((Some(stack), FAIL));
        }
        if false
            && let Some(preview) =
                self.with_level(|b, level, ticks| b.placement_preview(level, ticks, &ctx))
        {
            let against = ctx.placed_against_pos(preview.pos);
            let against = self.world.block(against.x, against.y, against.z);
            let mut canceled = false;
            if canceled {
                return Ok((Some(stack), FAIL));
            }
        }
        let outcome = self.with_level(|b, level, ticks| b.place_block(level, ticks, &ctx));
        let PlaceOutcome::Placed { pos: placed, state } = outcome else {
            self.process_level_events()?;
            return Ok((Some(stack), FAIL));
        };
        let target = (placed.x, placed.y, placed.z);
        let state = self.apply_item_block_state([target.0, target.1, target.2], state, &stack)?;
        if !self.world.block_entities.stateful_entity(state.0)
            && let Some(entity) = self.new_block_entity_for_state(state)
        {
            self.world.block_entities.insert(target, entity);
        }
        if self.world.block_entities.get(&target).is_some() {
            self.apply_placed_item([target.0, target.1, target.2], &stack, false);
        }
        self.block_item_placed(&stack, None, placed, &ctx);
        self.process_level_events()?;
        Ok((shrink(stack), CLICK))
    }

    // ---- bottles, glowstone, shears, brush, honeycomb ----

    fn dispense_glass_bottle(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        let target = offset(pos, facing);
        let state = self.block_at(target);
        let name = self.dispenser_block_name(target);
        let honey = self
            .prop(state, "honey_level")
            .and_then(|v| v.parse::<i32>().ok());
        if self.in_block_tag("minecraft:beehives", &name)
            && let Some(level) = honey
            && level >= 5
        {
            // releaseBeesAndResetHoneyLevel.
            if let Some(reset) = self.with_props(state, &[("honey_level", "0")]) {
                self.level_set([target.0, target.1, target.2], reset)?;
            }
            self.dispenser_empty_hive(target, state);
            let bottle = ItemStack::new("minecraft:honey_bottle", 1);
            self.post_block_game_event("minecraft:fluid_pickup", [pos.0, pos.1, pos.2], None, None);
            return Ok((
                self.consume_with_remainder(pos, facing, stack, bottle),
                CLICK,
            ));
        }
        if self.water_at(target) {
            let water = ItemStack::new("minecraft:potion", 1).with_potion("minecraft:water");
            self.post_block_game_event("minecraft:fluid_pickup", [pos.0, pos.1, pos.2], None, None);
            return Ok((
                self.consume_with_remainder(pos, facing, stack, water),
                CLICK,
            ));
        }
        Ok((self.default_execute(pos, facing, stack), FAIL))
    }
    fn dispense_glowstone(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        let target = offset(pos, facing);
        let state = self.block_at(target);
        if &*self.dispenser_block_name(target) == "minecraft:respawn_anchor" {
            let charge = self
                .prop(state, "charges")
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(0);
            if charge != 4 {
                if let Some(next) =
                    self.with_props(state, &[("charges", &(charge + 1).to_string())])
                {
                    self.level_set([target.0, target.1, target.2], next)?;
                    // RespawnAnchorBlock.charge: Context.of(null, newState).
                    self.post_block_game_event(
                        "minecraft:block_change",
                        [target.0, target.1, target.2],
                        None,
                        Some(next),
                    );
                }
                self.audio.play(
                    "minecraft:block.respawn_anchor.charge",
                    Some(Self::center(target)),
                    1.0,
                    1.0,
                );
                return Ok((shrink(stack), CLICK));
            }
            return Ok((Some(stack), FAIL));
        }
        Ok((self.default_execute(pos, facing, stack), CLICK))
    }
    fn dispense_shears(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        let target = offset(pos, facing);
        let sheared = self.shear_beehive(target)? || self.shear_entity(target)?;
        if sheared {
            return Ok((self.hurt_and_break(stack, 1), CLICK));
        }
        Ok((Some(stack), FAIL))
    }
    /// The bees of a hive a dispenser harvested come out (`BEE_RELEASED`:
    /// not at night nor in the rain), through the front of its old state.
    fn dispenser_empty_hive(&mut self, target: Pos, old: BlockStateId) {
        let facing = self.prop(old, "facing");
        self.empty_hive(
            [target.0, target.1, target.2],
            myriade_engine::beehive::facing_step(facing.as_deref()),
            myriade_engine::beehive::Release::BeeReleased,
        );
    }
    fn shear_beehive(&mut self, target: Pos) -> Result<bool> {
        let state = self.block_at(target);
        let name = self.dispenser_block_name(target);
        let level = self
            .prop(state, "honey_level")
            .and_then(|v| v.parse::<i32>().ok());
        if !self.in_block_tag("minecraft:beehives", &name) || level.is_none_or(|l| l < 5) {
            return Ok(false);
        }
        self.audio.play(
            "minecraft:block.beehive.shear",
            Some(Self::center(target)),
            1.0,
            1.0,
        );
        // BeehiveBlock.dropHoneycomb (harvest/beehive), then
        // releaseBeesAndResetHoneyLevel and the shear event.
        let p = [target.0, target.1, target.2];
        for stack in self.sequence_loot(
            "minecraft:harvest/beehive",
            "minecraft:harvest/beehive",
            None,
        ) {
            self.pop_resource(p, stack);
        }
        if let Some(reset) = self.with_props(state, &[("honey_level", "0")]) {
            self.level_set(p, reset)?;
        }
        self.dispenser_empty_hive(target, state);
        self.post_block_game_event("minecraft:shear", p, None, None);
        Ok(true)
    }
    /// `LootTable.getRandomItems` of `table` in the context of `parent`: the
    /// draws come from `parent`'s server random sequence when it names one (a
    /// nested `loot_table` entry shares its parent's context), else the level's.
    pub(super) fn sequence_loot(
        &mut self,
        parent: &str,
        table: &str,
        held: Option<&ItemStack>,
    ) -> Vec<ItemStack> {
        match world_items!(self).loot_sequence(parent) {
            Some(sequence) => {
                let random = self.rand_sequences.get(&sequence, self.level.seed);
                world_items!(self).loot_table_from(table, false, held, random)
            }
            None => world_items!(self).loot_table(table, false, held, &mut self.world.random),
        }
    }
    /// `Entity.spawnAtLocation(level, stack, offset)`: an item at the entity's
    /// feet plus the offset, moving as `new ItemEntity` makes it.
    pub(super) fn spawn_at_location(
        &mut self,
        at: Vec3,
        y_offset: f64,
        stack: ItemStack,
    ) -> [f64; 3] {
        self.spawn_item_own(stack, [at.x, at.y + y_offset, at.z], 10)
    }
    /// `shear` on an ECS entity by the species' own definition (`behaviours::shear::shear_entity`: state, loot, sound; the copper golem's
    /// antenna too). None when it is not shorn now; the kind it returns is the one the native hook reads.
    fn shear_ecs(&mut self, id: u32, _shears: &ItemStack) -> Option<&'static str> {
        let m = self.ecs_mob(id)?;
        self.ecs.shear(id).then_some(m.kind)
    }
    /// `ShearsDispenseItemBehavior.tryShearEntity`: the first live, ready
    /// `Shearable` in the block is sheared (an entity part's `IShearable` answer comes first).
    pub(super) fn shear_entity(&mut self, target: Pos) -> Result<bool> {
        let shears = ItemStack::new("minecraft:shears", 1);
        for t in self.targets_at(target) {
            // Entity.shearOffAllLeashConnections comes first, for any entity.
            if self.shear_off_leashes(t) {
                return Ok(true);
            }
            if let Target::Ecs(id) = t {
                let Some(m) = self.ecs_mob(id) else { continue };
                let part = self.part_shearable_of(id, m.kind, None, &shears);
                if !m.alive || part.is_some_and(|(_, shearable)| !shearable) {
                    continue;
                }
                let kind = match part {
                    Some((part, _)) => {
                        self.part_shear_ecs(id, m.kind, part, None, &shears);
                        m.kind
                    }
                    None => {
                        let Some(kind) = self.shear_ecs(id, &shears) else { continue };
                        kind
                    }
                };
                self.post_block_game_event("minecraft:shear", [target.0, target.1, target.2], None, None);
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// `Entity.shearOffAllLeashConnections(null)`: the entity's own lead and
    /// every lead held by it (within a 32 block box around its centre) drop;
    /// a shear game event at the entity and a snip at its block follow.
    fn shear_off_leashes(&mut self, t: Target) -> bool {
        use myriade_engine::mobs::Who;
        let centre = match t {
            Target::Player => {
                let Some(v) = self.player_view() else {
                    return false;
                };
                v.bounds.center()
            }
            Target::Ecs(id) => return self.shear_ecs_leashes(id),
            Target::Guest(_) | Target::Stand(_) => return false,
        };
        let area = Aabb::new(centre, centre).inflate(Vec3::new(16.0, 16.0, 16.0));
        // The player's leashes on the ECS level, by the bridge's holder index.
        let id = self.own_entity_id() as u32;
        let drops = self.ecs_unleash(None, Some(horizon_ecs_proto::leash::Holder::Player(id)), area);
        if drops.is_empty() {
            return false;
        }
        for at in drops {
            self.spawn_at_location(at, 0.0, ItemStack::new("minecraft:lead", 1));
        }
        let at = self.player.as_ref().map_or(Vec3::ZERO, |p| p.position);
        let ctx = self.who_context(Some(Who::Player), None);
        self.post_game_event("minecraft:shear", [at.x, at.y, at.z], ctx);
        let block = [at.x.floor() + 0.5, at.y.floor() + 0.5, at.z.floor() + 0.5];
        self.audio
            .play("minecraft:item.shears.snip", Some(block), 1.0, 1.0);
        true
    }
    /// `shearOffAllLeashedEntities` for an ECS entity: its own leash and the ones it holds within 16 blocks drop.
    fn shear_ecs_leashes(&mut self, id: u32) -> bool {
        let Some(m) = self.ecs_mob(id) else { return false };
        let Some(e) = self.ecs.entity_of(id) else { return false };
        let area = Aabb::new(m.at, m.at).inflate(Vec3::new(16.0, 16.0, 16.0));
        let drops = self.ecs_unleash(Some(id), Some(horizon_ecs_proto::leash::Holder::Entity(e)), area);
        if drops.is_empty() {
            return false;
        }
        for at in drops {
            self.spawn_at_location(at, 0.0, ItemStack::new("minecraft:lead", 1));
        }
        let at = m.at;
        let ctx = self.who_context(Some(myriade_engine::mobs::Who::Mob(id)), None);
        self.post_game_event("minecraft:shear", [at.x, at.y, at.z], ctx);
        let block = [at.x.floor() + 0.5, at.y.floor() + 0.5, at.z.floor() + 0.5];
        self.audio.play("minecraft:item.shears.snip", Some(block), 1.0, 1.0);
        true
    }
    /// The brush behaviour: the first armadillo in front that `brushOffScute`s.
    fn dispense_brush(&mut self, pos: Pos, facing: Direction, stack: ItemStack) -> Dispensed {
        let target = offset(pos, facing);
        let armadillos: Vec<Target> = self
            .targets_at(target)
            .into_iter()
            .filter(|t| match *t {
                Target::Ecs(id) => self.ecs_mob(id).is_some_and(|m| m.species == Some(Species::Armadillo)),
                _ => false,
            })
            .collect();
        for t in armadillos {
            // The brush has no state here: the armadillo's position, id and age are all it reads; the ECS one is brushed by the bridge.
            let Target::Ecs(id) = t else { continue };
            let Some(m) = self.ecs_mob(id) else { continue };
            let (at, baby) = (m.at, m.baby);
            if baby {
                continue;
            }
            self.ecs.brush(id);
            let drops = self.sequence_loot(
                "minecraft:brush/armadillo",
                "minecraft:brush/armadillo",
                Some(&stack),
            );
            for d in drops {
                self.spawn_at_location(at, 0.0, d);
            }
            self.audio.play(
                "minecraft:entity.armadillo.brush",
                Some([at.x, at.y, at.z]),
                1.0,
                1.0,
            );
            let who = myriade_engine::mobs::Who::Mob(id);
            let ctx = self.who_context(Some(who), None);
            self.post_game_event("minecraft:entity_interact", [at.x, at.y, at.z], ctx);
            return (self.hurt_and_break(stack, 16), optional_event("brush"));
        }
        set_success("brush", false);
        (Some(stack), FAIL)
    }
    /// `SulfurCubeBlockDispenseItemBehavior.dispenseBlock`: the first sulfur
    /// cube in the block that `equipItem`s the stack swallows one.
    /// The swallow of an ECS cube: the body item goes in its `Body` slot, the one it held drops on top.
    fn sulfur_swallow_ecs(&mut self, id: u32, stack: &ItemStack) -> bool {
        let Some(m) = self.ecs_mob(id) else { return false };
        if m.baby || self.ecs_item(id, goals::Slot::Body).as_deref() == Some(&*stack.item) {
            return false;
        }
        // The swallowed block comes out on top (the passenger attachment).
        if let Some(old) = self.ecs_take(id, goals::Slot::Body) {
            self.spawn_at_location(m.at, m.height, old);
        }
        self.ecs_set_slot(id, goals::Slot::Body, &ItemStack { count: 1, ..stack.clone() });
        // setItemSlot: a body item without `equippable` posts UNEQUIP.
        let at = m.at;
        let ctx = self.who_context(Some(myriade_engine::mobs::Who::Mob(id)), None);
        self.post_game_event("minecraft:unequip", [at.x, at.y, at.z], ctx);
        self.audio.play(
            "minecraft:entity.sulfur_cube.absorb",
            Some([at.x, at.y, at.z]),
            1.0,
            1.0,
        );
        true
    }
    fn sulfur_cube_swallow(&mut self, target: Pos, stack: &ItemStack) -> bool {
        let cubes: Vec<Target> = self
            .targets_at(target)
            .into_iter()
            .filter(|t| match *t {
                Target::Ecs(id) => self.ecs_mob(id).is_some_and(|m| m.species == Some(Species::SulfurCube)),
                _ => false,
            })
            .collect();
        for t in cubes {
            let Target::Ecs(id) = t else { continue };
            if self.sulfur_swallow_ecs(id, stack) {
                return true;
            }
        }
        false
    }
    fn dispense_honeycomb(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        let target = offset(pos, facing);
        let state = self.block_at(target);
        // HoneycombItem.getWaxed, as the player's honeycomb waxes.
        let Some(waxed) = self.honeycomb_waxed(state) else {
            return Ok((
                self.default_execute(pos, facing, stack),
                optional_event("honeycomb"),
            ));
        };
        set_success("honeycomb", true);
        self.level_set([target.0, target.1, target.2], waxed)?;
        self.level_events
            .push(horizon_world::level::LevelEvent::LevelEvent {
                id: 3003,
                pos: Self::pos_of(target),
                data: 0,
                source: None,
            });
        self.process_level_events()?;
        self.audio.play(
            "minecraft:item.honeycomb.wax_on",
            Some(Self::center(target)),
            1.0,
            1.0,
        );
        Ok((shrink(stack), CLICK))
    }
    fn dispense_potion(
        &mut self,
        pos: Pos,
        facing: Direction,
        stack: ItemStack,
    ) -> Result<Dispensed> {
        let target = offset(pos, facing);
        let water = stack.potion() == Some("minecraft:water");
        let name = self.dispenser_block_name(target);
        if !water || !self.in_block_tag("minecraft:convertible_to_mud", &name) {
            // The inner default behaviour dispenses, then the potion's own
            // events play on top.
            return Ok((self.dispense_default_in_level(pos, facing, stack), CLICK));
        }
        // The server picks the splash positions with the level random.
        // sendParticles vanilla : envoi aux joueurs à rebrancher
        for _ in 0..5 {
            let r = &mut self.world.random;
            let _splash = [
                f64::from(pos.0) + r.next_double(),
                f64::from(pos.1 + 1),
                f64::from(pos.2) + r.next_double(),
            ];
        }
        self.audio.play(
            "minecraft:item.bottle.empty",
            Some(Self::center(pos)),
            1.0,
            1.0,
        );
        self.post_block_game_event("minecraft:fluid_place", [pos.0, pos.1, pos.2], None, None);
        if let Some(mud) = self.pack.assets.registry.default_state("minecraft:mud") {
            self.level_set([target.0, target.1, target.2], mud)?;
        }
        let bottle = ItemStack::new("minecraft:glass_bottle", 1);
        Ok((
            self.consume_with_remainder(pos, facing, stack, bottle),
            CLICK,
        ))
    }
}
/// A tipped arrow's lasting effect at an eighth of the potion's duration
/// (`Arrow.doPostHurtEffects`).
pub(super) fn tipped_effect(stack: &ItemStack) -> Option<(&'static str, i32)> {
    stack
        .potion()
        .and_then(myriade_engine::brewing::potion)
        .and_then(|def| {
            def.effects
                .iter()
                .find(|(id, _, _)| !myriade_engine::brewing::effect(id).is_some_and(|e| e.instant))
                .map(|(id, duration, _)| {
                    (
                        myriade_engine::brewing::namespaced(id),
                        (duration / 8).max(1),
                    )
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One scene per dispense, three blocks apart: a dispenser at (5, 64, 2 + 3 row) facing east,
    /// and in front a zombie that picks up loot (id 10 + row) and an armour stand (id 20 + row),
    /// the zombie first. Dispenses `stack` and answers whether the dispenser equipped it, the
    /// zombie's slot that took it, whether the stand took it, and where the two stand.
    fn dress(
        game: &mut Game<'_>,
        row: u32,
        stack: ItemStack,
    ) -> (bool, Option<Slot>, bool, [f64; 3]) {
        use horizon_core::geometry::Vec3;
        (game.world.chunks).insert((0, 0), Arc::new(myriade_engine::chunk::Chunk::empty(0, 0)));
        let z = 2 + 3 * row as i32;
        let at = Vec3::new(6.5, 64.0, f64::from(z) + 0.5);
        crate::game::ecs::test_support::place_test_mob(game, 10 + row, "minecraft:zombie", [at.x, at.y, at.z]);
        game.ecs_put_flag(10 + row, "CanPickUpLoot", true);
        crate::game::ecs::test_support::place_test_stand(game, 20 + row, &myriade_engine::entities::ArmorStand::new(20 + row, at, 0.0));
        let mut stack = stack;
        let done = game.dispense_equipment((5, 64, z), Direction::East, &mut stack);
        let worn = [Slot::Head, Slot::Chest, Slot::Legs, Slot::Feet]
            .into_iter()
            .find(|slot| game.ecs_item(10 + row, slot.goal()).is_some());
        let stand = game.armor_stand_saved(20 + row).unwrap();
        (
            done,
            worn,
            stand.equipment.iter().any(Option::is_some),
            [at.x, at.y, at.z],
        )
    }

    /// An iron helmet whose `equippable` is removed (`fields` None) or replaced by `fields`.
    fn helmet_with(fields: Option<Vec<(&str, horizon_core::nbt::Tag)>>) -> ItemStack {
        use horizon_core::nbt::Tag;
        let mut helmet = ItemStack::new("minecraft:iron_helmet", 1);
        let (id, value) = match fields {
            Some(fields) => ("minecraft:equippable", Tag::compound(fields)),
            None => ("!minecraft:equippable", Tag::Compound(Default::default())),
        };
        helmet.data_mut().set_other(id, Some(value));
        helmet
    }

    /// The dispenser reads the stack's own `equippable`, with no part installed: a removed
    /// component dispenses nothing onto anyone, an override for the feet dresses the zombie's
    /// feet, `allowed_entities` naming only armour stands skips the zombie for the stand, an
    /// explicitly empty list allows nobody, and `dispensable: false` keeps the dispenser out.
    #[test]
    #[ignore = "daily"]
    fn a_dispenser_reads_the_stacks_own_equippable() {
        use horizon_core::nbt::Tag;
        super::super::native_entity_refs::tests::with_game(|game| {
            let slot =|name: &str| ("slot", Tag::String(name.into()));
            let allowed = |list: Vec<Tag>| ("allowed_entities", Tag::List(list));
            let dressed = |game: &mut Game<'_>, row, stack| {
                let (done, worn, stand, _) = dress(game, row, stack);
                (done, worn, stand)
            };
            assert_eq!(dressed(game, 0, helmet_with(None)), (false, None, false));
            assert_eq!(
                dressed(game, 1, helmet_with(Some(vec![slot("feet")]))),
                (true, Some(Slot::Feet), false)
            );
            let stands = vec![Tag::String("armor_stand".into())];
            assert_eq!(
                dressed(
                    game,
                    2,
                    helmet_with(Some(vec![slot("head"), allowed(stands)]))
                ),
                (true, None, true)
            );
            assert_eq!(
                dressed(
                    game,
                    3,
                    helmet_with(Some(vec![slot("head"), allowed(Vec::new())]))
                ),
                (false, None, false)
            );
            let kept = ("dispensable", Tag::Byte(0));
            assert_eq!(
                dressed(game, 4, helmet_with(Some(vec![slot("head"), kept]))),
                (false, None, false)
            );
        });
    }

    #[test]
    #[ignore = "daily"]
    fn dispenser_stack_damage_preserves_break_remainder() {
        super::super::native_entity_refs::tests::with_game(|game| {
            let mut shears = ItemStack::new("minecraft:shears", 2);
            let max = game.items().get(&shears.item).unwrap().max_damage;
            shears.damage = max - 1;
            let remainder = game.hurt_and_break(shears, 1).unwrap();
            assert_eq!(remainder.count, 1);
            assert_eq!(remainder.damage, max);
        });
    }

    /// `IItemExtension.damageItem` on the wear of a tool a dispenser uses: a brush (16 points a
    /// brushing) whose part halves it loses 8, one whose part cancels it keeps its durability, and
    /// the shears, which have no part, lose the whole point.
    #[test]
    #[ignore = "daily"]
    fn a_damage_item_part_changes_the_wear_of_a_dispensed_tool() {
        use crate::game::damage_item_tests::{Rule, with_wear_part};
        with_wear_part("minecraft:brush", Rule::Halve, |game, wear| {
            let damage = |game: &mut Game<'_>, item: &str, points| {
                game.hurt_and_break(ItemStack::new(item, 1), points)
                    .unwrap()
                    .damage
            };
            assert_eq!(damage(game, "minecraft:brush", 16), 8);
            assert_eq!(wear.take_asked(), [16]);
            wear.set(Rule::Cancel);
            assert_eq!(damage(game, "minecraft:brush", 16), 0);
            assert_eq!(wear.take_asked(), [16]);
            assert_eq!(damage(game, "minecraft:shears", 1), 1);
            assert!(wear.take_asked().is_empty());
        });
    }

    #[test]
    #[ignore = "daily"]
    fn sulfur_cube_dispenser_replacement_drops_the_full_old_stack() {
        super::super::native_entity_refs::tests::with_game(|game| {
            crate::game::ecs::test_support::place_test_mob(game, 12, "minecraft:sulfur_cube", [0.5, 0.0, 0.5]);
            let mut old = ItemStack::new("minecraft:stone", 3);
            old.damage = 7;
            old.data_mut().custom_name = Some(horizon_core::nbt::Tag::String("kept".into()));
            game.ecs_set_slot(12, goals::Slot::Body, &old);
            let incoming = ItemStack::new("minecraft:dirt", 5);
            assert!(game.sulfur_cube_swallow((0, 0, 0), &incoming));
            assert!(game.ecs.item_views().iter().any(|drop| drop.stack == old));
            assert_eq!(game.ecs_item(12, goals::Slot::Body).as_deref(), Some("minecraft:dirt"));
        });
    }

    use myriade_engine::lan::{Joined, NetConfig, PlayerSender, guest::Guest, hub::HubConfig};
    use horizon_protocol::packets::configuration::ClientInformation;
    use std::sync::{Arc, atomic::AtomicI32, mpsc};

    /// A server with no player of its own, an open hub and one guest (conn 2)
    /// standing at (8.5, 200, 8.5) on a stone floor.
    fn server_with_guest(
        resources: &mut Option<std::sync::Arc<crate::game::Resources>>,
    ) -> Game<'_> {
        let mut game = super::super::game_tests::tests::server_game(resources);
        game.build_registries();
        game.ecs = game.test_ecs_level();
        game.set_allow_commands(true);
        game.block_ticks
            .add_empty_chunk(horizon_core::position::ChunkPos::new(0, 0))
            .unwrap();
        assert!(game.run_command("fill 0 199 0 15 199 15 stone"));
        game.lan_host(
            super::super::lan::LanSettings {
                port: 0,
                guest_commands: true,
                force_game_mode: false,
            },
            NetConfig {
                announce: false,
                ..NetConfig::loopback(0, "dispenser-equip-test".into(), 2)
            },
            HubConfig {
                default_game_mode: 0,
                guest_commands: true,
                force_game_mode: false,
                hardcore: false,
                view_distance: 2,
                simulation_distance: 2,
                dimension: "minecraft:overworld".into(),
                allow_flight: false,
                seed: 0,
                max_players: 2,
                levels: Default::default(),
                level_seeds: Default::default(),
            },
        )
        .unwrap();
        let (tx, _rx) = mpsc::channel();
        let mut guest = Guest::new(
            Joined {
                conn: 2,
                uuid: 208,
                name: "Wearer".into(),
                properties: Vec::new(),
                info: ClientInformation::new(2),
                brand: "test".into(),
                sender: PlayerSender::new(tx, Arc::default()),
                latency: Arc::new(AtomicI32::new(0)),
                address: "127.0.0.1:25565".parse().unwrap(),
                expected: false,
            },
            2,
            [8.5, 200.0, 8.5],
            2,
        );
        guest.dimension = "minecraft:overworld".into();
        guest.placed = true;
        guest.loaded = true;
        game.lan.as_mut().unwrap().hub.guests.push(guest);
        game
    }

    fn dispense_at_guest(game: &mut Game<'_>, item: &str) -> (bool, u32) {
        let mut stack = ItemStack::new(item, 2);
        // The dispenser sits one block over the guest's feet, facing down.
        let done = game.dispense_equipment((8, 201, 8), Direction::Down, &mut stack);
        (done, stack.count)
    }

    /// `EquipmentDispenseItemBehavior`: a server's player, the integrated
    /// owner or a guest, takes the piece (a server has no player of its own).
    #[test]
    #[ignore = "daily"]
    fn a_dispenser_equips_a_server_player_in_front() {
        let mut resources = None;
        let mut game = server_with_guest(&mut resources);
        assert!(game.player.is_none(), "the server thread has no player");
        // The guest's feet are in block (8, 200, 8): the dispenser at y 201
        // facing down targets the block under it.
        let mut stack = ItemStack::new("minecraft:carved_pumpkin", 3);
        assert!(game.dispense_equipment((8, 201, 8), Direction::Down, &mut stack));
        assert_eq!(stack.count, 2);
        let guest = game.lan.as_ref().unwrap().hub.guest(2).unwrap();
        assert_eq!(
            guest.armor[0].as_ref().map(|s| (&*s.item, s.count)),
            Some(("minecraft:carved_pumpkin", 1))
        );
        // The head is taken now: a second pumpkin is not dispensed onto them.
        assert!(!game.dispense_equipment((8, 201, 8), Direction::Down, &mut stack));
        assert_eq!(stack.count, 2);
        // A boots piece still has its own empty slot.
        let (done, left) = dispense_at_guest(&mut game, "minecraft:iron_boots");
        assert_eq!((done, left), (true, 1), "boots go on the free feet slot");
        let guest = game.lan.as_ref().unwrap().hub.guest(2).unwrap();
        assert!(guest.armor[3].is_some() && guest.armor[1].is_none());
    }

    /// A spectator is not a target (`canEquipWithDispenser`: `!isSpectator()`).
    #[test]
    #[ignore = "daily"]
    fn a_dispenser_skips_a_spectator_and_a_dead_player() {
        let mut resources = None;
        let mut game = server_with_guest(&mut resources);
        for spectate in [true, false] {
            game.lan.as_mut().unwrap().hub.guests.update(0, |g| {
                g.game_mode = if spectate { 3 } else { 0 };
                g.dead = !spectate;
            });
            let mut stack = ItemStack::new("minecraft:iron_helmet", 1);
            assert!(!game.dispense_equipment((8, 201, 8), Direction::Down, &mut stack));
            assert_eq!(stack.count, 1);
        }
    }

    /// `ArmorStand.canUseSlot` through `isDisabled`: the first stand in front
    /// that can wear the piece gets it. Bit `1 << filterBit` locks a slot
    /// (head 4, chest 3, legs 2, feet 1); a hand needs arms.
    #[test]
    #[ignore = "daily"]
    fn a_dispenser_skips_an_armor_stand_whose_slot_is_locked() {
        use myriade_engine::entities::ArmorStand;
        let mut resources = None;
        let mut game = server_with_guest(&mut resources);
        game.lan.as_mut().unwrap().hub.guests.clear();
        let at = horizon_core::geometry::Vec3::new(8.5, 200.0, 8.5);
        let mut stands = [(1, 1u32 << 4), (2, 0)].map(|(id, locked)| {
            let mut s = ArmorStand::new(id, at, 0.0);
            s.disabled_slots = locked as i32;
            s
        });
        stands[0].show_arms = true;
        crate::game::ecs::test_support::clear_test_stands(&mut game);
        for s in &stands {
            crate::game::ecs::test_support::place_test_stand(&mut game, s.id, s);
        }
        // The head-locked first stand is skipped for a helmet; the second wears it.
        let mut stack = ItemStack::new("minecraft:iron_helmet", 1);
        assert!(game.dispense_equipment((8, 201, 8), Direction::Down, &mut stack));
        assert!(game.armor_stand_list()[0].equipment[5].is_none());
        assert!(game.armor_stand_list()[1].equipment[5].is_some());
        // The same stand takes chest armour: only its head is locked.
        let mut stack = ItemStack::new("minecraft:iron_chestplate", 1);
        assert!(game.dispense_equipment((8, 201, 8), Direction::Down, &mut stack));
        assert!(game.armor_stand_list()[0].equipment[4].is_some());
        // Every stand locked for that slot: nothing takes it (the dispenser ejects it).
        game.armor_stand_edit(game.armor_stand_ids()[0], |a| a.disabled_slots = 1 << 4);
        game.armor_stand_edit(game.armor_stand_ids()[1], |a| a.disabled_slots = 1 << 4);
        game.armor_stand_edit(game.armor_stand_ids()[1], |a| a.equipment[5] = None);
        let mut stack = ItemStack::new("minecraft:iron_helmet", 1);
        assert!(!game.dispense_equipment((8, 201, 8), Direction::Down, &mut stack));
        assert_eq!(stack.count, 1);
    }

    /// A stand of entity id `id` standing at `x`, 200, `z`.
    fn stand_at(id: u32, x: f64, z: f64) -> myriade_engine::entities::ArmorStand {
        let at = horizon_core::geometry::Vec3::new(x, 200.0, z);
        myriade_engine::entities::ArmorStand::new(id, at, 0.0)
    }

    /// A guest whose client is still on its loading screen (joined or
    /// respawned, no ServerboundPlayerLoadedPacket yet) is in the level all the
    /// same: `getEntitiesOfClass` finds it and it wears the piece.
    #[test]
    #[ignore = "daily"]
    fn a_dispenser_equips_a_player_whose_client_is_still_loading() {
        let mut resources = None;
        let mut game = server_with_guest(&mut resources);
        game.lan.as_mut().unwrap().hub.guests.update(0, |g| {
            (g.loaded, g.loading) = (false, true);
        });
        let (done, left) = dispense_at_guest(&mut game, "minecraft:iron_helmet");
        assert_eq!((done, left), (true, 1));
        let guest = game.lan.as_ref().unwrap().hub.guest(2).unwrap();
        let head = guest.armor[0].as_ref().map(|s| &*s.item);
        assert_eq!(head, Some("minecraft:iron_helmet"));
    }

    /// Players are entities of the level: in one section they go in join
    /// order (entity ids) with the mobs and stands, not after them.
    #[test]
    #[ignore = "daily"]
    fn a_player_and_a_stand_in_front_go_in_join_order() {
        // (stand id, guest id) -> who wears the helmet: the lower id joined first.
        for (stand_id, guest_id, guest_first) in [(7, 5, true), (3, 5, false)] {
            let mut resources = None;
            let mut game = server_with_guest(&mut resources);
            game.lan
                .as_mut()
                .unwrap()
                .hub
                .guests
                .update(0, |g| g.entity_id = guest_id);
            crate::game::ecs::test_support::clear_test_stands(&mut game);
            let s = stand_at(stand_id, 8.5, 8.5);
            crate::game::ecs::test_support::place_test_stand(&mut game, stand_id, &s);
            let (done, _) = dispense_at_guest(&mut game, "minecraft:iron_helmet");
            assert!(done);
            let guest = game.lan.as_ref().unwrap().hub.guest(2).unwrap();
            assert_eq!(
                guest.armor[0].is_some(),
                guest_first,
                "stand {stand_id}, guest {guest_id}"
            );
            assert_eq!(game.armor_stand_list()[0].equipment[5].is_some(), !guest_first);
        }
    }

    /// `forEachAccessibleNonEmptySection` walks sections before join order:
    /// a stand whose feet are in section x 1 but whose box reaches into block
    /// x 15 comes after one in section x 0, even with the lower id.
    #[test]
    #[ignore = "daily"]
    fn a_dispenser_walks_entity_sections_before_ids() {
        let mut resources = None;
        let mut game = server_with_guest(&mut resources);
        game.lan.as_mut().unwrap().hub.guests.clear();
        // Stand boxes are 0.5 wide: x 16.2 spans 15.95..16.45.
        crate::game::ecs::test_support::clear_test_stands(&mut game);
        game.armor_stand_list()
            .extend(vec![stand_at(1, 16.2, 8.5), stand_at(2, 15.5, 8.5)]);
        let mut stack = ItemStack::new("minecraft:iron_helmet", 1);
        assert!(game.dispense_equipment((15, 201, 8), Direction::Down, &mut stack));
        assert!(game.armor_stand_list()[0].equipment[5].is_none());
        assert!(game.armor_stand_list()[1].equipment[5].is_some());
    }

    /// `SectionPos.asLong` packs z (22 bits) over y (20 bits) under a fixed x,
    /// and the section set is sorted as longs: z -1 packs as 0x3FFFFF << 20,
    /// above z 0's 0; y -1 at z 0 packs as 0xFFFFF, below z 1's 1 << 20.
    #[test]
    fn sections_sort_as_their_packed_keys() {
        use horizon_core::geometry::Vec3;
        let at = |x: f64, y: f64, z: f64| super::section_order(Vec3::new(x, y, z));
        assert!(at(0.0, 0.0, -1.0) > at(0.0, 0.0, 0.0));
        assert!(at(0.0, -1.0, 0.0) < at(0.0, 0.0, 16.0));
        assert!(at(0.0, -1.0, 0.0) > at(0.0, 0.0, 0.0));
        // x is the outer loop, ascending as a signed coordinate.
        assert!(at(-1.0, 0.0, 0.0) < at(0.0, 0.0, 0.0));
        assert!(at(15.9, 0.0, 0.0) < at(16.0, 0.0, 0.0));
    }
}
