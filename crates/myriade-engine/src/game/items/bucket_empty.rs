//! Emptying a filled bucket from a player's hand: the `empty_contents` Part's say, the guest's
//! sounds and the client's prediction.

use super::Game;

impl Game<'_> {
    /// `BucketItem.emptyContents` plays `playSound(player, …)`: a guest's own client already
    /// played the empty or evaporate `sound` from its prediction (arismoko/horizon#4684), so those go to the
    /// guest's own list instead of being sent back to them.
    pub(in crate::game) fn keep_guest_bucket_sounds(&mut self, sound: &str) {
        if self.lan_acting.is_none() {
            return;
        }
        let Some(capture) = self.audio.capture.as_mut() else {
            return;
        };
        let want = sound.trim_start_matches("minecraft:");
        let mut keep = Vec::with_capacity(capture.len());
        for entry in capture.drain(..) {
            let name = entry.event.trim_start_matches("minecraft:");
            if name == want || name == "block.fire.extinguish" {
                self.guest_player_sounds.push((
                    entry.event,
                    entry.position,
                    entry.volume,
                    entry.pitch,
                ));
            } else {
                keep.push(entry);
            }
        }
        *capture = keep;
    }

}
