//! The ender dragons on the bridge: a summon, the dragons of the level, what the host reads of one (`DragonView`), the spike crystals
//! (`CrystalView`), and the fight's calls (hurts, blasts, crystals, save and tick).
use crate::bridge::host::EcsWorld;
use crate::bridge::ids::NetId;
use crate::bridge::uuids::EntityUuid;
use crate::behaviours::elder_curse::Silent;
use crate::end_crystal as species;
use crate::cow::{Health, Pos, Yaw};
use crate::dragon::{
    crystals::{self, Beam, EndCrystal, Shielded},
    death::DeathTime,
    fight::{self, EndFight, FightEvents, FightSave, FightView},
    fight_tick,
    flight::{DragonRng, NoAi},
    hurt::{self, Blow, Hurt},
    parts::{self, Parts},
    phases::{self, PhaseId},
    respawn,
};
use crate::host_out_fight::FightOut;
use crate::rules::dragon_data::BODY;
use crate::targeting::{Target, Who};
use bevy_ecs::entity::Entity;
use myriade_engine_lib::mobs::MobWorld;
use horizon_core::geometry::{Aabb, Vec3};

/// What the host reads of one dragon: its place and yaw, its health, its phase, its death time, its parts' places and boxes, its target
/// (a network id), whether it is the fight's dragon, whether it is silent and whether its AI is off.
#[derive(Clone, Copy, Debug)]
pub struct DragonView {
    pub entity: Entity,
    pub position: [f64; 3],
    pub yaw: f32,
    pub health: f32,
    pub phase: u8,
    pub death_time: i32,
    pub parts: [[f64; 3]; 8],
    pub boxes: [Aabb; 8],
    pub target: Option<u32>,
    pub fight: bool,
    pub silent: bool,
    pub no_ai: bool,
}

/// A spike crystal as the host reads it: its feet, whether its bottom shows, its beam and its shield.
#[derive(Clone, Copy, Debug)]
pub struct CrystalView {
    pub position: [f64; 3],
    pub show_bottom: bool,
    pub beam: Option<[i32; 3]>,
    pub shield: bool,
}

fn boxes_of(p: &Parts) -> [Aabb; 8] {
    std::array::from_fn(|i| parts::part_box(p, i))
}

impl EcsWorld {
    /// `EnderDragon` summoned at `at` with `yaw`, its generators seeded by `seed`, born in `phase`, its spawn `sheet` applied.
    pub fn summon_dragon(&mut self, mw: &MobWorld<'_>, at: [f64; 3], yaw: f32, seed: i64, phase: u8, sheet: &crate::spawn_sheet::SpawnSheet) -> Entity {
        let e = self.lent(mw, |w| crate::dragon::respawn::summon(w, Vec3::new(at[0], at[1], at[2]), yaw, seed, phase));
        self.apply_sheet(e, sheet);
        e
    }

    /// The dragons of the level (every one carries its flight generators), in entity order.
    pub fn dragon_ids(&self) -> Vec<Entity> {
        self.world().iter_entities().filter(|r| r.contains::<DragonRng>()).map(|r| r.id()).collect()
    }

    /// The view of dragon `e`; None when it is not a dragon.
    pub fn dragon_view(&self, e: Entity) -> Option<DragonView> {
        let w = self.world();
        let p = w.get::<Pos>(e)?.0;
        let parts = w.get::<Parts>(e)?;
        let target = w.get::<Target>(e).and_then(|t| match t.who {
            Who::Player(p) => Some(p),
            Who::Entity(a) => w.get::<NetId>(a).map(|n| n.0),
        });
        Some(DragonView {
            entity: e,
            position: [p.x, p.y, p.z],
            yaw: w.get::<Yaw>(e).map_or(0.0, |y| y.0),
            health: w.get::<Health>(e).map_or(0.0, |h| h.0),
            phase: w.get::<PhaseId>(e).map_or(0, |p| p.0),
            death_time: w.get::<DeathTime>(e).map_or(0, |d| d.0),
            parts: std::array::from_fn(|i| parts.0[i].pos),
            boxes: boxes_of(parts),
            target,
            fight: w.get_resource::<EndFight>().is_some_and(|f| f.dragon == Some(e)),
            silent: w.get::<Silent>(e).is_some(),
            no_ai: w.get::<NoAi>(e).is_some(),
        })
    }

    /// The end crystals of the level, the fight's spikes and the species' alike, with their entities.
    pub fn crystal_views(&self) -> Vec<(Entity, CrystalView)> {
        let w = self.world();
        w.iter_entities()
            .filter_map(|r| {
                let e = r.id();
                let show_bottom = w.get::<EndCrystal>(e).map(|c| c.show_bottom).or_else(|| w.get::<species::EndCrystal>(e).map(|c| c.show_bottom))?;
                let p = w.get::<Pos>(e)?.0;
                let beam = w.get::<Beam>(e).map(|b| b.0).or_else(|| w.get::<species::Beam>(e).map(|b| b.0));
                let shield = w.get::<Shielded>(e).is_some() || w.get::<species::Shielded>(e).is_some();
                Some((e, CrystalView { position: [p.x, p.y, p.z], show_bottom, beam, shield }))
            })
            .collect()
    }

    /// Whether player `id` is in the level (a target the host may name).
    pub fn player_online(&self, id: u32) -> bool {
        fight::player_pos(self.world(), id).is_some()
    }

    /// Dragon `e` targets `target` (None: no target); false when it is not a dragon. The target keeps its memory of 0 ticks.
    pub fn set_dragon_target(&mut self, e: Entity, target: Option<Who>) -> bool {
        let w = self.world_mut();
        if w.get::<DragonRng>(e).is_none() {
            return false;
        }
        match target {
            Some(who) => {
                w.entity_mut(e).insert(Target::new(who, 0, u8::MAX));
            }
            None => {
                w.entity_mut(e).remove::<Target>();
            }
        }
        true
    }

    /// The boxes of the eight parts of dragon `e` (`part_box`), for the ray tests of a hit; empty when it is not a dragon.
    pub fn dragon_boxes(&self, e: Entity) -> Vec<(usize, Aabb)> {
        let Some(p) = self.world().get::<Parts>(e) else { return Vec::new() };
        (0..8).map(|i| (i, parts::part_box(p, i))).collect()
    }

    /// Dragon `e` takes phase `phase`; false when it is not a dragon.
    pub fn set_dragon_phase(&mut self, mw: &MobWorld<'_>, e: Entity, phase: i32) -> bool {
        self.lent(mw, |w| {
            if w.get::<DragonRng>(e).is_none() {
                return false;
            }
            phases::set_phase(w, e, phase as u8);
            true
        })
    }

    /// A hit on dragon `e` (`hurt::hurt`): what it did to the blow.
    pub fn hurt_dragon(&mut self, mw: &MobWorld<'_>, e: Entity, b: Blow) -> Hurt {
        self.lent(mw, |w| hurt::hurt(w, e, b))
    }

    /// An explosion at `center` on dragon `e`: each part it reaches is hurt, then the dragon is pushed. `exposure` gives the share of a box
    /// the blast reaches; the push takes the body's share. None when the dragon is not there to be pushed.
    pub fn blast_dragon(&mut self, mw: &MobWorld<'_>, e: Entity, center: [f64; 3], radius: f32, exposure: &dyn Fn(Aabb) -> f32, by: Option<u32>) -> Option<[f64; 3]> {
        self.lent(mw, |w| {
            for (part, damage) in hurt::explosion_hits(w, e, center, radius, exposure) {
                let b = Blow { part, damage, cause: crate::cow::Cause::Blast, by: by.map(Who::Player), arrow: false, from: None };
                hurt::hurt(w, e, b);
            }
            let body = w.get::<Parts>(e).map(|p| exposure(parts::part_box(p, BODY)))?;
            hurt::explosion_push(w, e, center, radius, body)
        })
    }

    /// The push of an explosion on dragon `e` alone, with no damage (a wind charge); the body's share of `exposure` is the push's.
    pub fn dragon_push(&mut self, mw: &MobWorld<'_>, e: Entity, center: [f64; 3], radius: f32, exposure: &dyn Fn(Aabb) -> f32) -> Option<[f64; 3]> {
        self.lent(mw, |w| {
            let body = w.get::<Parts>(e).map(|p| exposure(parts::part_box(p, BODY)))?;
            hurt::explosion_push(w, e, center, radius, body)
        })
    }

    /// The fight of the saved world: its state from `saved`, the gateways shuffled from `seed` when the save has none.
    pub fn fight_load(&mut self, seed: i64, saved: FightSave) {
        let w = self.world_mut();
        w.insert_resource(fight::restore(seed, saved));
        w.init_resource::<FightEvents>();
        w.init_resource::<FightOut>();
    }

    /// The fight as the host reads it; `loaded` is false until `fight_load` has run.
    pub fn fight_view(&self) -> FightView {
        let w = self.world();
        w.get_resource::<EndFight>().map_or_else(FightView::default, |f| FightView {
            dragon: f.dragon,
            crystals_alive: u32::from(f.crystals_alive),
            exit_portal: f.exit_portal,
            killed: f.killed,
            previously_killed: f.previously_killed,
            respawn: f.respawn,
            gateways: f.gateways.clone(),
            loaded: f.loaded,
        })
    }

    /// The fight's save: its own fields, the respawn crystals' places and the dragon's uuid (the living dragon's, else the saved one).
    pub fn fight_save(&self) -> FightSave {
        let w = self.world();
        let Some(f) = w.get_resource::<EndFight>() else { return FightSave::default() };
        FightSave {
            killed: f.killed,
            previously_killed: f.previously_killed,
            exit_portal: f.exit_portal,
            gateways: f.gateways.clone(),
            respawn: f.respawn,
            respawn_crystals: f.respawn_crystals.iter().filter_map(|&c| w.get::<Pos>(c).map(|p| [p.0.x, p.0.y, p.0.z])).collect(),
            crystals_placed: f.crystals_placed,
            dragon_uuid: f.dragon.and_then(|d| w.get::<EntityUuid>(d).map(|u| u.0)).or(f.dragon_uuid),
            needs_scan: f.needs_scan,
        }
    }

    /// The fight's tick (`fight_tick`), once its arena is loaded (`arena_ready`).
    pub fn fight_tick(&mut self, mw: &MobWorld<'_>, arena_ready: bool) {
        self.lent(mw, |w| fight_tick::tick(w, arena_ready));
    }

    /// A spike crystal at `at` (feet); a respawn is asked when it stands at the exit portal (`try_respawn`).
    pub fn place_crystal(&mut self, mw: &MobWorld<'_>, at: [f64; 3], show_bottom: bool) -> Entity {
        self.lent(mw, |w| {
            let e = crystals::spawn(w, Vec3::new(at[0], at[1], at[2]), show_bottom);
            respawn::try_respawn(w);
            e
        })
    }

    /// A hit on crystal `e` (`crystals::hit`): true when it took it.
    pub fn hit_crystal(&mut self, mw: &MobWorld<'_>, e: Entity, by: Option<u32>, fire: bool, creative: bool) -> bool {
        self.lent(mw, |w| crystals::hit(w, e, by, fire, creative))
    }

    /// Destroys crystal `e` by a direct blow, as the command `kill` does.
    pub fn destroy_crystal(&mut self, mw: &MobWorld<'_>, e: Entity, by: Option<u32>) {
        self.lent(mw, |w| crystals::destroy(w, e, by, true));
    }

    /// Asks for a respawn with the four portal crystals (`respawn::try_respawn`).
    pub fn try_respawn(&mut self, mw: &MobWorld<'_>) -> bool {
        self.lent(mw, |w| respawn::try_respawn(w))
    }

    /// The exit portal's origin is `origin` (`set_exit_portal`).
    pub fn set_exit_portal(&mut self, origin: [i32; 3]) {
        let w = self.world_mut();
        w.init_resource::<EndFight>();
        w.resource_mut::<EndFight>().exit_portal = Some(origin);
    }
}
