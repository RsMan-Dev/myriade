//! Lot 30 (colle) : the blocks and machines that touch entities, on the server roles. Reads go to the index and the view by id
//! (`ecs_entities_meeting`, `ecs_mob_eye`), writes are requests the level applies (`ecs_add_effect`, `ecs_launch`). Nothing
//! scans the level: the candidates of a box are the index's.
use crate::game::Game;
use myriade_engine::potent_sulfur::{Level, can_be_reached_by_noxious_gas};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_ecs_proto::bridge::host::{Flag, HurtSource};

impl Game<'_> {
    /// The followed entities whose box meets `area`, from the level's spatial index (`EcsLevel::ids_near`).
    pub(in crate::game) fn ecs_entities_meeting(&self, area: Aabb) -> Vec<u32> {
        let (mn, mx) = (area.min_position(), area.max_position());
        let at = [(mn.x + mx.x) / 2.0, (mn.y + mx.y) / 2.0, (mn.z + mx.z) / 2.0];
        let r = ((mx.x - mn.x).powi(2) + (mx.y - mn.y).powi(2) + (mx.z - mn.z).powi(2)).sqrt() / 2.0;
        self.ecs.ids_near(at, r).into_iter().filter(|&id| self.mob_box(id).is_some_and(|b| b.intersects(area))).collect()
    }

    /// The eye point of the followed entity `id` (`Entity.getEyeY` on its position); None when it is absent.
    pub(in crate::game) fn ecs_mob_eye(&self, id: u32) -> Option<[f64; 3]> {
        let [x, y, z] = self.ecs.position(id)?;
        Some([x, y + self.ecs.eye_offset(id)?, z])
    }

    /// The point, width and height of the followed entity `id` as a portal plan takes them (`EcsLevel::size`, still to be bridged).
    pub(in crate::game) fn ecs_body(&self, id: u32) -> Option<(Vec3, f32, f32)> {
        let [x, y, z] = self.ecs.position(id)?;
        let (w, h) = self.ecs.size(id)?;
        Some((Vec3::new(x, y, z), w, h))
    }

    /// `Mob.add_effect` on the followed mob `id`: the effect is named as the mob effects name it; an unknown name or id is skipped.
    pub(in crate::game) fn ecs_add_effect(&mut self, id: u32, effect: &str, duration: i32, amp: i32) {
        self.ecs_mob_add_effect(id, effect, duration, amp, true, false);
    }

    /// The followed mobs of `area` alive and at the eyes of which the noxious gas of `source` reaches, with their kinds.
    pub(in crate::game) fn ecs_gas_reached(&self, source: [i32; 3], area: Aabb) -> Vec<(u32, String)> {
        let level = Level {
            world: &self.world,
            facts: self.res.facts(),
            collision: self.collision,
        };
        self.ecs_entities_meeting(area)
            .into_iter()
            .filter(|&id| self.mob_alive(id))
            .filter_map(|id| {
                let eye = self.ecs_mob_eye(id)?;
                can_be_reached_by_noxious_gas(&level, source, eye).then(|| (id, self.ecs.kind(id).unwrap_or_default().to_owned()))
            })
            .collect()
    }

    /// `PotentSulfurBlockEntity.LAUNCH_ENTITY_TICKER` on the followed entities of `area`: mobs, items, primed TNT, projectiles, boats
    /// and minecarts alike, from the index. Each one falls no further than one block, and one that rises below `limit` gets `push`
    /// more, unless it rides or is of a kind geysers do not move.
    pub(in crate::game) fn ecs_geyser_launch(&mut self, area: Aabb, limit: f64, push: f64) {
        let immune = self.res.stations().enchantments.entity_tags.get("minecraft:not_affected_by_geysers");
        for id in self.ecs_entities_meeting(area) {
            let (Some(e), Some(kind), Some(mut v)) = (self.ecs.entity_of(id), self.ecs.kind(id), self.ecs.velocity(id)) else {
                continue;
            };
            // Entity.checkFallDistanceAccumulation.
            if v.y > -0.5 && self.ecs.fall_distance(id) > 1.0 {
                self.ecs.set_fall_distance(e, 1.0);
            }
            let kind = kind.to_owned();
            if !self.ecs.is_passenger(id) && !immune.is_some_and(|t| t.contains(&kind)) && v.y < limit {
                v.y += push;
                self.ecs.set_velocity(e, v);
            }
        }
    }

    /// `BellBlockEntity` heard, for the followed villager `id`: its brain keeps the time of its own clock (`EcsLevel::heard_bell`).
    pub(in crate::game) fn ecs_heard_bell(&mut self, id: u32, _now: i64) {
        let Some(e) = self.ecs.entity_of(id) else {
            return;
        };
        self.ecs.heard_bell(e);
    }

    /// The conduit's target test of `conduit_attack` on the followed mob `id`: alive, and within 8 blocks of the conduit's block.
    pub(in crate::game) fn ecs_conduit_reachable(&self, id: u32, pos: [i32; 3]) -> bool {
        let Some([x, y, z]) = self.ecs.position(id) else {
            return false;
        };
        let at = [x, y, z].map(|v| v.floor() as i64);
        self.mob_alive(id) && at.iter().zip(pos).map(|(a, b)| (a - i64::from(b)).pow(2)).sum::<i64>() < 64
    }

    /// The enemies of `area` that `selectNewTarget` may pick, with their water and their top: (id, in water, position, top).
    pub(in crate::game) fn ecs_enemies_near(&self, area: Aabb) -> Vec<(u32, bool, Vec3, f64)> {
        self.ecs_entities_meeting(area)
            .into_iter()
            .filter(|&id| self.mob_exists(id) && self.ecs.is_enemy(id))
            .filter_map(|id| {
                let [x, y, z] = self.ecs.position(id)?;
                let (_, height) = self.ecs.size(id)?;
                Some((id, self.ecs.in_water(id), Vec3::new(x, y, z), y + f64::from(height)))
            })
            .collect()
    }
    /// The water-sensitive and burning mobs of a splash's box (`affect_entities_around`), hurt and put out by id.
    pub(in crate::game) fn splash_mobs_ecs(&mut self, reach: Aabb, at: Vec3, hurts: bool, extinguishes: bool) {
        for id in self.ecs_entities_meeting(reach) {
            let Some([x, y, z]) = self.ecs.position(id) else {
                continue;
            };
            let (dx, dy, dz) = (x - at.x, y - at.y, z - at.z);
            // WATER_SENSITIVE_OR_ON_FIRE, within 4 blocks of the splash.
            let sensitive = self.ecs.water_sensitive(id);
            if !(sensitive || (self.ecs.flag(id, Flag::OnFire) == Some(true))) || dx * dx + dy * dy + dz * dz >= 16.0 || !self.mob_exists(id) {
                continue;
            }
            if hurts && sensitive {
                let invulnerable = self.ecs_flag(id, "Invulnerable").unwrap_or(false) || self.ecs.invulnerable_time(id).unwrap_or(0) > 0;
                // Magic kind of the indirect hit (HurtSource::Magic, missing in P: listed).
                self.ecs_hurt(id, 1.0, HurtSource::IndirectMagic { owner: None });
                // Enderman.hurtServer: the repeated teleport attempt, which the AI step runs.
                if self.ecs.kind(id) == Some("minecraft:enderman") && !invulnerable && self.mob_alive(id) && !self.ecs.is_passenger(id) {
                    self.ecs_with(|ecs, mw| ecs.repeated_teleport(mw, id, 64));
                }
            }
            if extinguishes && self.ecs.flag(id, Flag::OnFire) == Some(true) && self.mob_alive(id) {
                self.ecs.extinguish(id);
            }
        }
    }
}
