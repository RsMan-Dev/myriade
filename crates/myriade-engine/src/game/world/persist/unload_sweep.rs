//! The chunk-unload sweep of `entity_chunks` (`remove`): every family's
//! rows of the unloaded chunks leave — each was saved above — and their
//! ids are forgotten with them.
use super::*;

impl Game<'_> {
    /// Remove the entities of chunks that unloaded: their records were
    /// written into `out` above, so the rows themselves now go, and what
    /// each family keeps (a ridden boat) stays; projectiles leave with `chunk_unloaded` in the ECS.
    /// `keys` are the chunks leaving.
    pub(super) fn unload_removed_entities(&mut self, keys: &HashSet<(i32, i32)>) {
        let gone = |p: Vec3| keys.contains(&chunk_of(p));
        let mut removed: Vec<u32> = Vec::new();
        let riding = self.riding;
        for (id, c) in self.ecs.crystal_views() {
            if gone(Vec3::new(c.position[0], c.position[1], c.position[2])) {
                if let Some(e) = self.ecs.entity_of(id) {
                    self.ecs.despawn(e);
                }
            }
        }
        for (id, v) in self.dragon_views() {
            if gone(Vec3::new(v.position[0], v.position[1], v.position[2])) {
                removed.push(id);
                if let Some(e) = self.ecs.entity_of(id) {
                    self.ecs.despawn(e);
                }
            }
        }
        for id in self.fang_ids() {
            let Some(at) = self.ecs.position(id) else { continue };
            if gone(Vec3::new(at[0], at[1], at[2])) {
                removed.push(id);
                self.kill_fang(id);
            }
        }
        for a in self.armor_stand_list() {
            if gone(a.position) {
                removed.push(a.id);
                self.armor_stand_remove(a.id);
            }
        }
        for item in self.ecs.item_views() {
            if gone(item.position) {
                removed.push(item.id);
                if let Some(e) = self.ecs.entity_of(item.id) {
                    self.ecs.despawn(e);
                }
            }
        }
        self.persist
            .anon
            .retain(|_, (at, _)| !keys.contains(&chunk_of(*at)));
        for id in removed {
            self.forget_entity(id);
        }
    }
}
