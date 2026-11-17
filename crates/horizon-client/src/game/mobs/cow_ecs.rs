//! Prototype ECS for cows (`suivi/ecs-prototype.md`). A `bevy_ecs` world holds one entity per cow with
//! two components: the stamp of the blocks around it, and the cache of its pure world queries
//! (`mobs/cow_memo.rs`). Everything else about the cow stays in `Mob` and is ticked by the same code
//! in the same order. The cache is emptied when `StampC` changes (change detection), and lent to the
//! cow's tick through `cow_memo::enter`.
//!
//! Modes (`HORIZON_ECS`, or the field): 0 off; 1 same work (the entities, the stamp and the cache are
//! kept and asked, but no answer is reused); 2 reuse the answers.
use bevy_ecs::{change_detection::DetectChangesMut, entity::Entity, world::World};
use horizon_client::mobs::{Mob, Species, cow_memo::QueryCache};
use horizon_client_render::world::RegionStamp;

#[derive(bevy_ecs::component::Component, PartialEq, Eq, Clone, Copy)]
struct StampC(Option<RegionStamp>);

pub struct CowEcs {
    pub mode: u8,
    world: World,
    ids: horizon_client::fast_hash::FxHashMap<u32, Entity>,
    entered: Option<Entity>,
    entered_in_mob: bool,
    passes: u32,
}

impl Default for CowEcs {
    fn default() -> Self {
        let mode = std::env::var("HORIZON_ECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Self::new(mode)
    }
}

impl CowEcs {
    pub fn new(mode: u8) -> Self {
        Self {
            mode,
            world: World::new(),
            ids: Default::default(),
            entered: None,
            entered_in_mob: false,
            passes: 0,
        }
    }

    /// Queries asked and answered from the cache, over every cow.
    pub fn counts(&mut self) -> (u64, u64) {
        let mut q = self.world.query::<&QueryCache>();
        q.iter(&self.world).fold((0, 0), |(a, h), c| (a + c.asks, h + c.hits))
    }

    /// The start of a mob pass: the world's change tick moves on; dead cows' entities go now and then.
    pub fn begin_pass(&mut self, ids: impl Iterator<Item = u32>) {
        if self.mode == 0 {
            return;
        }
        self.world.increment_change_tick();
        self.passes += 1;
        if self.passes % 256 == 0 {
            let live: horizon_client::fast_hash::FxHashSet<u32> = ids.collect();
            let gone: Vec<u32> = self.ids.keys().copied().filter(|id| !live.contains(id)).collect();
            for id in gone {
                if let Some(e) = self.ids.remove(&id) {
                    self.world.despawn(e);
                }
            }
        }
    }

    /// Before cow `m`'s tick: its entity (made on first sight), the stamp of the blocks around its
    /// box refreshed, and its cache lent to this thread.
    pub fn enter(&mut self, m: &mut Mob, world: &horizon_client_render::world::World) {
        if self.mode == 0 || m.species() != Species::Cow || !world.moving_pistons.is_empty() || !world.shulkers.is_empty() {
            return;
        }
        if self.mode == 3 {
            // The same cache and stamp kept in the mob itself: no entity, no change detection.
            let b = m.bounds();
            let (lo, hi) = (b.min_position(), b.max_position());
            let lo = [lo.x.floor() as i32 - 2, lo.y.floor() as i32 - 2, lo.z.floor() as i32 - 2];
            let hi = [hi.x.floor() as i32 + 2, hi.y.floor() as i32 + 2, hi.z.floor() as i32 + 2];
            let Some(stamp) = world.region_stamp(lo, hi) else {
                return;
            };
            let mut held = m.memo.0.take().unwrap_or_default();
            if held.0 != Some(stamp) {
                held.0 = Some(stamp);
                held.1.clear();
            }
            horizon_client::mobs::cow_memo::enter(std::mem::take(&mut held.1), lo, hi, true);
            m.memo.0 = Some(held);
            self.entered_in_mob = true;
            return;
        }
        let e = *self.ids.entry(m.id).or_insert_with(|| {
            self.world
                .spawn((StampC(None), QueryCache::default()))
                .id()
        });
        let b = m.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        let lo = [lo.x.floor() as i32 - 2, lo.y.floor() as i32 - 2, lo.z.floor() as i32 - 2];
        let hi = [hi.x.floor() as i32 + 2, hi.y.floor() as i32 + 2, hi.z.floor() as i32 + 2];
        let Some(stamp) = world.region_stamp(lo, hi) else {
            return;
        };
        let mut entity = self.world.entity_mut(e);
        let changed = entity
            .get_mut::<StampC>()
            .is_some_and(|mut s| s.set_if_neq(StampC(Some(stamp))));
        let mut cache = entity
            .get_mut::<QueryCache>()
            .map(|mut c| std::mem::take(c.bypass_change_detection()))
            .unwrap_or_default();
        if changed {
            cache.clear();
        }
        horizon_client::mobs::cow_memo::enter(cache, lo, hi, self.mode >= 2);
        self.entered = Some(e);
    }

    /// After the tick: the cache goes back to its entity.
    pub fn leave(&mut self, m: &mut Mob) {
        if std::mem::take(&mut self.entered_in_mob) {
            if let (Some(cache), Some(held)) = (horizon_client::mobs::cow_memo::leave(), m.memo.0.as_mut()) {
                held.1 = cache;
            }
            return;
        }
        let Some(e) = self.entered.take() else {
            return;
        };
        if let Some(cache) = horizon_client::mobs::cow_memo::leave()
            && let Some(mut c) = self.world.entity_mut(e).get_mut::<QueryCache>()
        {
            *c.bypass_change_detection() = cache;
        }
    }
}

#[cfg(test)]
mod tests {
    use horizon_client::lan::{
        Body,
        entity::{Entities, Riding, States, Tracker, Viewer},
    };
    use horizon_core::{geometry::Vec3, position::BlockPos, random::RandomSource};

    fn game<'r>(res: &'r mut Option<std::sync::Arc<crate::game::Resources>>) -> crate::game::Game<'r> {
        let mut g = crate::game::game_tests::tests::server_game(res);
        g.build_registries();
        g.set_allow_commands(true);
        g.settings.simulation_distance = 4;
        g.settings.render_distance = 8;
        g.player = Some(
            horizon_client::motion::PlayerMotion::new(g.motion, Vec3::new(8.5, 260.0, 8.5)).unwrap(),
        );
        g.block_ticks.add_empty_chunk(BlockPos::new(0, 0, 0).chunk()).unwrap();
        assert!(g.run_command("forceload add 0 0"));
        assert!(g.run_command("fill 0 199 0 15 199 15 stone"));
        g.next_entity = 100;
        g
    }

    /// Everything a mob is, as text (the `Debug` of the whole `Mob`).
    fn mobs_of(g: &crate::game::Game<'_>) -> Vec<String> {
        g.mobs.iter().map(|m| canonical(format!("{m:#?}"))).collect()
    }

    /// The pretty `Debug` text with what is not state taken out: `JoinSeq(n)` counts the process's
    /// joins, and a `HashMap`'s entries (lines of `"key": value,` one after another) come in the
    /// order of each map's own hasher: those runs are sorted.
    fn canonical(text: String) -> String {
        let mut text = without_join_seq(text);
        // Process-wide counters: two games in one process do not share them.
        for key in ["section_seq: "] {
            let mut out = String::with_capacity(text.len());
            let mut rest = text.as_str();
            while let Some(at) = rest.find(key) {
                let (head, tail) = rest.split_at(at + key.len());
                out.push_str(head);
                out.push('_');
                rest = &tail[tail.bytes().take_while(u8::is_ascii_digit).count()..];
            }
            out.push_str(rest);
            text = out;
        }
        let is_entry = |l: &str| l.trim_start().starts_with('"') && l.trim_end().ends_with(',') && l.contains("\": ");
        let lines: Vec<&str> = text.lines().collect();
        let mut out = Vec::with_capacity(lines.len());
        let mut i = 0;
        while i < lines.len() {
            if is_entry(lines[i]) {
                let mut j = i;
                while j < lines.len() && is_entry(lines[j]) {
                    j += 1;
                }
                let mut run: Vec<&str> = lines[i..j].to_vec();
                run.sort_unstable();
                out.extend(run);
                i = j;
            } else {
                out.push(lines[i]);
                i += 1;
            }
        }
        out.join("\n")
    }

    /// `JoinSeq(n)` counts the process's joins (two games in one process do not share it).
    fn without_join_seq(text: String) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text.as_str();
        while let Some(at) = rest.find("JoinSeq(") {
            let (head, tail) = rest.split_at(at + "JoinSeq(".len());
            out.push_str(head);
            let digits = tail
                .bytes()
                .take_while(|b| b.is_ascii_digit() || b.is_ascii_whitespace() || *b == b',')
                .count();
            out.push('_');
            rest = &tail[digits..];
        }
        out.push_str(rest);
        out
    }

    /// Two games run the same script of cows (a herd walking about, a floor taken away, water,
    /// lava, a pair in love and its calf, a push, damage, the player coming close and going away,
    /// a kill), one with the ECS path off, one with `mode`: every mob (whole state, as text), the
    /// network states and the bytes two clients are sent are equal at every tick, and the level's
    /// random stream ends at the same draw. Returns (ticks compared, queries asked, answered from
    /// the cache) of the ECS game.
    fn scenario(mode: u8, ticks: u32) -> Result<(u32, u64, u64), String> {
        let (mut res_a, mut res_b) = (None, None);
        let mut games = [game(&mut res_a), game(&mut res_b)];
        games[0].cow_ecs = Some(Box::new(super::CowEcs::new(0)));
        games[1].cow_ecs = Some(Box::new(super::CowEcs::new(mode)));
        let _rx = [
            crate::game::lan::world_spawn_sync_tests::host_for(&mut games[0]),
            crate::game::lan::world_spawn_sync_tests::host_for(&mut games[1]),
        ];
        let mut states = [States::default(), States::default()];
        let mut levels = [Entities::default(), Entities::default()];
        let mut trackers = [[Tracker::default(), Tracker::default()], [Tracker::default(), Tracker::default()]];
        let chunks: horizon_client::fast_hash::FxHashSet<(i32, i32)> =
            (-4..4).flat_map(|x| (-4..4).map(move |z| (x, z))).collect();
        // The unseeded randoms (`Entity.random` of a spawn) are the first game's, replayed in the second.
        let both = |games: &mut [crate::game::Game<'_>; 2], f: &dyn Fn(&mut crate::game::Game<'_>)| {
            let ((), seeds) = horizon_client_render::random_source::record_seeds(|| f(&mut games[0]));
            horizon_client_render::random_source::replay_seeds(seeds.clone(), || f(&mut games[1]));
            assert!(seeds.complete(), "the second game drew the first's seeds");
        };
        both(&mut games, &|g| {
            for n in 0..40 {
                let (x, z) = (1 + (n % 8) * 2, 1 + (n / 8) * 3);
                assert!(g.run_command(&format!(
                    "summon cow {x}.5 200.0 {z}.5 {{UUID:[I;{},7,7,7],PersistenceRequired:1b}}",
                    n + 1
                )));
            }
        });
        let mut compared = 0;
        for t in 0..ticks {
            match t {
                150 => both(&mut games, &|g| {
                    assert!(g.run_command("setblock 3 199 1 air"));
                    assert!(g.run_command("setblock 7 199 4 air"));
                }),
                // Under (and in) the cows that are nearest the player: the blocks their queries read change.
                200 => both(&mut games, &|g| {
                    assert!(g.run_command("execute as @e[type=cow,limit=5,sort=nearest] at @s run setblock ~ ~-1 ~ air"));
                }),
                250 => both(&mut games, &|g| {
                    assert!(g.run_command("execute as @e[type=cow,limit=3,sort=furthest] at @s run setblock ~ ~-1 ~ stone"));
                }),
                300 => both(&mut games, &|g| {
                    assert!(g.run_command("setblock 11 200 7 water"));
                    assert!(g.run_command("execute as @e[type=cow,limit=2,sort=random] at @s run setblock ~ ~ ~ water"));
                }),
                350 => both(&mut games, &|g| {
                    assert!(g.run_command("execute as @e[type=cow,limit=2,sort=random] at @s run setblock ~ ~ ~ fire"));
                }),
                400 => both(&mut games, &|g| {
                    assert!(g.run_command("execute as @e[type=cow,limit=2,sort=random] at @s run setblock ~ ~ ~ powder_snow"));
                }),
                450 => both(&mut games, &|g| assert!(g.run_command("setblock 5 200 10 lava"))),
                600 => both(&mut games, &|g| {
                    assert!(g.run_command("execute as @e[type=cow,sort=nearest,limit=2] run data merge entity @s {InLove:600}"));
                }),
                700 => both(&mut games, &|g| {
                    g.player = Some(
                        horizon_client::motion::PlayerMotion::new(g.motion, Vec3::new(8.5, 200.0, 7.5)).unwrap(),
                    );
                }),
                900 => both(&mut games, &|g| {
                    assert!(g.run_command("execute as @e[type=cow,limit=3,sort=nearest] run data merge entity @s {Motion:[0.4d,0.3d,0.0d]}"));
                }),
                1000 => both(&mut games, &|g| {
                    g.player = Some(
                        horizon_client::motion::PlayerMotion::new(g.motion, Vec3::new(8.5, 260.0, 8.5)).unwrap(),
                    );
                    assert!(g.run_command("execute as @e[type=cow,limit=2,sort=furthest] run damage @s 4"));
                }),
                1200 => both(&mut games, &|g| assert!(g.run_command("kill @e[type=cow,limit=1,sort=random]"))),
                _ => {}
            }
            let mut failure = None;
            let [g0, g1] = &mut games;
            let (s0, s1) = states.split_at_mut(1);
            let mut one = |g: &mut crate::game::Game<'_>, st: &mut States| {
                g.world.game_time += 1;
                if let Err(e) = g.tick_blocks() {
                    failure = Some(e.to_string());
                }
                if let Err(e) = g.tick_creatures() {
                    failure = Some(e.to_string());
                }
                g.lan_entities(st, false, 0, None);
                g.lan_entity_uuids(st);
                // An experience orb's UUID is drawn from a source the seeds do not cover (the orb is not a cow).
                for e in st.as_mut_slice().iter_mut().filter(|e| e.kind == "minecraft:experience_orb") {
                    e.uuid = 0;
                }
                g.lan_syncs_looked();
            };
            let ((), seeds) = horizon_client_render::random_source::record_seeds(|| one(g0, &mut s0[0]));
            horizon_client_render::random_source::replay_seeds(seeds.clone(), || one(g1, &mut s1[0]));
            if let Some(e) = failure {
                return Err(e);
            }
            if !seeds.complete() {
                return Err(format!("tick {t}: the second game did not draw the first's seeds"));
            }
            let (ma, mb) = (mobs_of(&games[0]), mobs_of(&games[1]));
            if ma.len() != mb.len() {
                return Err(format!("tick {t}: {} mobs and {}", ma.len(), mb.len()));
            }
            if let Some(i) = (0..ma.len()).find(|&i| ma[i] != mb[i]) {
                let at = ma[i].bytes().zip(mb[i].bytes()).position(|(a, b)| a != b).unwrap_or(0);
                let lo = at.saturating_sub(60);
                return Err(format!(
                    "tick {t}: mob {i} differs near byte {at}: {:?} / {:?}",
                    &ma[i][lo..(at + 80).min(ma[i].len())],
                    &mb[i][lo..(at + 80).min(mb[i].len())]
                ));
            }
            if states[0].as_slice() != states[1].as_slice() {
                let (a, b) = (states[0].as_slice(), states[1].as_slice());
                let at = (0..a.len().min(b.len())).find(|&i| a[i] != b[i]);
                return Err(format!(
                    "tick {t}: the network states differ ({} and {}): {:?} / {:?}",
                    a.len(),
                    b.len(),
                    at.map(|i| &a[i]),
                    at.map(|i| &b[i])
                ));
            }
            let mut out: [(Vec<Body>, Vec<Body>); 2] = [(Vec::new(), Vec::new()), (Vec::new(), Vec::new())];
            for k in 0..2 {
                let refs: Vec<&horizon_client::lan::entity::EntityState> = states[k].as_slice().iter().collect();
                let [t0, t1] = &mut trackers[k];
                let (mut o0, mut o1): (Vec<Body>, Vec<Body>) = (Vec::new(), Vec::new());
                let mut viewers = [
                    Viewer {
                        entity_id: 1,
                        spectator: false,
                        permission: 0,
                        horizon: true,
                        moved: t == 0,
                        position: [8.0, 200.0, 8.0],
                        view_distance: 10,
                        chunks: &chunks,
                        sources: None,
                        tracker: t0,
                        out: &mut o0,
                        gone: None,
                    },
                    Viewer {
                        entity_id: 2,
                        spectator: false,
                        permission: 0,
                        horizon: true,
                        moved: t == 0,
                        position: [200.0, 200.0, 200.0],
                        view_distance: 10,
                        chunks: &chunks,
                        sources: None,
                        tracker: t1,
                        out: &mut o1,
                        gone: None,
                    },
                ];
                levels[k].tick(&refs, &Riding::default(), &mut viewers, |_| Some(100));
                out[k] = (o0, o1);
            }
            if out[0] != out[1] {
                return Err(format!("tick {t}: the bytes differ"));
            }
            compared += 1;
        }
        let draws = [
            games[0].world.random.next_int_bound(1_000_000).unwrap(),
            games[1].world.random.next_int_bound(1_000_000).unwrap(),
        ];
        if draws[0] != draws[1] {
            return Err("the level's random streams end in different places".into());
        }
        let (asks, hits) = games[1].cow_ecs.as_mut().unwrap().counts();
        Ok((compared, asks, hits))
    }

    /// The control: the ECS path off in both games compares equal (the test is deterministic).
    #[test]
    #[ignore = "daily"]
    fn cows_ecs_control_off_against_off() {
        let (n, asks, hits) = scenario(0, 300).unwrap();
        assert_eq!((n, asks, hits), (300, 0, 0));
    }

    /// Mode 1: the entities, stamps and caches are kept and asked, no answer is reused.
    #[test]
    #[ignore = "daily"]
    fn cows_ecs_same_work_ticks_like_mob() {
        let (n, asks, hits) = scenario(1, 1500).unwrap();
        assert!(n == 1500 && asks > 10_000 && hits == 0, "{n} {asks} {hits}");
    }

    /// Mode 2: the answers are reused (and the game is the same).
    #[test]
    #[ignore = "daily"]
    fn cows_ecs_reuse_ticks_like_mob() {
        let (n, asks, hits) = scenario(2, 1500).unwrap();
        eprintln!("cow ecs: {n} ticks compared, {asks} queries asked, {hits} answered from the cache");
        assert!(n == 1500 && hits * 2 > asks, "{n} {asks} {hits}");
    }

    /// Mode 3: the same cache and stamp kept in the `Mob` itself (no entity, no change detection).
    #[test]
    #[ignore = "daily"]
    fn cows_in_mob_memo_ticks_like_mob() {
        let (n, asks, _) = scenario(3, 1500).unwrap();
        assert!(n == 1500 && asks == 0, "{n} {asks}");
    }
}
