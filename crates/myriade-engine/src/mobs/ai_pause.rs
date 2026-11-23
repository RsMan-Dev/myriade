//! A mob whose AI step a mod paused (`MobRef::set_ai_paused`, arismoko/horizon#4478): one
//! more `LivingEntity.isImmobile` reason, read once at the AI step. The AI
//! step (`serverAiStep`: the Brain or the goal and target selectors,
//! navigation and controls) is skipped and `aiStep` zeroes the jump flag and
//! the horizontal inputs; here the vertical input goes too, as a flying
//! mob's controls can leave it set, and the navigation timeout's clock is
//! held so a resume does not count the pause. The path, move target, target,
//! goals and memories are left as they are. Vanilla never pauses: an
//! unpaused mob pays this one boolean check.
use super::{MobWorld};

