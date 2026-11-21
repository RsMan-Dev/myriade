//! The lightning, end crystals, fangs and area clouds on the bridge: each spawn runs under the lent world, and what they queue is read with
//! `EcsWorld::drain_misc`.
use crate::bridge::host::EcsWorld;
use crate::cloud_tick;
use crate::effect_cloud::CloudSpec;
use crate::colony_register::MiscOut;
use crate::end_crystal::Blow;
use crate::end_crystal_tick;
use crate::fangs_tick;
use crate::lightning::BoltLevel;
use crate::lightning_tick;
use crate::targeting::Who;
use bevy_ecs::entity::Entity;
use horizon_client_lib::mobs::MobWorld;
use horizon_core::geometry::Vec3;

fn at_of(at: [f64; 3]) -> Vec3 {
    Vec3::new(at[0], at[1], at[2])
}

impl EcsWorld {
    /// A bolt at `at`; `cosmetic` for a skeleton trap's; `level` the host's blocks for the strike.
    pub fn strike(&mut self, mw: &MobWorld<'_>, at: [f64; 3], cosmetic: bool, level: Option<Box<dyn BoltLevel>>) -> Entity {
        self.lent(mw, |w| lightning_tick::strike(w, at_of(at), cosmetic, level))
    }

    /// The player whose trident summons the next bolt (`LightningBolt.setCause`); the next `strike` takes it.
    pub fn set_next_bolt_cause(&mut self, player: Option<u32>) {
        self.world_mut().insert_resource(crate::lightning::NextCause(player));
    }

    /// An end crystal at `at` (its feet).
    pub fn spawn_crystal(&mut self, mw: &MobWorld<'_>, at: [f64; 3], show_bottom: bool) -> Entity {
        self.lent(mw, |w| end_crystal_tick::spawn(w, at_of(at), show_bottom))
    }

    /// A blow on a crystal; true when it goes (its blast is queued).
    pub fn hurt_crystal(&mut self, mw: &MobWorld<'_>, e: Entity, blow: Blow) -> bool {
        self.lent(mw, |w| end_crystal_tick::hurt(w, e, blow))
    }

    /// Evoker fangs at `at`, biting after `warmup` ticks for `owner`.
    pub fn spawn_fangs(&mut self, mw: &MobWorld<'_>, at: [f64; 3], yaw: f32, warmup: i32, owner: Option<Who>) -> Entity {
        self.lent(mw, |w| fangs_tick::spawn(w, at_of(at), yaw, warmup, owner))
    }

    /// A potion cloud at `at`, from its `CloudSpec` (`CloudSpec::new`, the jar's defaults, and the fields the caller changes).
    pub fn spawn_cloud(&mut self, mw: &MobWorld<'_>, at: [f64; 3], spec: CloudSpec) -> Entity {
        self.lent(mw, |w| cloud_tick::spawn(w, at_of(at), spec))
    }

    /// The host's answer to `MiscEvent::BeeEnterHive` when the hive is gone or full: the bee forgets it (`hivePos = null`).
    pub fn bee_forget_hive(&mut self, e: Entity) {
        crate::behaviours::bee::forget_hive(self.world_mut(), e);
    }

    /// The host's answer to `MiscEvent::TreasureAsk`: the structure block found for the dolphin, or `None`.
    pub fn dolphin_treasure(&mut self, e: Entity, at: Option<[i32; 3]>) {
        crate::behaviours::swim_to_treasure::treasure_found(self.world_mut(), e, at);
    }

    /// `Bee.setStayOutOfHiveCountdown(ticks)`.
    pub fn bee_stay_out(&mut self, e: Entity, ticks: u64) {
        crate::behaviours::bee::stay_out(self.world_mut(), e, ticks);
    }

    /// A bee the host just made from the data of the hive `hive`: it is put at the hive's exit (`facing` step; `blocked` front: on the hive) and
    /// its hive goals are armed again (`delivered`: it gave its nectar).
    pub fn bee_exited(&mut self, mw: &MobWorld<'_>, e: Entity, hive: [i32; 3], facing: [i32; 2], blocked: bool, delivered: bool) {
        if let Some(b) = self.bounds(e) {
            let size = b.size();
            let p = crate::rules::bee::exit_point(hive, facing, blocked, size.x as f32, size.y as f32);
            self.teleport(mw, e, [p.x, p.y, p.z]);
        }
        self.lent(mw, |w| crate::behaviours::bee::exited(w, e, delivered));
    }

    /// Whether the bees have been told that the hive block at `hive` is full.
    pub fn hive_is_full(&self, hive: [i32; 3]) -> bool {
        self.world().get_resource::<crate::behaviours::bee::FullHives>().is_some_and(|f| f.0.contains(&hive))
    }

    /// Tells the bees that the hive block at `hive` holds three occupants (`full`) or has room again.
    pub fn set_hive_full(&mut self, hive: [i32; 3], full: bool) {
        let mut set = self.world_mut().get_resource_or_insert_with(crate::behaviours::bee::FullHives::default);
        if full {
            set.0.insert(hive);
        } else {
            set.0.remove(&hive);
        }
    }

    /// The bees whose box meets `area` and that have no target (`BeehiveBlock.angerNearbyBees`), from the spatial index.
    pub fn untargeted_bees_in(&self, area: horizon_core::geometry::Aabb) -> Vec<Entity> {
        let (c, (mn, mx)) = (area.center(), (area.min_position(), area.max_position()));
        let radius = f64::from((mx.x - mn.x).max(mx.z - mn.z)) * 0.5 + 1.0;
        let q = crate::bridge::select::Select { kind: None, tag: None, near: Some(([c.x, c.y, c.z], radius)), limit: 0 };
        crate::bridge::select::select(self.world(), &q)
            .into_iter()
            .filter(|&e| {
                self.world().get::<crate::behaviours::bee::Bee>(e).is_some()
                    && self.world().get::<crate::targeting::Target>(e).is_none()
                    && self.world().get::<crate::cow::Bounds>(e).is_some_and(|b| b.0.intersects(area))
            })
            .collect()
    }
}
