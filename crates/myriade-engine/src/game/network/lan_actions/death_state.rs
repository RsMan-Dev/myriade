use crate::game::Game;

impl Game<'_> {
    /// A veto belongs to the guest's living body, not its replaceable combat history.
    pub(in crate::game) fn guest_death_canceled(&mut self, gi: usize, conn: u32) -> bool {
        if self
            .lan
            .as_ref()
            .is_some_and(|host| host.hub.guests[gi].death_canceled)
        {
            return true;
        }
        false
    }
}
