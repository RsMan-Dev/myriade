//! `/data` (DataCommands), `/tag` (TagCommand) and `/summon` with NBT: entity,
//! block entity and storage data accessors over the game state.
use super::command_host::{data::DataTarget, tag::TagTargets};
use super::{Arg, Context, Game, R, Target, err};
use myriade_engine::{
    containers::BlockEntity,
    data_cmd::{self, path_error},
    entity_nbt,
    nbt_path::NbtPath,
    save::{self, random_uuid},
    snbt,
    text::Text,
};
use horizon_core::nbt::{Compound, JavaString, Tag, parse_snbt};

/// A `DataAccessor`.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::game) enum DataRef {
    Entity(Target),
    Block([i32; 3]),
    Storage(String),
}

fn empty() -> Tag {
    Tag::Compound(Compound::new())
}
fn set_key(tag: &mut Tag, key: &str, value: Tag) {
    if let Tag::Compound(c) = tag {
        c.insert(JavaString::from(key), value);
    }
}
/// `SummonCommand.spawnEntity`'s name for a kind no command target stands for:
/// its entity type's. An item is the exception: `EntityType`'s factory leaves
/// `ItemStack.EMPTY`, which `ItemEntity.readAdditionalSaveData` discards, and
/// `ItemEntity.getName` reads that empty stack's `getItemName` — an empty
/// component, not the type's name.
pub(super) fn summon_type_name(kind: &str) -> Text {
    if kind.trim_start_matches("minecraft:") == "item" {
        return Text::empty();
    }
    Text::translate(&super::entity_name_key(kind), &[])
}
/// `EntityType.loadEntityRecursive` snaps every entity of the stack,
/// passengers included, to the position (`/summon`, `summon`).
pub(in crate::game) fn place_summoned(tag: &mut Tag, pos: [f64; 3]) {
    let p = pos.map(|v| Tag::Double(v.to_bits()));
    set_key(tag, "Pos", Tag::List(p.to_vec()));
    // Entity.load: an absent Fire reads as 0 (until the entity moves).
    if tag.get("Fire").is_none() {
        set_key(tag, "Fire", Tag::Short(0));
    }
    if tag.get("Rotation").is_none() {
        set_key(
            tag,
            "Rotation",
            Tag::List(vec![
                Tag::Float(0.0f32.to_bits()),
                Tag::Float(0.0f32.to_bits()),
            ]),
        );
    }
    if let Tag::Compound(c) = tag
        && let Some(Tag::List(riders)) = c.get_mut(&"Passengers".into())
    {
        for r in riders {
            if matches!(r, Tag::Compound(_)) {
                place_summoned(r, pos);
            }
        }
    }
}

impl Game<'_> {
    // ---- entity data ---------------------------------------------------

    /// `NbtPredicate.getEntityTagToCompare`: the entity as saved. An entity
    /// in another level is read in the level that holds it.
    pub(in crate::game) fn command_target_nbt(&mut self, t: Target) -> Option<Tag> {
        self.in_target_level(t, |g| {
            let mut body = g.entity_nbt_in_level(t)?;
            if !matches!(
                t,
                Target::Mob(_) | Target::Boat(_) | Target::Cart(_) | Target::Tnt(_)
            ) && let Some(id) = super::targets::target_id(t)
            {
                g.put_mob_passengers(&mut body, id);
            }
            Some(body)
        })
        .flatten()
    }
    /// The acting player's own UUID: a guest's turn (the window's player on
    /// the server thread) keeps the world owner's in `level`.
    fn saved_player_uuid(&self) -> Option<[i32; 4]> {
        self.lan_acting_uuid
            .map(myriade_engine::lan::playerdata::uuid_ints)
            .or(self.level.player_uuid)
    }
    /// [`Self::entity_nbt`] of the player whose turn it is.
    pub(in crate::game) fn player_nbt(&mut self) -> Option<Tag> {
        self.player.as_ref()?;
        self.sync_level();
        let mut tag = self.level.player_tag();
        // `ServerPlayer.addAdditionalSaveData` always stores the tracker.
        self.load_warden_tracker();
        set_key(
            &mut tag,
            "warden_spawn_tracker",
            self.sculk.tracker.to_nbt(),
        );
        if let Some(u) = self.saved_player_uuid() {
            set_key(&mut tag, "UUID", Tag::IntArray(u.to_vec()));
        }
        // NbtPredicate.getEntityTagToCompare: a player's held stack.
        let selected = self.level.selected_slot;
        let held = match tag.get("Inventory") {
            Some(Tag::List(items)) => items
                .iter()
                .find(|i| matches!(i.get("Slot"), Some(Tag::Byte(b)) if i32::from(*b) == selected))
                .cloned(),
            _ => None,
        };
        if let Some(mut stack) = held {
            if let Tag::Compound(c) = &mut stack {
                c.remove(&"Slot".into());
            }
            set_key(&mut tag, "SelectedItem", stack);
        }
        Some(tag)
    }
    fn entity_nbt_in_level(&mut self, t: Target) -> Option<Tag> {
        Some(match t {
            // `FishingHook.shouldBeSaved` is false, and it keeps no compound here.
            // Neither is saved: `FishingHook.shouldBeSaved` and
            // `EnderDragonPart.shouldBeSaved` are false.
            Target::Hook(_) | Target::DragonPart(_) => return None,
            Target::General(id) | Target::Projectile(id) => return self.ecs_mob_tree(id).map(Tag::Compound),
            Target::Guest(_) => {
                return self
                    .for_player(t, |g| g.command_target_nbt(Target::Player))
                    .flatten();
            }
            Target::Player => return self.player_nbt(),
            Target::Mob(id) => {
                // `saveWithoutId` writes the entity as saved, riders under
                // `Passengers`: `/data get` reads the same tree persistence does (arismoko/horizon#1519).
                let mut tag = Tag::Compound(self.ecs_mob_tree(id)?);
                // `Entity.saveWithoutId` always writes the UUID: the ECS holds one only once the host names it.
                if tag.get("UUID").is_none() {
                    let uuid = self.uuid_for(id);
                    // The ECS keeps it too, so the save writes the one the commands showed.
                    if let Some(e) = self.ecs.entity_of(id) {
                        self.ecs.set_uuid(e, uuid);
                    }
                    set_key(&mut tag, "UUID", Tag::IntArray(uuid.to_vec()));
                }
                let session = self
                    .lan_trade_conn(id)
                    .and_then(|conn| self.lan_trade_session(conn))
                    .or_else(|| {
                        self.station
                            .as_ref()
                            .and_then(|s| s.merchant.as_ref())
                            .filter(|m| m.mob == Some(id))
                            .map(|m| &*m.session)
                    });
                if let (Some(session), Tag::Compound(data)) = (session, &mut tag) {
                    data.insert("Offers".into(), entity_nbt::offers_to_nbt(&session.offers));
                    data.insert("Xp".into(), Tag::Int(session.xp));
                }
                tag
            }
            Target::Item(id) => {
                let mut tag = Tag::Compound(self.ecs_mob_tree(id)?);
                self.save_portal_cooldown(id, &mut tag);
                tag
            }
            Target::Stand(id) => {
                let uuid = self.uuid_for(id);
                let a = self.armor_stand_saved(id)?;
                entity_nbt::armor_stand_to_nbt(&a, uuid, self.persist.raw.get(&id))
            }
            Target::Frame(id) => {
                let uuid = self.uuid_for(id);
                let f = self.ecs_frames().into_iter().find(|f| f.id == id)?;
                entity_nbt::item_frame_to_nbt(&f, uuid, self.persist.raw.get(&id))
            }
            Target::Painting(id) => {
                let uuid = self.uuid_for(id);
                let p = self.ecs_paintings().into_iter().find(|p| p.id == id)?;
                entity_nbt::painting_to_nbt(&p, uuid, self.persist.raw.get(&id))
            }
            Target::Boat(id) | Target::Cart(id) => {
                // The ECS tree already carries the riders under `Passengers` (as a mob's does): adding them again listed each one twice.
                let mut tag = Tag::Compound(self.ecs_mob_tree(id)?);
                self.save_portal_cooldown(id, &mut tag);
                tag
            }
            Target::Fangs(id) => Tag::Compound(self.ecs_mob_tree(id)?),
            Target::Other(_) => return None,
            Target::Crystal(id) | Target::Dragon(id) => Tag::Compound(self.ecs_mob_tree(id)?),
            Target::Tnt(id) | Target::Trident(id) | Target::Rocket(id) | Target::Falling(id) | Target::Orb(id) => {
                Tag::Compound(self.ecs_mob_tree(id)?)
            }
            Target::Cushion(id) => {
                let uuid = self.uuid_for(id);
                let c = self.cushion_saved(id)?;
                entity_nbt::cushion_to_nbt(&c, uuid, self.persist.raw.get(&id))
            }
            Target::Cloud(id) => Tag::Compound(self.ecs_mob_tree(id)?),
            Target::Spawner(id) => {
                let e = self.ecs.entity_of(id)?;
                Tag::Compound(self.ecs.entity_nbt(e)?)
            }
            // LeashFenceKnotEntity saves nothing of its own: the common
            // entity fields, as an explicitly summoned knot's.
            Target::Knot(id) => {
                let fence = self.natural_knot(id)?;
                let uuid = *self.persist.uuids.get(&id)?;
                let knot = super::OtherEntity {
                    // A serialization view, not a second resident entity.
                    join: myriade_engine::join_seq::JoinSeq::UNJOINED,
                    id,
                    kind: "minecraft:leash_knot".into(),
                    position: myriade_engine::mobs::leash::knot_state(fence).position,
                    yaw: 0.0,
                    pitch: 0.0,
                    item_transformation: None,
                    interaction: None,
                    knot_check: None,
                };
                let mut tag = self.other_to_nbt(&knot, uuid);
                // Entity.saveWithoutId writes `remainingFireTicks` always; the
                // knot's never changes (BlockAttachedEntity.tick has no
                // baseTick).
                set_key(&mut tag, "Fire", Tag::Short(0));
                tag
            }
        })
    }
    /// `Entity.load` of new data (`EntityDataAccessor.setData`): the entity is
    /// rebuilt from its compound, keeping its id and UUID. Also what
    /// `TypedEntityData.loadInto` is: a spawn egg's data on the mob it spawned.
    /// A trading screen open on `id` holds its own copy of the offers and the experience (newer than the mob's until it
    /// closes): a `/data modify` of those keys goes to that copy too, or `/data get` and the next trade would read the old ones.
    fn refresh_open_trade(&mut self, id: u32, tag: &Tag) {
        let offers = tag.get("Offers").map(entity_nbt::offers_from_nbt);
        let xp = match tag.get("Xp") {
            Some(Tag::Int(x)) => Some(*x),
            _ => None,
        };
        if offers.is_none() && xp.is_none() {
            return;
        }
        let edit = |s: &mut myriade_engine::merchant::Session| {
            if let Some(o) = offers.clone() {
                s.offers = o;
            }
            if let Some(x) = xp {
                s.xp = x;
            }
        };
        if let Some(conn) = self.lan_trade_conn(id) {
            self.lan_trade_edit(conn, edit);
        } else if let Some(m) = self.station.as_mut().and_then(|s| s.merchant.as_mut()).filter(|m| m.mob == Some(id)) {
            edit(&mut *m.session);
        }
    }
    pub(in crate::game) fn apply_entity_nbt(&mut self, t: Target, mut tag: Tag) -> R<()> {
        let stale = || err("commands.data.merge.failed", &[]);
        // `Entity.load` keeps only the fields it reads, so vanilla drops a key it
        // does not know on `EntityDataAccessor.setData` (and on
        // `TypedEntityData.loadInto`). `horizon:finalize` is worldgen's, and
        // `Game::load_mob` would run the finalize it asks for on the next load.
        strip_finalize_tree(&mut tag);
        // Entity.load does not read Passengers: the riders stay, and so do the
        // passengers the load kept raw (none, when it kept none).
        if let Tag::Compound(c) = &mut tag {
            c.remove(&"Passengers".into());
        }
        match t {
            // EntityDataAccessor.setData: players can't be modified, guests neither.
            Target::Player | Target::Guest(_) => {
                return Err(err("commands.data.entity.invalid", &[]));
            }
            Target::General(id) | Target::Projectile(id) | Target::Dragon(id) => {
                let Tag::Compound(patch) = &tag else {
                    return Err(stale());
                };
                if !self.ecs_merge_mob(id, patch) {
                    return Err(stale());
                }
            }
            Target::Mob(id) => {
                Self::normalize_loaded_mob_tags(&mut tag);
                let uuid = self.uuid_for(id);
                set_key(&mut tag, "UUID", Tag::IntArray(uuid.to_vec()));
                if let Some(k) = self.ecs_mob_kind(id) {
                    set_key(&mut tag, "id", Tag::String(k.into()));
                }
                let Tag::Compound(patch) = &tag else {
                    return Err(stale());
                };
                if !self.ecs_merge_mob(id, patch) {
                    return Err(stale());
                }
                self.refresh_open_trade(id, &tag);
            }
            Target::Fangs(id) => {
                // RESTE: Warmup et Owner ne sont pas dans le composant ECS du croc, seuls la place et le cap sont écrits.
                let at = entity_nbt::position(&tag).ok_or_else(stale)?;
                self.move_fang(id, [at.x, at.y, at.z], Some(entity_nbt::rotation(&tag)[0]));
            }
            Target::Stand(id) => {
                // The merged compound goes whole onto the ECS entity: keys the struct does not model survive.
                entity_nbt::armor_stand_from_nbt(&tag, id).ok_or_else(stale)?;
                let Tag::Compound(full) = &tag else { return Err(stale()) };
                self.ecs_merge_mob(id, full);
            }
            Target::Item(id) | Target::Boat(id) | Target::Cart(id) | Target::Tnt(id) | Target::Trident(id) | Target::Rocket(id) | Target::Falling(id) | Target::Orb(id) | Target::Frame(id) | Target::Painting(id) | Target::Other(id) => {
                let uuid = self.uuid_for(id);
                set_key(&mut tag, "UUID", Tag::IntArray(uuid.to_vec()));
                let Tag::Compound(patch) = &tag else {
                    return Err(stale());
                };
                if !self.ecs_merge_mob(id, patch) {
                    return Err(stale());
                }
            }
            _ => return Err(stale()),
        }
        if let Target::Mob(id)
        | Target::Item(id)
        | Target::Stand(id)
        | Target::Frame(id)
        | Target::Painting(id)
        | Target::Boat(id)
        | Target::Cart(id)
        | Target::Tnt(id)
        | Target::Orb(id)
        | Target::Cushion(id)
        | Target::Trident(id)
        | Target::Rocket(id)
        | Target::Other(id)
        | Target::Fangs(id)
        | Target::Projectile(id) = t
        {
            self.persist.raw.insert(id, tag);
        }
        Ok(())
    }
    /// The block entity part of `BlockInput.place`: load the block entity
    /// from the argument's tag (read; `load_block_entity_nbt` reads SNBT); true when its saved data changed.
    pub(in crate::game) fn load_block_entity_tag(
        &mut self,
        pos: [i32; 3],
        mut tag: horizon_core::nbt::Tag,
    ) -> bool {
        use horizon_core::nbt::Tag;
        if self.pot_data(pos).is_some() {
            return self.set_pot_data(pos, &tag).unwrap_or(false);
        }
        if let Some(changed) = self.load_comparator_output(pos, &tag) {
            let changed = changed.unwrap_or(false);
            if changed {
                self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
            }
            return changed;
        }
        let key = (pos[0], pos[1], pos[2]);
        let was_auto = self.command_block_auto(key);
        let Some(e) = self.world.block_entities.get(&key) else {
            return false;
        };
        let name = self.data_block_name(pos);
        let before = myriade_engine::save::block_entity_encode::encode(key, e, &name);
        if let Tag::Compound(c) = &mut tag {
            for k in ["id", "x", "y", "z"] {
                if let Some(v) = before.get(k) {
                    c.insert(k.into(), v.clone());
                }
            }
        }
        let Some((_, mut new)) = self.decode_block_entity_at(key, &tag) else {
            return false;
        };
        let rolled = self.take_loot_table(key, &tag, &mut new);
        let after = myriade_engine::save::block_entity_encode::encode(key, &new, &name);
        // The lock is outside the model: a tag that only changes it still
        // changes the block entity.
        let lock_changed = self.world.block_lock(key) != tag.get("lock");
        // So is the custom name.
        let name_changed = self.load_block_name(key, &new, &tag);
        let cooldown_changed = if name == "minecraft:hopper" {
            let state = super::super::machines::HopperState::load(&tag);
            let changed = self.hoppers.get(&key).map_or(-1, |s| s.cooldown) != state.cooldown;
            self.hoppers.insert(key, state);
            changed
        } else {
            false
        };
        if after == before && !rolled && !lock_changed && !name_changed && !cooldown_changed {
            return false;
        }
        self.world.block_entities.insert(key, new);
        self.world.set_block_lock(key, &tag);
        self.command_block_loaded(key, was_auto);
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        true
    }
    pub(in crate::game) fn data_block_name(&self, p: [i32; 3]) -> String {
        let state = self.world.block(p[0], p[1], p[2]);
        self.pack
            .assets
            .registry
            .state(state)
            .map_or_else(|| "minecraft:air".into(), |s| s.name.to_string())
    }
    fn raw_mut(&mut self, t: Target) -> Option<&mut Tag> {
        match t {
            Target::Player => Some(self.level.player_extra.get_or_insert_with(empty)),
            Target::Guest(conn) => self
                .lan
                .as_mut()?
                .hub
                .guests
                .saved_data_mut(conn)
                .map(|saved| saved.get_or_insert_with(empty)),
            Target::Mob(id)
            | Target::Item(id)
            | Target::Stand(id)
            | Target::Frame(id)
            | Target::Painting(id)
            | Target::Boat(id)
            | Target::Cart(id)
            // `tags_of` reads a primed TNT's compound; without this arm
            // `/tag` wrote nothing and still reported success.
            | Target::Tnt(id)
            | Target::Cushion(id)
            | Target::Orb(id)
            | Target::Projectile(id)
            | Target::Crystal(id)
            | Target::Dragon(id)
            | Target::Other(id)
            | Target::Fangs(id)
            | Target::Spawner(id)
            | Target::Cloud(id) => Some(self.persist.raw.entry(id).or_insert_with(empty)),
            // `tags_of` reads these two from the same map and a falling
            // block from its saved compound; without these arms `/tag`
            // wrote nothing and still reported success.
            Target::Rocket(id) | Target::Trident(id) | Target::Falling(id) => {
                Some(self.persist.raw.entry(id).or_insert_with(empty))
            }
            _ => None,
        }
    }
    /// Writes `t`'s `Tags`; false when its kind keeps no saved compound to
    /// write them to.
    pub(in crate::game) fn set_entity_tags(&mut self, t: Target, tags: &[String]) -> bool {
        if let Target::Mob(id) = t {
            return self.ecs_set_mob_tags(id, tags);
        }
        if let Some(id) = super::targets::target_id(t)
            && self.ecs.exists(id)
        {
            return self.ecs_set_mob_tags(id, tags);
        }
        let Some(raw) = self.raw_mut(t) else {
            return false;
        };
        // `Entity.saveWithoutId` writes `Tags` only for a nonempty live set:
        // removing the last tag leaves no key.
        if tags.is_empty() {
            if let Tag::Compound(map) = raw {
                map.remove(&"Tags".into());
            }
            return true;
        }
        let list = tags
            .iter()
            .map(|s| Tag::String(s.as_str().into()))
            .collect();
        set_key(raw, "Tags", Tag::List(list));
        true
    }

    /// `ComparatorBlockEntity.loadAdditional` (`getIntOr("OutputSignal", 0)`)
    /// for a comparator at `p`: None for any other block; else whether the
    /// loaded output differs from the one it had (None inside when the tag
    /// does not decode). The block's own metadata comes from `comparator_data`.
    fn load_comparator_output(&mut self, p: [i32; 3], tag: &Tag) -> Option<Option<bool>> {
        let mut input = self.comparator_data(p)?;
        let before = input.get("OutputSignal").cloned();
        if let Some(value) = tag.get("OutputSignal") {
            set_key(&mut input, "OutputSignal", value.clone());
        } else if let Tag::Compound(map) = &mut input {
            map.remove(&"OutputSignal".into());
        }
        let Some(normalized) = myriade_engine::block_entity_nbt::canonical(&input) else {
            return Some(None);
        };
        let output = normalized.get("OutputSignal").cloned();
        if let Some(Tag::Int(value)) = output {
            let pos = horizon_core::position::BlockPos::new(p[0], p[1], p[2]);
            self.block_ticks.comparator_outputs.insert(pos, value);
        }
        Some(Some(output != before))
    }

    /// `ComparatorBlockEntity.saveAdditional`: expose the circuit's live output,
    /// rather than keeping a second copy in the ordinary block entity map.
    fn comparator_data(&self, p: [i32; 3]) -> Option<Tag> {
        if self.data_block_name(p) != "minecraft:comparator" {
            return None;
        }
        let pos = horizon_core::position::BlockPos::new(p[0], p[1], p[2]);
        let output = self
            .block_ticks
            .comparator_outputs
            .get(&pos)
            .copied()
            .or_else(|| {
                let saved = self
                    .world
                    .chunks
                    .get(&(p[0] >> 4, p[2] >> 4))?
                    .saved
                    .as_deref()?;
                let tag = saved.block_entities.iter().find(|t| {
                    [t.get("x"), t.get("y"), t.get("z")]
                        .map(|v| save::number(v).unwrap_or(0) as i32)
                        == p
                        && save::text(t.get("id")).as_deref() == Some("minecraft:comparator")
                })?;
                let tag = myriade_engine::block_entity_nbt::canonical(tag)?;
                match tag.get("OutputSignal") {
                    Some(Tag::Int(v)) => Some(*v),
                    _ => None,
                }
            })
            .unwrap_or(0);
        Some(Tag::compound([
            ("id", Tag::String("minecraft:comparator".into())),
            ("x", Tag::Int(p[0])),
            ("y", Tag::Int(p[1])),
            ("z", Tag::Int(p[2])),
            ("components", empty()),
            ("OutputSignal", Tag::Int(output)),
        ]))
    }

    // ---- accessors -----------------------------------------------------

    /// The `role`th provider of the command (`entity`, `block` or `storage`).
    pub(in crate::game) fn data_ref(&mut self, ctx: &Context, role: usize) -> R<DataRef> {
        let prefix = if role == 0 { "target" } else { "source" };
        self.data_ref_named(ctx, role, prefix)
    }
    /// `PistonMovingBlockEntity.saveAdditional`, with full command metadata.
    fn moving_piston_data(&self, p: [i32; 3]) -> Option<Tag> {
        let pos = horizon_core::position::BlockPos::new(p[0], p[1], p[2]);
        let (_, piston) = self
            .block_ticks
            .moving_pistons
            .iter()
            .find(|(at, _)| *at == pos)?;
        let mut tag = self.moving_piston_to_nbt(pos, piston);
        // BlockEntity.saveWithoutMetadata includes the component map.
        set_key(&mut tag, "components", empty());
        Some(tag)
    }
    pub(in crate::game) fn data_ref_named(
        &mut self,
        ctx: &Context,
        role: usize,
        prefix: &str,
    ) -> R<DataRef> {
        let kind = ctx
            .literals
            .iter()
            .filter(|l| matches!(l.as_str(), "entity" | "block" | "storage"))
            .nth(role)
            .cloned()
            .unwrap_or_default();
        match kind.as_str() {
            "entity" => {
                let Some(Arg::Entity(sel)) = ctx.get(prefix).cloned() else {
                    return Err(err("argument.entity.notfound.entity", &[]));
                };
                let found = self.select(&sel);
                found
                    .first()
                    .map(|t| DataRef::Entity(*t))
                    .ok_or_else(|| err("argument.entity.notfound.entity", &[]))
            }
            "block" => {
                let p = self.block_pos_arg(ctx, &format!("{prefix}Pos"))?;
                if !self.world.block_entities.contains_key(&(p[0], p[1], p[2]))
                    && self.comparator_data(p).is_none()
                    && self.pot_data(p).is_none()
                    && self.moving_piston_data(p).is_none()
                {
                    return Err(err("commands.data.block.invalid", &[]));
                }
                Ok(DataRef::Block(p))
            }
            _ => match ctx.get(prefix) {
                Some(Arg::Resource(id)) => Ok(DataRef::Storage(id.clone())),
                _ => Err(err("command.failed", &[])),
            },
        }
    }
    pub(in crate::game) fn data_of(&mut self, r: &DataRef) -> R<Tag> {
        match r {
            // EntityDataAccessor.getData: Entity.saveWithoutId.
            DataRef::Entity(t) => {
                let mut tag = self
                    .command_target_nbt(*t)
                    .ok_or_else(|| err("argument.entity.notfound.entity", &[]))?;
                if let Tag::Compound(c) = &mut tag {
                    c.remove(&"id".into());
                }
                Ok(tag)
            }
            DataRef::Block(p) => self
                .block_entity_nbt(*p)
                .ok_or_else(|| err("commands.data.block.invalid", &[])),
            DataRef::Storage(id) => Ok(self.score.storage.get(id)),
        }
    }
    /// `BlockEntity.saveWithFullMetadata` of the block entity at `p`: no
    /// chunk-only keepPacked, and the lock and custom name the model does
    /// not hold. `None` where there is no block entity.
    pub(in crate::game) fn block_entity_nbt(&self, p: [i32; 3]) -> Option<Tag> {
        // A live moving piston is the level's block entity there (#9617).
        if let Some(tag) = self.moving_piston_data(p) {
            return Some(tag);
        }
        if let Some(tag) = self.comparator_data(p).or_else(|| self.pot_data(p)) {
            return Some(tag);
        }
        let key = (p[0], p[1], p[2]);
        let name = self.data_block_name(p);
        let e = self.world.block_entities.get(&key)?;
        let mut tag = save::block_entity_data(key, e, &name, self.world.block_lock(key));
        if name == "minecraft:hopper" {
            set_key(
                &mut tag,
                "TransferCooldown",
                Tag::Int(self.hopper_cooldown(key)),
            );
        }
        if let (Some(custom), Tag::Compound(map)) = (self.world.block_names.get(&key), &mut tag) {
            map.entry("CustomName".into())
                .or_insert_with(|| custom.clone());
        }
        Self::write_pending_loot(&self.world, key, &mut tag);
        Some(tag)
    }
    pub(in crate::game) fn set_data_of(&mut self, r: &DataRef, tag: Tag) -> R<()> {
        match r {
            DataRef::Entity(t) => self.apply_entity_nbt(*t, tag),
            DataRef::Block(p) => {
                // This view is read-only: never decode a live piston into a
                // second, detached entry in world.block_entities.
                if self.moving_piston_data(*p).is_some() {
                    return Err(err("commands.data.block.invalid", &[]));
                }
                if self.set_pot_data(*p, &tag).is_some() {
                    return Ok(());
                }
                let key = (p[0], p[1], p[2]);
                if let Some(changed) = self.load_comparator_output(*p, &tag) {
                    changed.ok_or_else(|| err("commands.data.block.invalid", &[]))?;
                    self.mark_block_entity_updated(key);
                    return Ok(());
                }
                if let Some((_, mut new)) = self.decode_block_entity_at(key, &tag) {
                    let was_auto = self.command_block_auto(key);
                    if self.data_block_name(*p) == "minecraft:hopper" {
                        self.hoppers
                            .insert(key, super::super::machines::HopperState::load(&tag));
                    }
                    self.take_loot_table(key, &tag, &mut new);
                    self.load_block_name(key, &new, &tag);
                    self.world.block_entities.insert(key, new);
                    self.world.set_block_lock(key, &tag);
                    self.command_block_loaded(key, was_auto);
                    // BlockDataAccessor.setData: setChanged and sendBlockUpdated(3),
                    // even when the block state itself stays the same.
                    self.mark_block_entity_updated(key);
                    let state = self.world.block(p[0], p[1], p[2]);
                    // Redraw what shows the block entity (signs, banners).
                    let _ = self.world.set(p[0], p[1], p[2], state);
                }
                Ok(())
            }
            DataRef::Storage(id) => {
                self.score.storage.set(id, tag);
                Ok(())
            }
        }
    }
    /// `BlockEntity.loadWithComponents` for the block entity at `key` from its full saved
    /// `tag` (id and position included): false when the tag doesn't decode.
    pub(in crate::game) fn load_block_entity_data(
        &mut self,
        key: (i32, i32, i32),
        tag: &Tag,
    ) -> bool {
        let state = self.world.block(key.0, key.1, key.2);
        let name = &self.res.facts().get(state).name;
        let Some((_, mut new)) = save::decode_block_entity_for_block(tag, name) else {
            return false;
        };
        self.take_loot_table(key, tag, &mut new);
        self.load_block_name(key, &new, tag);
        self.world.block_entities.insert(key, new);
        self.world.set_block_lock(key, tag);
        true
    }
    /// What `BlockDataAccessor.setData` tells the world after a load: `setChanged` and
    /// `sendBlockUpdated(3)`, even when the block state stays the same.
    pub(in crate::game) fn block_entity_updated(&mut self, key: (i32, i32, i32)) {
        self.mark_block_entity_updated(key);
        let state = self.world.block(key.0, key.1, key.2);
        // Redraw what shows the block entity (signs, banners).
        let _ = self.world.set(key.0, key.1, key.2, state);
    }
    fn data_name(&self, r: &DataRef) -> Text {
        match r {
            DataRef::Entity(t) => self.target_name(*t),
            DataRef::Block(p) => Text::literal(format!("{}, {}, {}", p[0], p[1], p[2])),
            DataRef::Storage(id) => Text::literal(id.clone()),
        }
    }
    pub(super) fn modified_text(&self, r: &DataRef) -> Text {
        match r {
            DataRef::Entity(_) => {
                Text::translate("commands.data.entity.modified", &[self.data_name(r)])
            }
            DataRef::Block(p) => Text::translate(
                "commands.data.block.modified",
                &[
                    Text::literal(p[0].to_string()),
                    Text::literal(p[1].to_string()),
                    Text::literal(p[2].to_string()),
                ],
            ),
            DataRef::Storage(id) => Text::translate(
                "commands.data.storage.modified",
                &[Text::literal(id.clone())],
            ),
        }
    }
    pub(super) fn query_text(&self, r: &DataRef, data: &Tag, path: Option<&NbtPath>) -> Text {
        // Only a block's writer supplies this provenance: the live piston's
        // fields, or any other block entity's saveWithFullMetadata. The same
        // compound in storage or an entity keeps default order.
        let root = path.is_none_or(|p| p.is_field_path(&[]));
        let piston = if let DataRef::Block(p) = r
            && self.moving_piston_data(*p).is_some()
        {
            match path {
                _ if root => Some(snbt::PistonPart::Entity),
                Some(p) if p.is_field_path(&["blockState"]) => Some(snbt::PistonPart::State),
                Some(p) if p.is_field_path(&["blockState", "properties"]) => {
                    Some(snbt::PistonPart::Properties)
                }
                _ => None,
            }
        } else if matches!(r, DataRef::Block(_)) && root {
            Some(snbt::PistonPart::BlockEntity)
        } else {
            None
        };
        let pretty = piston.map_or_else(|| snbt::pretty(data), |p| snbt::pretty_piston(data, p));
        match r {
            DataRef::Entity(_) => {
                Text::translate("commands.data.entity.query", &[self.data_name(r), pretty])
            }
            DataRef::Block(p) => Text::translate(
                "commands.data.block.query",
                &[
                    Text::literal(p[0].to_string()),
                    Text::literal(p[1].to_string()),
                    Text::literal(p[2].to_string()),
                    pretty,
                ],
            ),
            DataRef::Storage(id) => Text::translate(
                "commands.data.storage.query",
                &[Text::literal(id.clone()), pretty],
            ),
        }
    }
    pub(super) fn get_text(&self, r: &DataRef, path: &NbtPath, scale: f64, value: i32) -> Text {
        let p = Text::literal(path.as_str());
        let (s, v) = (
            data_cmd::scale_text(scale),
            Text::literal(value.to_string()),
        );
        match r {
            DataRef::Entity(_) => {
                Text::translate("commands.data.entity.get", &[p, self.data_name(r), s, v])
            }
            DataRef::Block(b) => Text::translate(
                "commands.data.block.get",
                &[
                    p,
                    Text::literal(b[0].to_string()),
                    Text::literal(b[1].to_string()),
                    Text::literal(b[2].to_string()),
                    s,
                    v,
                ],
            ),
            DataRef::Storage(id) => Text::translate(
                "commands.data.storage.get",
                &[p, Text::literal(id.clone()), s, v],
            ),
        }
    }

    // ---- /data -----------------------------------------------------------

    pub(super) fn path_arg(&self, ctx: &Context, name: &str) -> R<NbtPath> {
        let Some(Arg::Str(text)) = ctx.get(name) else {
            return Err(err("command.failed", &[]));
        };
        NbtPath::parse(text).map_err(path_error)
    }
    pub(super) fn cmd_data(&mut self, id: &str, ctx: &Context) -> R<i32> {
        let target = self.data_ref(ctx, 0)?;
        // An entity in another level is read and written in the level that
        // holds it.
        if let DataRef::Entity(t) = &target {
            let t = *t;
            let target = target.clone();
            return match self.in_target_level(t, |g| g.cmd_data_in(id, ctx, target)) {
                Some(r) => r,
                None => Err(err("argument.entity.notfound.entity", &[])),
            };
        }
        self.cmd_data_in(id, ctx, target)
    }
    /// `DataCommands.mergeData`: literal values keep the modern argument
    /// parser's decoded Tag contract through execution.
    fn cmd_data_in(&mut self, id: &str, ctx: &Context, target: DataRef) -> R<i32> {
        horizon_gameplay::commands::data::data_in(&mut DataTarget { game: self, target }, id, ctx)
    }
    /// A message sent without the feedback gamerule check (`sendSuccess(.., false)`).
    pub(super) fn system_feedback(&mut self, message: Text) {
        self.feedback(message);
    }
    /// `DataCommands.manipulateData`: literal sources use the same SNBT
    /// decode contract as command admission; stored sources are unchanged.
    pub(super) fn cmd_data_modify(&mut self, ctx: &Context, target: DataRef) -> R<i32> {
        horizon_gameplay::commands::data::data_modify(&mut DataTarget { game: self, target }, ctx)
    }

    // ---- /tag ------------------------------------------------------------

    pub(super) fn cmd_tag(&mut self, id: &str, ctx: &Context) -> R<i32> {
        let mut host = TagTargets {
            game: self,
            targets: Vec::new(),
        };
        horizon_gameplay::commands::tag::tag(&mut host, id, ctx)
    }

    // ---- /summon ---------------------------------------------------------

    /// Spawn an entity from its compound (`EntityType.loadEntityRecursive`);
    /// returns what was created.
    pub(in crate::game) fn spawn_entity_from_nbt(&mut self, tag: Tag) -> Option<Target> {
        let p = entity_nbt::position(&tag)?;
        let Some(Tag::String(kind)) = tag.get("id") else { return None };
        let kind = kind.to_string_lossy();
        let Tag::Compound(c) = &tag else { return None };
        // A species is made by the ECS's spawner; every other entity (boat, cart, TNT, arrow, stand, frame...) is what its compound loads.
        // A kind the ECS makes on a summon without saving it (the leash knot) is spawned, not loaded: its load refuses it.
        if !self.res.mob_types().contains_key(&*kind) && !self.ecs.summons_unsaved(&kind) {
            return self.ecs_load(c).map(Target::Mob);
        }
        let id = self.ecs_spawn(&kind, [p.x, p.y, p.z], Some(c))?;
        // A knot is listed by the selectors as a knot, once its identity is kept (`tie_knot` does the same for a lead's).
        if kind == "minecraft:leash_knot" {
            self.uuid_for(id);
            return Some(Target::Knot(id));
        }
        Some(Target::Mob(id))
    }
    /// The `summon <entity> <pos> <nbt>` form.
    pub(in crate::game) fn summon_with_nbt(
        &mut self,
        kind: &str,
        pos: [f64; 3],
        nbt: &str,
    ) -> R<Option<Target>> {
        let (mut tag, _) = parse_snbt(nbt).map_err(|e| err("argument.nbt.invalid", &[&e]))?;
        if !matches!(tag, Tag::Compound(_)) {
            return Err(err("argument.nbt.expected.compound", &[]));
        }
        // SummonCommand.createEntity with an NBT argument passes
        // `finalize = false`: the mob is what its compound says, and the
        // `horizon:finalize` marker worldgen leaves on a structure mob is not
        // the player's to set.
        strip_finalize_tree(&mut tag);
        set_key(&mut tag, "id", Tag::String(kind.into()));
        // Turtle.readAdditionalSaveData chooses its home before SummonCommand's snapTo.
        // Preserve that load-stage default while the shared loader receives the final Pos.
        if kind == "minecraft:turtle"
            && !matches!(tag.get("home_pos"), Some(Tag::IntArray(home)) if home.len() == 3)
        {
            let home = entity_nbt::position(&tag).map_or([0; 3], |p| {
                [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]
            });
            set_key(&mut tag, "home_pos", Tag::IntArray(home.to_vec()));
        }
        place_summoned(&mut tag, pos);
        // ServerLevel.tryAddFreshEntityWithPassengers: a UUID of the tree that
        // an entity in the level holds refuses the whole tree, before any of
        // it joins (players are not looked at here).
        if self.summon_tree_uuids(&tag).into_iter().any(|u| {
            self.target_with_uuid(u)
                .is_some_and(|t| t != Target::Player)
        }) {
            return Err(err("commands.summon.failed.uuid", &[]));
        }
        let canceled = self.persist.joins_canceled;
        let uuid = entity_nbt::uuid(&tag);
        let target = self.spawn_entity_from_nbt(tag);
        // The UUID a summoned mob was given is what `target_with_uuid` looks up; `Entity`'s constructor draws one for the others.
        if let Some(Target::Mob(id)) = target {
            let uuid = uuid.unwrap_or_else(|| self.uuid_for(id));
            if let Some(e) = self.ecs.entity_of(id) {
                self.ecs.set_uuid(e, uuid);
            }
        }
        // ItemEntity's constructor takes an id, then addEntity discards the empty stack.
        if target.is_none() && kind == "minecraft:item" {
            self.next_entity += 1;
        }
        // A mod kept the entity out: tryAddFreshEntityWithPassengers is false.
        if self.persist.joins_canceled != canceled
            && target.is_none_or(|t| self.target_position(t).is_none())
        {
            return Err(err("commands.summon.failed.uuid", &[]));
        }
        Ok(target)
    }
}

/// `horizon:finalize` off `tag` and off the riders under its `Passengers`:
/// only worldgen leaves it, for a structure mob the game finishes when its
/// chunk loads (`Game::load_mob`), never for a player to ask for. Taken off the
/// compounds a player's NBT arrives in: `/summon`'s argument, `/data`'s and the
/// `minecraft:entity_data` component's (`Game::apply_entity_nbt`), and
/// `native_summon`, which takes `/summon`'s argument.
pub(in crate::game) fn strip_finalize_tree(tag: &mut Tag) {
    let Tag::Compound(c) = tag else { return };
    c.remove(&"horizon:finalize".into());
    if let Some(Tag::List(passengers)) = c.get_mut(&"Passengers".into()) {
        for passenger in passengers {
            strip_finalize_tree(passenger);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::geometry::Vec3;


}

#[cfg(test)]
#[path = "data/orb_uuid_tests.rs"]
mod orb_uuid_tests;

#[cfg(test)]
mod wolf_live_anger_tests {
    use super::Target;
    use crate::game::ecs::test_support::ecs_game_with;
    use horizon_core::nbt::Tag;

    #[test]
    fn a_merge_keeps_the_anger_end_time_as_written_for_bee_and_wolf() {
        // Bee's independently modeled end is the first control, before the Wolf regression.
        for kind in ["bee", "wolf"] {
            let (mut game, id) = ecs_game_with(&format!("minecraft:{kind}"), [10.5, 200.0, 10.5]);
            game.world.game_time = 12_345;
            for end in [12_395, 12_395, 12_420, -1] {
                let mut tag = Tag::Compound(game.ecs_mob_tree(id).unwrap());
                let Tag::Compound(fields) = &mut tag else {
                    panic!("mob compound")
                };
                fields.insert("anger_end_time".into(), Tag::Long(end));
                game.apply_entity_nbt(Target::Mob(id), tag).unwrap();
                let saved = Tag::Compound(game.ecs_mob_tree(id).unwrap());
                assert_eq!(saved.get("anger_end_time"), Some(&Tag::Long(end)), "{kind} end={end}");
                assert_eq!(game.world.game_time, 12_345);
            }
        }
    }
}

#[cfg(test)]
mod loot_table_tests {
    use crate::game::{Audio, Game, Options, Progress, Settings, load_resources};
    use myriade_engine::{
        chunk::Chunk,
        containers::BlockEntity,
        items::ItemStack,
        save::{self, ChunkExtras, PendingContainerLoot},
    };
    use horizon_core::nbt::Tag;
    use std::sync::Arc;

    const TABLE: &str = "minecraft:chests/simple_dungeon";

    /// The chest as a chunk save writes it, and its slots when that save loads.
    fn saved_and_reloaded(game: &Game<'_>, pos: (i32, i32, i32)) -> (Tag, Vec<Option<ItemStack>>) {
        let live = game.world.chunks.get(&(0, 0)).unwrap().clone();
        let chunk = game.with_entities([&live]).remove(0);
        let registry = game.pack.assets.registry;
        let encoded = save::encode(&chunk, registry, game.res.biomes());
        let Some(Tag::List(list)) = encoded.tag.get("block_entities") else {
            panic!("no block entities");
        };
        let saved = list
            .iter()
            .find(|t| save::number(t.get("x")) == Some(i64::from(pos.0)))
            .unwrap()
            .clone();
        let reloaded = save::decode(&encoded, registry, game.res.biomes()).unwrap();
        let (_, BlockEntity::Chest(slots)) = reloaded
            .block_entities
            .iter()
            .find(|(p, _)| *p == pos)
            .unwrap()
        else {
            panic!("chest");
        };
        (saved, slots.clone())
    }

    /// RandomizableContainer.tryLoadLootTable: a tag without a table loads an
    /// empty chest that stays so, and one with a table keeps it.
    #[test]
    #[ignore = "daily"]
    fn data_commands_on_an_unopened_loaded_chest_survive_a_save_and_reload() {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);

        // Two unopened chests, with one owner for their pending table tags.
        let removed = (1, 64, 2);
        let merged = (2, 64, 2);
        let chest = game
            .pack
            .assets
            .registry
            .default_state("minecraft:chest")
            .unwrap();
        let mut chunk = Chunk::empty(0, 0);
        let mut extras = ChunkExtras::default();
        for pos in [removed, merged] {
            let slots = vec![None; 27];

            chunk.set(pos.0, pos.1, pos.2, chest.0);
            extras.pending_loot.insert(
                pos,
                PendingContainerLoot {
                    table: Tag::String(TABLE.into()),
                    seed: Some(Tag::Long(42)),
                },
            );
            game.world
                .block_entities
                .insert(pos, BlockEntity::Chest(slots));
        }
        chunk.saved = Some(Arc::new(extras));
        game.world.chunks.insert((0, 0), Arc::new(chunk));

        for field in ["LootTable", "LootTableSeed"] {
            assert!(game.run_command(&format!("data get block 2 64 2 {field}")));
        }
        let raw = game.block_entity_nbt([2, 64, 2]).unwrap();
        assert_eq!(raw.get("LootTable"), Some(&Tag::String(TABLE.into())));
        assert_eq!(raw.get("LootTableSeed"), Some(&Tag::Long(42)));
        assert!(
            game.world
                .block_entities
                .get(&merged)
                .unwrap()
                .slots()
                .iter()
                .all(Option::is_none)
        );
        assert!(game.run_command("data remove block 1 64 2 LootTable"));
        let (saved, slots) = saved_and_reloaded(&game, removed);
        assert!(
            saved.get("LootTable").is_none(),
            "the emptied chest rolls again on load: {saved:?}"
        );
        assert!(saved.get("LootTableSeed").is_none());
        assert!(slots.iter().all(Option::is_none));

        assert!(game.run_command("data merge block 2 64 2 {LootTableSeed:7L}"));
        let (saved, _) = saved_and_reloaded(&game, merged);
        assert_eq!(save::text(saved.get("LootTable")).as_deref(), Some(TABLE));
        assert_eq!(save::number(saved.get("LootTableSeed")), Some(7));
        assert!(saved.get("Items").is_none());
    }
}

#[cfg(test)]
mod compute_tests;




#[cfg(test)]
mod arrow_owner_tests {
    use super::Target;
    use crate::game::{Audio, Game, Options, Progress, Settings, load_resources};
    use horizon_core::nbt::Tag;

}

#[cfg(test)]
mod modern_literal_tests {
    use super::*;
    use crate::game::commands::execute::ExecState;
    use crate::game::{Audio, Options, Progress, Settings, load_resources};

    #[test]
    #[ignore = "daily"]
    fn admitted_modern_literals_reach_actual_storage_as_typed_tags() {
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            horizon_assets::resource_source::ResourceRoot::directory(root.clone()),
            Vec::new(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            crate::game::execution_role::ExecutionRole::DedicatedServer,
            &Progress::new(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::DedicatedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            options,
            settings,
        )
        .unwrap();
        game.build_registries();
        game.set_allow_commands(true);
        let run = |g: &mut Game<'_>, line: &str| {
            g.exec = ExecState::default();
            let source = g.player_source();
            g.execute_line(line, source)
        };
        let cases = [
            ("hex", "0x10", "16", Tag::Int(16)),
            ("underscore", "1_000", "1000", Tag::Int(1000)),
            ("unicode4", r#""\u0041""#, r#""A""#, Tag::String("A".into())),
            ("hex2", r#""\x41""#, r#""A""#, Tag::String("A".into())),
            (
                "unicode8",
                r#""\U00000041""#,
                r#""A""#,
                Tag::String("A".into()),
            ),
            (
                "trailing",
                "{n:3,}",
                "{n:3}",
                Tag::compound([("n", Tag::Int(3))]),
            ),
            ("plain", "plain", r#""plain""#, Tag::String("plain".into())),
        ];
        let clock = game.world.game_time;
        let mut differences = Vec::new();
        for (name, modern, plain, expected) in &cases {
            for route in ["merge", "modify"] {
                let target = format!("h189:{route}_{name}");
                let reference = format!("h189:ref_{route}_{name}");
                let seed = ["data merge storage ", &target, r#" {v:"seed"}"#].concat();
                assert_eq!(run(&mut game, &seed).unwrap(), 1);
                assert_eq!(
                    game.score.storage.get(&target).get("v"),
                    Some(&Tag::String("seed".into()))
                );
                let canonical = ["data merge storage ", &reference, " {v:", plain, "}"].concat();
                assert_eq!(run(&mut game, &canonical).unwrap(), 1);
                assert_eq!(game.score.storage.get(&reference).get("v"), Some(expected));
                let command = if route == "merge" {
                    ["data merge storage ", &target, " {v:", modern, "}"].concat()
                } else {
                    ["data modify storage ", &target, " v set value ", modern].concat()
                };
                let dispatcher = myriade_engine::command::vanilla::game_dispatcher();
                assert!(
                    dispatcher
                        .parse(&command, &game.session.registries)
                        .executable()
                        .is_ok(),
                    "admission: {route}/{name}"
                );
                let result = run(&mut game, &command);
                let actual = game.score.storage.get(&target).get("v").cloned();
                if result.as_ref().ok() != Some(&1) || actual.as_ref() != Some(expected) {
                    differences.push(format!("{route}/{name}: result={result:?}, stored={actual:?}, expected={expected:?}"));
                }
                // Copy actual canonical stored data back: equality is a real
                // no-change failure, independent of a formatter or parser stub.
                let copy = [
                    "data modify storage ",
                    &target,
                    " v set from storage ",
                    &reference,
                    " v",
                ]
                .concat();
                if run(&mut game, &copy).is_ok() {
                    differences.push(format!(
                        "{route}/{name}: canonical copy changed the stored Tag"
                    ));
                }
                assert_eq!(game.score.storage.get(&target).get("v"), Some(expected));
            }
        }
        assert_eq!(game.world.game_time, clock);
        assert!(differences.is_empty(), "{}", differences.join("\n"));
    }
}

#[cfg(test)]
mod bell_command_identity_tests {
    use super::*;
    use myriade_engine::chunk::Chunk;
    use horizon_core::{geometry::Vec3, position::ChunkPos};
    use std::sync::Arc;

}

#[cfg(test)]
mod vehicle_container_tests {
    use super::*;

}
