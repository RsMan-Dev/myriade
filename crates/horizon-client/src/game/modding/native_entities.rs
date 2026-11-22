//! Entities for native server mods (docs/modding/src/how-to/native-mods.md, "Entities"): spawn
//! any entity from its compound as `/summon` does, and move, show or remove
//! the entities Horizon keeps without simulating them (displays, markers,
//! interactions); `native_display_refs.rs` poses displays. They act where
//! the world is simulated, in the level in place, which each names by its
//! `dimension` (`minecraft:overworld`) as `native_set_block` does.
use super::{
    Game,
    commands::{Target, place_summoned, strip_finalize_tree},
};
use horizon_core::geometry::Vec3;
use horizon_core::nbt::{Compound, Tag};

/// Whether a position is one `/summon` takes.
fn position_ok(pos: [f64; 3]) -> bool {
    pos.iter().all(|v| v.is_finite())
        && pos[0].abs() < 30_000_000.0
        && pos[2].abs() < 30_000_000.0
        && (-20_000_000.0..20_000_000.0).contains(&pos[1])
}


/// The entity id a summoned stack's root is known by.
pub(super) fn target_id(t: Target) -> Option<u32> {
    match t {
        Target::Mob(id)
        | Target::Item(id)
        | Target::Orb(id)
        | Target::Projectile(id)
        | Target::Stand(id)
        | Target::Frame(id)
        | Target::Painting(id)
        | Target::Boat(id)
        | Target::Cart(id)
        | Target::Other(id)
        | Target::Fangs(id)
        | Target::Crystal(id)
        | Target::Dragon(id)
        | Target::DragonPart(id)
        | Target::Tnt(id)
        | Target::Cushion(id)
        | Target::Falling(id)
        | Target::Rocket(id)
        | Target::Trident(id)
        | Target::Spawner(id)
        | Target::Cloud(id)
        | Target::Hook(id)
        | Target::Knot(id)
        | Target::General(id) => Some(id),
        Target::Player | Target::Guest(_) => None,
    }
}

#[allow(dead_code)]
impl Game<'_> {
    /// Whether this game simulates the level `dimension`.
    fn native_simulates_level(&self, dimension: &str) -> bool {
        (self.serving.is_some() || self.options.dedicated.is_some()) && dimension == self.dim.id()
    }
    /// Spawns an entity of `kind` (`minecraft:item_display`) at `pos` in the
    /// level in place, `dimension`, from `nbt` (its compound, as `/summon
    /// <kind> <pos> <nbt>` takes it; passengers included): its id. None on a
    /// client's copy of the world, for another dimension, a position outside
    /// `/summon`'s bounds, a chunk that isn't loaded, a kind Horizon can't
    /// load, one a mod kept out (`native_entity_join_level`), or with the entity ids exhausted.
    /// Inside a transaction the id comes back at once: a join a mod cancels at the root commit
    /// leaves its handle stale.
    pub(super) fn summon(
        &mut self,
        dimension: &str,
        kind: &str,
        pos: [f64; 3],
        nbt: Compound,
    ) -> Option<u32> {
        if self.entity_ids_exhausted() {
            return None;
        }
        if !self.native_simulates_level(dimension) || !position_ok(pos) {
            return None;
        }
        let chunk = ((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4);
        if !self.world.chunks.contains_key(&chunk) {
            return None;
        }
        let mut tag = nbt;
        tag.insert("id".into(), Tag::String(kind.into()));
        // As `/summon <kind> <pos> <nbt>` takes it: with an NBT argument vanilla
        // does not finalize the mob (`SummonCommand.createEntity`), so a
        // `horizon:finalize` in it is not the caller's to set.
        let mut tag = Tag::Compound(tag);
        strip_finalize_tree(&mut tag);
        // As `/summon` does: every entity of the stack, passengers included,
        // stands at `pos`.
        place_summoned(&mut tag, pos);
        let Tag::Compound(c) = &tag else {
            return None;
        };
        let e = self.with_ecs(|ecs, mw| ecs.load_entity(mw, c))?;
        let id = self.ecs_id_of(e);
        // The join hook of a summoned mob, as `native_mobs_joined` gives it to the remote ones.
        if let (Some(kind), Some(at)) = (self.ecs.kind(id), self.ecs.position(id)) {
            self.native_mob_spawned(self.dim.id(), kind, at, false);
        }
        Some(id)
    }
}

/// The entities the walk does not hold: they are read from the ECS on demand.
#[derive(Clone, Copy)]
enum Apart {
    Crystal(u32),
    Cushion(u32),
    Painting(u32),
    Frame(u32),
}

/// An end crystal the ECS holds: its id, the join it came in with and its feet.
struct EcsCrystal {
    id: u32,
    join: horizon_client::join_seq::JoinSeq,
    position: Vec3,
}

impl Game<'_> {
    /// Every end crystal the ECS holds, read on demand (none is kept in a list).
    fn ecs_crystals(&self) -> Vec<EcsCrystal> {
        self.ecs
            .crystal_views()
            .into_iter()
            .map(|(id, c)| {
                let p = c.position;
                let join = self
                    .ecs
                    .entity_of(id)
                    .and_then(|e| self.ecs.ecs().and_then(|w| w.world().get::<horizon_ecs_proto::hanging::JoinedAt>(e)))
                    .map_or_else(Default::default, |j| j.0);
                EcsCrystal { id, join, position: Vec3::new(p[0], p[1], p[2]) }
            })
            .collect()
    }
}

/// One entity a mod is told of: in the walk, or apart from it.
#[derive(Clone, Copy)]
enum Slot {
    Walk(super::entity::EntityHandle),
    Apart(Apart),
}

/// Entities joining and leaving the level (`native_entity_join_level`,
/// `native_entity_leave_level`), for every kind in the entity walk and the
/// five kept apart from it. Kept out of `native_hooks.rs`, whose lines may not hold macros.
impl Game<'_> {
    /// Whether an entity may have joined since the last announcement. A join
    /// in another level may cause one harmless walk of this level.
    fn joins_pending(&self) -> bool {
        horizon_client::join_seq::JoinSeq::latest() > self.persist.joined_seq
    }
    /// Tells `native_entity_join_level` once for every entity added since the
    /// last call, of any kind, in insertion order. A mod that cancels takes
    /// the entity back out here,
    /// before it ticks again, and `/summon` of it fails (`joins_canceled`).
    /// `loaded_from_disk` is for the entities that came with a chunk. Returns
    /// whether `watch` (an id, or 0) is still in the level. Walks the lists
    /// with static dispatch and allocates nothing but the kind name of an
    /// entity it announces; nothing runs when no mod is loaded, on a client
    /// of a server (which tells nothing), or when nothing is new.
    /// Inside a transaction open in this level, construction is still told now, and the join
    /// at its root commit (`transactions/entities.rs`), with this call's `loaded_from_disk`, in
    /// insertion order and before the transaction's removals take effect; the entity is kept
    /// until then. The same holds for a call a listener makes while that commit is told: its
    /// joins are told after the batch being told. A `through` is the commit's own flush of one
    /// batch, never held.
    pub(super) fn native_joins_flush(&mut self, loaded_from_disk: bool, watch: u32) -> bool {
        self.flush_joins(loaded_from_disk, watch, &[], None)
    }
    /// [`Self::native_joins_flush`], never announcing the entities `removed` names and, with a
    /// `through`, none that joined after it: a root commit's, for the entities its transaction
    /// added and removed, each held flush keeping its own `loaded_from_disk`.
    pub(in crate::game) fn flush_joins(
        &mut self,
        loaded_from_disk: bool,
        watch: u32,
        removed: &[u32],
        through: Option<horizon_client::join_seq::JoinSeq>,
    ) -> bool {
        if mod_api::__private::mods().is_empty() || !self.joins_pending() {
            return true;
        }
        // Read before this call's own announcement is held: it reaches the entities to construct.
        let held_before = self.held_joins_through();
        if through.is_none()
            && self.hold_announcement(super::transactions::Announcement::Joins {
                loaded_from_disk,
                through: horizon_client::join_seq::JoinSeq::latest(),
            })
        {
            // What an earlier held flush of this level told is constructed already; with a toss
            // still asked it may have stopped short, so then everything since the mark is looked at.
            let mut mark = self.persist.joined_seq;
            if self.native_lifecycle.held_joins.is_empty() {
                mark = mark.max(held_before);
            }
            while let Some((slot, id, seq)) = self.next_unannounced(mark, &[], None) {
                mark = seq;
                let kind = self.slot_kind_and_position(slot).0;
                self.construct_joining(id, &kind);
            }
            return true;
        }
        let latest = horizon_client::join_seq::JoinSeq::latest();
        let observed = through.map_or(latest, |through| through.min(latest));
        let mut kept = true;
        while let Some((slot, id, seq)) =
            self.next_unannounced(self.persist.joined_seq, removed, through)
        {
            self.persist.joined_seq = seq;
            let (kind, at) = self.slot_kind_and_position(slot);
            let mut canceled = false;
            let dimension = self.dim.id();
            self.construct_joining(id, &kind);
            self.native_entity_join_level(
                dimension,
                id,
                &kind,
                [at.x, at.y, at.z],
                loaded_from_disk,
                &mut canceled,
            );
            let spawn = self
                .native_lifecycle
                .pending_item_spawns
                .iter()
                .position(|entry| entry.0 == id)
                .map(|index| self.native_lifecycle.pending_item_spawns.remove(index));
            if canceled {
                self.persist.joins_canceled += 1;
                self.forget_entity(id);
                // Handles are indexes: the entity is taken out before any
                // other change to its list.
                self.discard_slot(slot);
                kept &= id != watch;
            } else {
                self.remember_join(id, kind.into_owned());
                // `addEntity` files it under its section, untold.
                self.entity_section_joined(id, [at.x, at.y, at.z]);
                if let Some((id, item, count, position)) = spawn {
                    self.native_item_spawn(id, &item, count, position);
                }
            }
        }
        // A held toss is not past yet: the mark stays before it.
        if self.native_lifecycle.held_joins.is_empty() {
            self.persist.joined_seq = self.persist.joined_seq.max(observed);
        }
        kept
    }

    /// `Entity.<init>` comes before `addFreshEntity`: an entity that started on a path that
    /// doesn't tell it (`entity_initialization.rs`) is told here.
    fn construct_joining(&mut self, id: u32, kind: &str) {
        self.initialize_entity(mod_api::Holder::Entity(id), kind);
        // Constructor writes were canonical but unbound before insertion.
        // The real lifetime is now resident, before join listeners query it.
        if self
            .entity_attachments
            .get(&id)
            .is_some_and(|map| map.has_index_entries())
            && let Err(error) = self.bind_resident_entity_attachments(id)
        {
            log::warn!("constructor attachment binding failed for entity {id}: {error:?}");
        }
    }

    /// The entity inserted earliest after `mark` (and, with a `through`, no later than it), but
    /// the `skipped`.
    fn next_unannounced(
        &self,
        mark: horizon_client::join_seq::JoinSeq,
        skipped: &[u32],
        through: Option<horizon_client::join_seq::JoinSeq>,
    ) -> Option<(Slot, u32, horizon_client::join_seq::JoinSeq)> {
        use super::entity::Entity;
        use horizon_client::join_seq::{JoinSeq, Joined};
        let mut best: Option<(Slot, u32, JoinSeq)> = None;
        let mut offer = |slot: Slot, id: u32, seq: JoinSeq| {
            if skipped.contains(&id) || self.persist.removals.iter().chain(self.persist.destroying_vehicles.iter()).any(|target| {
                matches!(*target, super::commands::Target::Boat(n) | super::commands::Target::Cart(n) if n == id)
            }) {
                return;
            }
            if seq > mark
                && through.is_none_or(|through| seq <= through)
                && best.is_none_or(|(_, _, previous)| seq < previous)
            {
                best = Some((slot, id, seq));
            }
        };
        for handle in self.entity_handles() {
            let (id, seq) = super::entity::with_entity!(self, handle, e => (e.id(), e.join_seq()));
            offer(Slot::Walk(handle), id, seq);
        }
        for c in self.ecs_crystals() {
            offer(Slot::Apart(Apart::Crystal(c.id)), c.id, c.join);
        }
        for c in self.cushion_list() {
            offer(Slot::Apart(Apart::Cushion(c.id)), c.id, c.join);
        }
        for p in self.ecs_paintings() {
            offer(Slot::Apart(Apart::Painting(p.id)), p.id, p.join);
        }
        for f in self.ecs_frames() {
            offer(Slot::Apart(Apart::Frame(f.id)), f.id, f.join);
        }
        // A toss still being asked (`item_toss`) ends the flush: it and what joined after wait.
        best.filter(|&(_, id, _)| !self.native_lifecycle.held_joins.contains(&id))
    }

    fn remember_join(&mut self, id: u32, kind: String) {
        let index = self
            .persist
            .live
            .binary_search_by_key(&id, |entry| entry.0)
            .unwrap_or_else(|i| i);
        self.persist.live.insert(index, (id, kind, true));
    }
    /// The type id and position a mod is told of.
    fn slot_kind_and_position(&self, slot: Slot) -> (std::borrow::Cow<'static, str>, Vec3) {
        use super::entity::Entity;
        let block = |p: [i32; 3]| {
            Vec3::new(
                f64::from(p[0]) + 0.5,
                f64::from(p[1]) + 0.5,
                f64::from(p[2]) + 0.5,
            )
        };
        match slot {
            Slot::Walk(h) => {
                super::entity::with_entity!(self, h, e => (e.native_type(), e.position()))
            }
            Slot::Apart(Apart::Crystal(id)) => {
                ("minecraft:end_crystal".into(), self.ecs_crystals().into_iter().find(|c| c.id == id).map_or(Vec3::ZERO, |c| c.position))
            }
            Slot::Apart(Apart::Cushion(id)) => {
                ("minecraft:cushion".into(), self.cushion_saved(id).map_or(Vec3::ZERO, |c| c.position))
            }
            Slot::Apart(Apart::Painting(id)) => {
                let pos = self.ecs_paintings().into_iter().find(|p| p.id == id).map_or([0; 3], |p| p.pos);
                ("minecraft:painting".into(), block(pos))
            }
            Slot::Apart(Apart::Frame(id)) => {
                let (glow, pos) = self.ecs_frames().into_iter().find(|f| f.id == id).map_or((false, [0; 3]), |f| (f.glow, f.pos));
                let kind = if glow {
                    "minecraft:glow_item_frame"
                } else {
                    "minecraft:item_frame"
                };
                (kind.into(), block(pos))
            }
        }
    }
    /// Takes the entity out of the level, before it ever ticked.
    fn discard_slot(&mut self, slot: Slot) {
        match slot {
            Slot::Walk(h) => self.discard_entity(h),
            Slot::Apart(Apart::Crystal(id) | Apart::Cushion(id)) => {
                if let Some(e) = self.ecs.entity_of(id) {
                    self.ecs.despawn(e);
                }
            }
            Slot::Apart(Apart::Painting(id) | Apart::Frame(id)) => {
                self.ecs_remove_hanging(id);
            }
        }
    }
    /// Entities that were moved, not added (a portal's arrival keeps being
    /// silent): everything now in the level counts as announced, and will
    /// be told when it leaves.
    pub(super) fn native_joins_skip(&mut self) {
        let observed = horizon_client::join_seq::JoinSeq::latest();
        if !mod_api::__private::mods().is_empty() {
            while let Some((slot, id, seq)) =
                self.next_unannounced(self.persist.joined_seq, &[], None)
            {
                self.persist.joined_seq = seq;
                let kind = self.slot_kind_and_position(slot).0.into_owned();
                self.remember_join(id, kind);
            }
        }
        self.persist.joined_seq = self.persist.joined_seq.max(observed);
    }
    /// An entity taken out of the level without leaving it (an item through
    /// an End gateway): forgotten, not told of.
    pub(super) fn native_leave_skip(&mut self, id: u32) {
        self.entity_section_forget(id);
        if let Ok(i) = self.persist.live.binary_search_by_key(&id, |l| l.0) {
            self.persist.live.remove(i);
        }
    }
    /// Entity `id` has left the level: told once, if mods were told it joined.
    pub(super) fn native_entity_left(&mut self, id: u32) {
        if false
            || self.hold_announcement(super::transactions::Announcement::Left(id))
        {
            return;
        }
        self.entity_left_now(id);
    }
    /// [`Self::native_entity_left`] told now, though a transaction or a commit holds the rest:
    /// the root commit's own telling.
    pub(in crate::game) fn entity_left_now(&mut self, id: u32) {
        self.entity_section_forget(id);
        if let Ok(i) = self.persist.live.binary_search_by_key(&id, |l| l.0) {
            let (id, kind, _) = self.persist.live.remove(i);
            let dimension = self.dim.id();
            self.native_entity_leave_level(dimension, id, &kind);
        }
    }
    /// Tells `native_entity_leave_level` for every announced entity that is
    /// no longer in any list (died, despawned, picked up, burnt, blown up, gone
    /// through a portal, unloaded), lowest id first. Marks the entities still
    /// there in the registry and tells the rest: no allocation, and nothing
    /// runs when no mod is loaded or none was announced.
    pub(super) fn native_leaves_flush(&mut self) {
        self.native_removals_flush();
        if self.persist.live.is_empty()

            || self.hold_announcement(super::transactions::Announcement::Leaves)
        {
            return;
        }
        self.leaves_now();
    }
    /// [`Self::native_leaves_flush`] past its guards and told now, though a transaction or a
    /// commit holds the rest: the root commit's own telling.
    pub(in crate::game) fn leaves_now(&mut self) {
        use super::entity::Entity;
        let hanging: Vec<u32> = self.ecs_paintings().iter().map(|p| p.id).chain(self.ecs_frames().iter().map(|f| f.id)).collect();
        for l in &mut self.persist.live {
            l.2 = false;
        }
        let cushions = self.cushion_ids();
        let crystal_ids: Vec<u32> = self.ecs_crystals().into_iter().map(|c| c.id).collect();
        let live = &mut self.persist.live;
        let mut seen = |id: u32| {
            if let Ok(i) = live.binary_search_by_key(&id, |l| l.0) {
                live[i].2 = true;
            }
        };
        super::entity::for_each_entity!(super::entity::entity_lists_of!(self), e => {
            seen(e.id());
        });
        crystal_ids.into_iter().for_each(|id| seen(id));
        cushions.into_iter().for_each(|id| seen(id));
        hanging.into_iter().for_each(|id| seen(id));
        while let Some(i) = self.persist.live.iter().position(|l| !l.2) {
            let (id, kind, _) = self.persist.live.remove(i);
            self.entity_section_forget(id);
            let dimension = self.dim.id();
            self.native_entity_leave_level(dimension, id, &kind);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mod_api::ServerHost;

    #[test]
    #[ignore = "daily"]
    fn joins_follow_insertion_order_and_canceled_entities_never_leave() {
        use horizon_client::mobs::Mob;
        use horizon_core::geometry::Vec3;
        use mod_api::{self, EntityJoinLevelEvent, EntityLeaveLevelEvent, Flow, Hooks};
        use std::sync::{Arc, Mutex};

        type Joins = Arc<Mutex<Vec<(u32, String, bool)>>>;
        struct Observer {
            joins: Joins,
            leaves: Arc<Mutex<Vec<u32>>>,
        }
        #[mod_api::__subscriptions]
        impl Hooks for Observer {
            fn entity_join_level(
                &self,
                _: &mut dyn ServerHost,
                event: &mut EntityJoinLevelEvent<'_>,
            ) -> Flow {
                self.joins.lock().unwrap().push((
                    event.entity,
                    event.kind.into(),
                    event.loaded_from_disk,
                ));
                if event.kind == "minecraft:sheep" {
                    Flow::Cancel
                } else {
                    Flow::Continue
                }
            }
            fn entity_leave_level(
                &self,
                _: &mut dyn ServerHost,
                event: &mut EntityLeaveLevelEvent<'_>,
            ) {
                self.leaves.lock().unwrap().push(event.entity);
            }
        }
        let joins = Joins::default();
        let leaves = Arc::new(Mutex::new(Vec::new()));
        let mods: mod_api::Mods = vec![mod_api::ModEntry::new(
            "join-order-observer",
            Box::new(Observer {
                joins: joins.clone(),
                leaves: leaves.clone(),
            }),
        )]
        .into();
        mod_platform::testing::with_mods(mods, || {
            super::super::native_entity_refs::tests::with_game(|game| {
                game.native_joins_skip();
                let base = game.next_entity;
                let cow = game.res.mob_types().get("minecraft:cow").unwrap().clone();
                let sheep = game.res.mob_types().get("minecraft:sheep").unwrap().clone();
                let child = Mob::new(base + 1, cow.clone(), Vec3::ZERO, 0.0);
                game.next_entity += 2;
                game.remote_entities.push(Mob::new(base + 2, cow, Vec3::ZERO, 0.0));
                game.native_joins_flush(false, 0);
                game.remote_entities.push(child);
                game.native_joins_flush(false, 0);
                game.native_joins_flush(false, 0);
                assert_eq!(
                    *joins.lock().unwrap(),
                    vec![
                        (base + 2, "minecraft:cow".into(), false),
                        (base + 1, "minecraft:cow".into(), false)
                    ]
                );
                game.next_entity += 1;
                game.remote_entities.push(Mob::new(base + 3, sheep, Vec3::ZERO, 0.0));
                assert!(!game.native_joins_flush(true, base + 3));
                assert!(!game.remote_entities.iter().any(|mob| mob.id == base + 3));
                game.native_entity_left(base + 3);
                assert!(leaves.lock().unwrap().is_empty());
                assert_eq!(
                    joins.lock().unwrap().last(),
                    Some(&(base + 3, "minecraft:sheep".into(), true))
                );

                // A crystal, a cushion, a painting and a frame of the ECS (ids base + 4 to base + 7).
                game.ecs = game.test_ecs_level();
                assert_eq!(game.ecs_with(|ecs, mw| ecs.spawn_crystal(mw, [0.0; 3], true, &mut || base + 4)), base + 4);
                game.next_entity = base + 5;
                assert_eq!(crate::game::ecs::test_support::place_test_cushion(game, [0, 0, 0], base + 5), base + 5);
                assert_eq!(crate::game::ecs::test_support::hang_test_painting(game, [0, 0, 4], 3, "minecraft:kebab"), Some(base + 6));
                assert_eq!(crate::game::ecs::test_support::hang_test_frame(game, [0, 0, 0], 3, false), Some(base + 7));
                game.ecs_with(|ecs, mw| ecs.summon_dragon(mw, [0.0, 80.0, 0.0], 0.0, 0, horizon_ecs_proto::species::ender_dragon::START_PHASE, &mut || base + 8));
                game.next_entity = base + 8;
                game.native_joins_flush(true, 0);
                game.native_joins_flush(true, 0);
                let seen = joins.lock().unwrap();
                assert_eq!(
                    &seen[3..],
                    &[
                        (base + 4, "minecraft:end_crystal".into(), true),
                        (base + 5, "minecraft:cushion".into(), true),
                        (base + 6, "minecraft:painting".into(), true),
                        (base + 7, "minecraft:item_frame".into(), true),
                        (base + 8, "minecraft:ender_dragon".into(), true)
                    ]
                );
                drop(seen);
                game.native_joins_flush(false, 0);
                game.native_joins_flush(false, 0);
                assert_eq!(joins.lock().unwrap().len(), 8);
            });
        });
    }

    #[test]
    fn a_passenger_without_a_position_stands_where_the_stack_is_summoned() {
        let mut rider = Compound::new();
        rider.insert("id".into(), Tag::String("minecraft:pig".into()));
        let mut root = Compound::new();
        root.insert("id".into(), Tag::String("minecraft:pig".into()));
        root.insert("Passengers".into(), Tag::List(vec![Tag::Compound(rider)]));
        let mut tag = Tag::Compound(root);
        place_summoned(&mut tag, [1.5, 70.0, -2.5]);
        let pos = Tag::List(
            [1.5, 70.0, -2.5]
                .map(|v: f64| Tag::Double(v.to_bits()))
                .to_vec(),
        );
        let Tag::Compound(root) = &tag else { panic!() };
        assert_eq!(root.get(&"Pos".into()), Some(&pos));
        let Some(Tag::List(riders)) = root.get(&"Passengers".into()) else {
            panic!("no passengers");
        };
        let Tag::Compound(rider) = &riders[0] else {
            panic!()
        };
        assert_eq!(rider.get(&"Pos".into()), Some(&pos));
    }

    #[test]
    fn summon_positions_are_summons() {
        assert!(position_ok([0.5, 64.0, 0.5]));
        assert!(!position_ok([3.0e7, 64.0, 0.5]));
        assert!(!position_ok([0.5, f64::NAN, 0.5]));
    }
}
