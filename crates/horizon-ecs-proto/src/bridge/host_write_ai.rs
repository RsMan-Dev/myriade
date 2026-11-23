//! The host's writes on the AI of a creature: the path, the move control's wish, the look, the restriction, the sleep, the age, the
//! pause, the aggression and the no-action count. Each one writes a component the world has, or a marker in sparse storage.
use myriade_engine_lib::mobs::MobWorld;
use crate::behaviours::{bed::Sleeping, breed::{BreedData, Params}, follow_parent, hurt_by, item_pickup::Pickup, move_towards_restriction::Restriction};
use crate::bridge::{host::EcsWorld, view};
use crate::cow::{GrowAt, MoveTo, Nav, Pos, Sizes};
use crate::goals::{self, AiPaused, Look, Looking};
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
    pub fn set_no_ai(&mut self, e: Entity, on: bool) {
        let w = self.world_mut();
        crate::dragon::flight::set_no_ai(w, e, on);
        view::mark(w, e);
    }

    /// `IronGolem.setPlayerCreated`: a golem built by a player refuses that player as a target.
    pub fn set_player_created(&mut self, e: Entity, on: bool) {
        crate::behaviours::hurt_by::set_player_created(self.world_mut(), e, on);
    }

    /// The host's AI pause: the marker is posed or taken, and the view is marked since the follower reads it.
    pub fn set_ai_paused(&mut self, e: Entity, on: bool) {
        let w = self.world_mut();
        if on {
            w.entity_mut(e).insert(AiPaused);
        } else {
            w.entity_mut(e).remove::<AiPaused>();
        }
        view::mark(w, e);
    }

    /// `Mob.setBaby`: a baby takes the baby box and grows after the species' baby age (`BreedData`, else the default); an adult takes
    /// its own box back. A creature already in that state is left as it is.
    pub fn set_baby(&mut self, e: Entity, baby: bool) {
        let w = self.world_mut();
        if w.get::<Sizes>(e).is_none() || w.get::<Pos>(e).is_none() {
            return;
        }
        if baby == w.get::<GrowAt>(e).is_some() {
            return;
        }
        if baby {
            let ticks = w.get::<BreedData>(e).map_or(Params::DEFAULT.baby_age, |d| d.baby_age);
            follow_parent::make_baby(w, e, ticks);
        } else {
            w.entity_mut(e).remove::<GrowAt>();
        }
        view::mark(w, e);
    }

    /// `Mob.restrictTo`: the home area of the creature, its wanted point starts empty.
    pub fn restrict_to(&mut self, e: Entity, center: [i32; 3], radius: i32) {
        self.world_mut().entity_mut(e).insert(Restriction { center, radius, wanted: None });
    }

    /// `Mob.clearRestriction`: the radius goes to -1 and the centre stays (`setHomeTo` keeps the home position); no radius means no restriction.
    pub fn clear_restriction(&mut self, e: Entity) {
        if let Some(mut r) = self.world_mut().get_mut::<Restriction>(e) {
            r.radius = -1;
            r.wanted = None;
        }
    }

    /// `Mob.setCanPickUpLoot`: the rule's `picks_up` is set; a creature without the rule gets one with the default reach.
    pub fn set_can_pick_up_loot(&mut self, e: Entity, on: bool) {
        let w = self.world_mut();
        if let Some(mut p) = w.get_mut::<Pickup>(e) {
            p.picks_up = on;
        } else {
            let d = crate::behaviours::item_pickup::Params::DEFAULT;
            w.entity_mut(e).insert(Pickup { reach: d.reach, whole_stack: d.whole_stack, picks_up: on });
        }
    }

    /// `Entity.setInvulnerable`: the `Invulnerable` marker, present when `on` and absent otherwise.
    pub fn set_invulnerable(&mut self, e: Entity, on: bool) {
        let w = self.world_mut();
        if on {
            w.entity_mut(e).insert(crate::invulnerable::Invulnerable);
        } else {
            w.entity_mut(e).remove::<crate::invulnerable::Invulnerable>();
        }
    }

    /// `Mob.setPersistenceRequired`: the creature never despawns on its own when `on`, and the spawn caps count it again when not.
    pub fn set_persistence_required(&mut self, e: Entity, on: bool) {
        let w = self.world_mut();
        if on {
            crate::despawn::keep(w, e);
        } else if w.get::<crate::despawn::Persistent>(e).is_some() {
            crate::despawn::release(w, e);
        }
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

    /// The brain's attack-target memory (`ATTACK_TARGET`): posed for `who`, erased for `None`.
    pub fn set_brain_attack_target(&mut self, e: Entity, who: Option<Who>) {
        let w = self.world_mut();
        match who {
            Some(who) => crate::brain::set(w, e, &crate::brain::ATTACK_TARGET, crate::brain::MemValue::Who(who), None),
            None => crate::brain::erase(w, e, &crate::brain::ATTACK_TARGET),
        }
    }

    /// `Attacker.setTarget`:`who` becomes the target with the riposte's memory (`HurtBy`'s default, halved) and no goal slot; `None`
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
