//! A client mod's reads of the levels the server opened for viewing (arismoko/horizon#3981, gap 10), as Immersive
//! Portals' `ClientWorldLoader.getWorld(dim)` hands out the far `ClientLevel`: on a client,
//! `host.level(dim)` also names each level the server keeps open on `horizon:levels`, and
//! `block_state`, `clip`, `collision_boxes` and `has_collisions` read its world (`viewed_worlds`),
//! as do the reads built on `level_block_state`: `block`, `block_property` and the both-sided
//! fluid reads (`fluid_shape`, `fluid_height`, `fluid_height_for_camera`,
//! `fluid_can_be_replaced_with`). A read never writes and never loads a column: a column the
//! world does not hold is None. The handle answers None from the server's close of the level,
//! and from a Login, a Respawn or a new configuration phase, which drop every viewed world. A
//! `LevelRef` names a level and the client level's epoch, not one open of the view: a close and
//! a later open within the same epoch make the old handle read again, while a Login, a
//! reconfiguration or a Respawn into another level starts a new epoch, so the old handle stays
//! stale and a mod asks `host.level` again (a same-level Respawn keeps the epoch). Writes,
//! server-only reads, the other reads and the entity reads (until step 9d) refuse it.
use super::Game;
use horizon_client::world::World;
use mod_api::LevelRef;

impl Game<'_> {
    /// The world `block_state` and `clip` read: an open level, in place or parked
    /// (`native_level_world`).
    pub(super) fn native_level_view_world(&self, level: LevelRef) -> Option<&World> {
        self.native_level_world(level)
    }
}
