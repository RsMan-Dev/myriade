//! The host's writes on the AI of a creature: the path, the move control's wish, the look, the restriction, the sleep, the age, the
//! pause, the aggression and the no-action count. Each one writes a component the world has, or a marker in sparse storage.
use horizon_client_lib::mobs::MobWorld;
use crate::behaviours::{bed::Sleeping, breed::{BreedData, Params}, follow_parent, hurt_by, move_towards_restriction::Restriction};
use crate::bridge::{host::EcsWorld, view};
use crate::cow::{box_at, Body, Bounds, GrowAt, MoveTo, Nav, Pos, Sizes};
use crate::dragon::flight::NoAi;
use crate::goals::{self, Look, Looking};
use crate::raids::Aggressive;
use crate::targeting::{self, Target, Who};
use bevy_ecs::{entity::Entity, system::Commands, world::CommandQueue};
use horizon_core::geometry::Vec3;

impl EcsWorld {
    /// `PathNavigation.stop`: the path and the drive go, the creature stands where it is.
    pub fn stop_navigation(&mut self, e: Entity) {
        goals::stop_nav(self.world_mut(), e);
    }

    /// `PathNavigation.moveTo`: a path to `at` at `speed`, ended by `Nav`'s default reach; false for a creature that cannot walk.
    pub fn nav_move_to(&mut self, e: Entity, at: [f64; 3], speed: f64) -> bool {
        let w = self.world_mut();
        if w.get::<Nav>(e).is_none() {
            return false;
        }
        goals::go_to_at(w, e, at, speed, Nav::default().reach);
        true
    }

    /// `MoveControl.setWantedPosition`: the move control walks to `at` at `speed` on the next step.
    pub fn set_wanted(&mut self, e: Entity, at: [f64; 3], speed: f64) {
        if let Some(mut m) = self.world_mut().get_mut::<MoveTo>(e) {
            *m = MoveTo { active: true, x: at[0], y: at[1], z: at[2], speed_modifier: speed };
        }
    }

    /// `LookControl.setLookAt` with the default bounds: the look is borrowed out, aimed by `aim_clamped`, and given back.
    pub fn look_at(&mut self, e: Entity, at: [f64; 3]) {
        let w = self.world_mut();
        let Some(mut look) = w.get::<Look>(e).copied() else { return };
        let looking = w.get::<Looking>(e).is_some();
        let mut queue = CommandQueue::default();
        goals::aim_clamped(&mut Commands::new(&mut queue, w), e, &mut look, looking, at, None, None);
        if let Some(mut l) = w.get_mut::<Look>(e) {
            *l = look;
        }
        queue.apply(w);
    }

    /// `PatrollingMonster.setPatrolLeader`: the raider's `Patrol` is made if it has none.
    pub fn set_patrol_leader(&mut self, e: Entity, on: bool) {
        crate::raids::set_patrol_leader(self.world_mut(), e, on);
    }

    /// The raider's aggression: the marker is posed or taken (sparse, only the raiders carry it).
    pub fn set_aggressive(&mut self, e: Entity, on: bool) {
        let w = self.world_mut();
        if on {
            w.entity_mut(e).insert(Aggressive);
        } else {
            w.entity_mut(e).remove::<Aggressive>();
        }
    }

    /// `Mob.setNoAi`: the marker is posed or taken (sparse), and the view is marked since the follower reads it.
    pub fn set_ai_paused(&mut self, e: Entity, on: bool) {
        let w = self.world_mut();
        if on {
            w.entity_mut(e).insert(NoAi);
        } else {
            w.entity_mut(e).remove::<NoAi>();
        }
        view::mark(w, e);
    }

    /// `Mob.setBaby`: a baby takes the baby box and grows after the species' baby age (`BreedData`, else the default); an adult takes
    /// its own box back. A creature already in that state is left as it is.
    pub fn set_baby(&mut self, e: Entity, baby: bool) {
        let w = self.world_mut();
        let (Some(s), Some(p)) = (w.get::<Sizes>(e).copied(), w.get::<Pos>(e).map(|p| p.0)) else { return };
        if baby == w.get::<GrowAt>(e).is_some() {
            return;
        }
        if baby {
            let ticks = w.get::<BreedData>(e).map_or(Params::DEFAULT.baby_age, |d| d.baby_age);
            follow_parent::make_baby(w, e, ticks);
        } else {
            w.entity_mut(e).remove::<GrowAt>();
            if let Some(mut b) = w.get_mut::<Body>(e) {
                b.width = s.adult.0;
                b.height = s.adult.1;
            }
            if let Some(mut b) = w.get_mut::<Bounds>(e) {
                b.0 = box_at(p, s.adult.0, s.adult.1);
            }
        }
        view::mark(w, e);
    }

    /// `Mob.restrictTo`: the home area of the creature, its wanted point starts empty.
    pub fn restrict_to(&mut self, e: Entity, center: [i32; 3], radius: i32) {
        self.world_mut().entity_mut(e).insert(Restriction { center, radius, wanted: None });
    }

    /// `Mob.clearRestriction`: the restriction goes.
    pub fn clear_restriction(&mut self, e: Entity) {
        self.world_mut().entity_mut(e).remove::<Restriction>();
    }

    /// `LivingEntity.startSleeping`: the creature lies in the bed at `pos`, its body at `at` (the bed's side, as the client reads it);
    /// the view is marked, since it reads the sleeping position.
    pub fn start_sleeping(&mut self, e: Entity, pos: [i32; 3], at: Vec3) {
        let w = self.world_mut();
        if let Some(mut p) = w.get_mut::<Pos>(e) {
            p.0 = at;
        }
        w.entity_mut(e).insert(Sleeping(pos));
        view::mark(w, e);
    }

    /// `LivingEntity.stopSleeping`: the creature gets up; the view is marked.
    pub fn stop_sleeping(&mut self, e: Entity) {
        let w = self.world_mut();
        w.entity_mut(e).remove::<Sleeping>();
        view::mark(w, e);
    }

    /// `Attacker.setTarget`: `who` becomes the target with the riposte's memory (`HurtBy`'s default, halved) and no goal slot; `None`
    /// takes the target away.
    pub fn set_target(&mut self, mw: &MobWorld<'_>, e: Entity, who: Option<Who>) {
        self.lent(mw, |w| match who {
            Some(who) => {
                let memory = ((hurt_by::Params::DEFAULT.unseen_memory + 1) / 2) as u16;
                targeting::set_target(w, e, Target::new(who, memory, u8::MAX));
            }
            None => targeting::clear_target(w, e),
        })
    }
}
