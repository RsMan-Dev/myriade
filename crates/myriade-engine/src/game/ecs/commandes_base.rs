//! The commands' reads of a mob the ECS holds, by its network id (`Target::Mob`): the selector's candidates from the spatial index,
//! the name and the facing. The callers in `administration/commands` branch on `self.ecs.is_active()`; the host's mob list is the
//! other branch. The mob is a `Target::Mob` like any other: only its reads move.
use crate::game::Game;
use crate::game::commands::Target;
use crate::lan::entity::EntityState;
use myriade_engine::{command::args::Selector, text::Text};
use horizon_ecs_proto::bridge::host::Select;

impl Game<'_> {
    /// The ids of the ECS mobs a selector may pick: the index is asked for its one positive `type=` and its `distance=` upper bound;
    /// the selector's own tests run after, in `select_in_level`. Each mob found gets its network id here, as a command names it.
    pub(in crate::game) fn ecs_mob_candidates(&mut self, s: &Selector, origin: [f64; 3]) -> Vec<u32> {
        let kind = match s.types.as_slice() {
            [(k, false)] if !k.starts_with('#') => Some(k.clone()),
            _ => None,
        };
        let near = s.distance.and_then(|(_, hi)| hi).map(|h| (origin, h));
        let q = Select { kind, tag: None, near, limit: usize::MAX };
        // `@e` and `@n` skip the dead (`Entity::isAlive` in `EntitySelector`): a body lingering its 20 ticks is not selected.
        let alive_only = matches!(s.kind, 'e' | 'n');
        let found: Vec<_> = self.ecs.select(&q).into_iter().filter(|&e| !(alive_only && self.ecs.is_dead(e))).collect();
        let mut ids: Vec<u32> = found.into_iter().map(|e| self.ecs_link(e)).collect();
        // Projectiles are not in the spatial index yet: a bounded query on their marker, per command (goes with the lot P index).
        let (min, max) = match s.distance.and_then(|(_, hi)| hi) {
            Some(h) => ([origin[0] - h, origin[1] - h, origin[2] - h], [origin[0] + h, origin[1] + h, origin[2] + h]),
            None => ([-1.0e9; 3], [1.0e9; 3]),
        };
        ids.extend(self.ecs.projectiles_in(min, max));
        ids
    }

    /// The ECS entities a selector may pick, each as the `Target` its kind takes (see `ecs_target_of`).
    pub(in crate::game) fn ecs_selector_targets(&mut self, s: &Selector, origin: [f64; 3]) -> Vec<Target> {
        let boats: std::collections::HashSet<u32> = self.ecs.boat_ids().into_iter().collect();
        let carts: std::collections::HashSet<u32> = self.ecs.cart_ids().into_iter().collect();
        let dragons: std::collections::HashSet<u32> = self.ecs.dragon_views().into_iter().map(|(d, _)| d).collect();
        let ids = self.ecs_mob_candidates(s, origin);
        ids.into_iter()
            .map(|id| match self.ecs.kind(id).as_deref() {
                Some("minecraft:item") => Target::Item(id),
                Some("minecraft:experience_orb") => Target::Orb(id),
                Some("minecraft:tnt") => Target::Tnt(id),
                Some("minecraft:falling_block") => Target::Falling(id),
                Some("minecraft:end_crystal") => Target::Crystal(id),
                Some("minecraft:area_effect_cloud") => Target::Cloud(id),
                Some("minecraft:ominous_item_spawner") => Target::Spawner(id),
                // The kinds `level_candidates` lists by their own variant: the same entity is one candidate, not two.
                Some("minecraft:cushion") => Target::Cushion(id),
                Some("minecraft:item_frame" | "minecraft:glow_item_frame") => Target::Frame(id),
                Some("minecraft:painting") => Target::Painting(id),
                Some("minecraft:evoker_fangs") => Target::Fangs(id),
                Some("minecraft:leash_knot") => Target::Knot(id),
                _ if dragons.contains(&id) => Target::Dragon(id),
                _ if boats.contains(&id) => Target::Boat(id),
                _ if carts.contains(&id) => Target::Cart(id),
                _ => Target::Mob(id),
            })
            .collect()
    }

    /// The `Target` of the ECS entity `id`, by its kind: items, orbs, TNT, boats and carts by their own variant, the others mobs.
    pub(in crate::game) fn ecs_target_of(&self, id: u32) -> Target {
        match self.ecs.kind(id) {
            Some("minecraft:item") => Target::Item(id),
            Some("minecraft:experience_orb") => Target::Orb(id),
            Some("minecraft:tnt") => Target::Tnt(id),
            Some("minecraft:falling_block") => Target::Falling(id),
            Some("minecraft:ominous_item_spawner") => Target::Spawner(id),
            _ if self.ecs.boat_ids().contains(&id) => Target::Boat(id),
            _ if self.ecs.cart_ids().contains(&id) => Target::Cart(id),
            _ if self.ecs.dragon_views().iter().any(|(d, _)| *d == id) => Target::Dragon(id),
            _ => Target::Mob(id),
        }
    }

    /// `Entity.getDisplayName` of the ECS mob `id` by its type: `entity.<type>`. The CustomName, the team's format and the hover
    /// of the host's mob are not read here (the ECS holds no name yet).
    pub(in crate::game) fn ecs_mob_name(&self, id: u32) -> Text {
        let ty = self.ecs.kind(id).unwrap_or_default();
        // `Villager.getTypeName`: the profession's name, `entity.minecraft.villager.<profession>`.
        if ty == "minecraft:villager" {
            if let Some(profession) = self.ecs.trade_profession(id).filter(|p| !p.is_empty()) {
                return Text::translate(&format!("entity.minecraft.villager.{profession}"), &[]);
            }
        }
        Text::translate(&format!("entity.{}", ty.replace(':', ".")), &[])
    }

    /// The yaw and the pitch the tracker sends for the ECS mob `id`; `[0, 0]` when the level does not hold it.
    pub(in crate::game) fn ecs_mob_rotation(&self, id: u32) -> [f32; 2] {
        let mut st = EntityState::default();
        let Some(e) = self.ecs.entity_of(id) else { return [0.0; 2] };
        if self.ecs.view_state(e, id as i32, &mut st) { [st.yaw, st.pitch] } else { [0.0; 2] }
    }
}

#[cfg(test)]
mod tests {
    use crate::ecs_bridge::EcsLevel;
    use crate::game::native_entity_refs::tests::with_game;
    use myriade_engine::command::args::Selector;
    use horizon_ecs_proto::bridge::host::HostSetup;

    #[test]
    fn a_selector_of_type_and_distance_finds_the_ecs_mob_of_that_type_within_reach() {
        with_game(|game| {
            let level = {
                game.test_ecs_level()
            };
            game.ecs = level;
            let near = game.ecs_spawn("minecraft:cow", [0.5, 64.0, 0.5], None).unwrap();
            let _far = game.ecs_spawn("minecraft:cow", [50.5, 64.0, 0.5], None).unwrap();
            let _pig = game.ecs_spawn("minecraft:pig", [1.5, 64.0, 0.5], None).unwrap();
            let s = Selector {
                kind: 'e',
                types: vec![("minecraft:cow".into(), false)],
                distance: Some((None, Some(8.0))),
                ..Default::default()
            };
            assert_eq!(game.ecs_mob_candidates(&s, [0.5, 64.0, 0.5]), vec![near]);
        });
    }
}
