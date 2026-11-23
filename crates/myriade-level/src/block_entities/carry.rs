//! Block entities a moving piston carries (arismoko/horizon#3184): taken out of the world with
//! their attachments and without removal side effects, held by the moving
//! piston's position, and installed where their block lands as a new holder.
//! Each has one owner at a time: it is moved, never cloned, so what it holds
//! can't be duplicated.
use super::{BlockEntities, BlockEntity, Pos, identity};

/// A block entity out of the world while a piston moves its block.
pub struct Carried {
    pub entity: BlockEntity,
    /// Position-owned saved fields and adapter state, owned by the carried instance.
    pub position_data: horizon_core::nbt::Compound,}

impl BlockEntities {
    /// Takes the block entity at `from` out of the world for the moving piston
    /// at `to`. Its slot is freed and the attachment index sees the holder
    /// leave, but it is not removed: no side effects run, so nothing it holds
    /// drops. False when there is none, or it is out for its own tick.
    pub fn carry(&mut self, from: Pos, to: Pos) -> bool {
        if !self.map.contains_key(&from) {
            return false;
        }
        let Some(entity) = self.remove(&from) else {
            return false;
        };
        self.carried.insert(
            to,
            Carried {
                entity,
                position_data: Default::default(),
            },
        );
        true
    }
    /// Installs the block entity carried to `at` there as a new holder: a new
    /// ticker slot at the end of the list, and its attachments bound again.
    /// False when none was carried there.
    pub fn land(&mut self, at: Pos) -> bool {
        let Some(Carried {
            entity,
            ..
        }) = self.carried.remove(&at)
        else {
            return false;
        };
        self.remove(&at);
        self.insert(at, entity);
        true
    }
    /// The block entity carried to `at`, for its moving piston's save.
    pub fn carried(&self, at: &Pos) -> Option<&Carried> {
        self.carried.get(at)
    }
    /// An in-flight tick owns the payload: a piston must refuse rather than replace its lease.
    pub fn is_taken(&self, at: &Pos) -> bool {
        self.out.contains_key(at)
    }
    /// The owning adapter updates position-owned state before a save or landing.
    pub fn carried_mut(&mut self, at: &Pos) -> Option<&mut Carried> {
        self.carried.get_mut(at)
    }
    /// Holds `carried` for the moving piston at `at`, loaded with it.
    pub fn hold_carried(&mut self, at: Pos, carried: Carried) {
        self.carried.insert(at, carried);
    }
    /// Drops the block entity carried to `at` without installing it: a
    /// client's copy once the server's block took the moving piston's place.
    pub fn discard_carried(&mut self, at: &Pos) -> bool {
        self.carried.remove(at).is_some()
    }
}

