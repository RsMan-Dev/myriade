//! A mob's position writes inside its tick that are not its walk: `Entity.teleportTo`
//! (`snapTo`: an enderman's, a pet's to its owner, a cat's morning gift, a shulker's, a dismount)
//! and `setPos` (a villager laid in its bed or standing up from it, a size change's nudge). Each
//! is one position update of its own (`setPosRaw`, then `EntityInLevelCallback.onMove`), which
//! `entity_entering_section` tells, so a mob that teleports twice and then walks has made three.
//! Mob code moves the mob with [`snap_to`] or [`set_pos`]; each write is captured by the
//! thread-local section-write collector while the game ticks mobs with the hook listened to.

use horizon_core::geometry::Vec3;




