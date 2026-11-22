//! The loom screen (`LoomScreen`): the scrolling pattern list, the banner
//! preview and the six-layer error. The menu itself (`LoomMenu`: slots,
//! selectable patterns, the pattern buttons, quick moves, the result) runs on
//! the verified engine, `horizon_client::menu`, through `Game::menu_session`.
use super::{Game, stations::Kind};
use horizon_client::{
    banner,
    items::ItemStack,
};
use std::sync::Arc;

/// Per-menu loom state: `selectablePatterns`, the selected index and the
/// screen's scroller.
#[derive(Default)]
pub(super) struct LoomState {
    pub(super) patterns: Vec<Arc<str>>,
    pub(super) selected: Option<usize>,
    /// `displayPatterns`: the pattern buttons are shown.
    pub(super) display: bool,
    pub(super) has_max: bool,
    pub(super) scroll: f32,
    pub(super) start_row: usize,
    /// The inputs the screen last saw (`bannerStack`, `dyeStack`, `patternStack`).
    last_inputs: [Option<ItemStack>; 3],
}
impl LoomState {
    fn total_rows(&self) -> usize {
        self.patterns.len().div_ceil(4)
    }
}
impl Game<'_> {
    /// `LoomScreen.containerChanged`: the error flag, whether the pattern
    /// buttons show (only when an input changed), and the scroller reset.
    pub(super) fn loom_screen_changed(&mut self) {
        let Some(s) = self.station.as_mut().filter(|s| s.kind == Kind::Loom) else {
            return;
        };
        let inputs = [
            s.inputs[0].clone(),
            s.inputs[1].clone(),
            s.inputs[2].clone(),
        ];
        let l = &mut s.loom;
        l.has_max = inputs[0].as_ref().is_some_and(banner::has_max_layers);
        if inputs != l.last_inputs {
            l.display =
                inputs[0].is_some() && inputs[1].is_some() && !l.has_max && !l.patterns.is_empty();
        }
        if l.start_row >= l.total_rows() {
            l.start_row = 0;
            l.scroll = 0.0;
        }
        l.last_inputs = inputs;
    }
    /// A sound the loom or stonecutter menu played: the result's once per
    /// tick (`lastSoundTime`).
    pub(super) fn station_sound(&mut self, sound: &str) {
        let ticks = self.ticks;
        let Some(s) = self.station.as_mut() else {
            return;
        };
        if s.last_sound == ticks {
            return;
        }
        s.last_sound = ticks;
        let pos = s.pos;
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]) + 0.5,
            f64::from(pos[2]) + 0.5,
        ];
        self.audio.play(sound, Some(at), 1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroller_rows_follow_the_offset() {
        let l = LoomState {
            patterns: (0..40)
                .map(|i| Arc::from(format!("p{i}").as_str()))
                .collect(),
            display: true,
            ..Default::default()
        };
        // 40 patterns = 10 rows, 6 off screen.
        assert_eq!(l.offscreen_rows(), 6);
        assert_eq!(l.row_for(0.0), 0);
        assert_eq!(l.row_for(1.0), 6);
        assert_eq!(l.row_for(0.5), 3);
    }
    #[test]
    fn few_patterns_do_not_scroll() {
        let l = LoomState {
            patterns: (0..16)
                .map(|i| Arc::from(format!("p{i}").as_str()))
                .collect(),
            display: true,
            ..Default::default()
        };
        assert_eq!(l.offscreen_rows(), 0);
    }
}
