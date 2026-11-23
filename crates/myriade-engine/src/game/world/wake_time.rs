//! Where sleeping through the night moves the level's clock: vanilla's morning, or the mods'
//! `ClockAdjustment` from `sleep_finished_time` (NeoForge's `SleepFinishedTimeEvent`), and the
//! clock time markers both read.
use super::{Dim, Game};
use myriade_types::ClockAdjustment;
use myriade_types::Identifier;

/// Vanilla's wake-up time marker, where sleeping through the night moves the clock
/// (`ClockTimeMarkers.WAKE_UP_FROM_SLEEP`).
const WAKE_UP_FROM_SLEEP: &str = "minecraft:wake_up_from_sleep";
/// A day on the clock, in ticks.
const DAY: i64 = 24_000;

/// The Overworld's world clock.
pub(in crate::game) const OVERWORLD_CLOCK: &str = "minecraft:overworld";
/// A clock time marker from the timelines (ClockTimeMarker); `resolve` is None past `i64::MAX`.
#[derive(Clone, Copy, Debug)]
pub struct TimeMarker {
    pub ticks: i64,
    pub period: Option<i64>,
    pub show: bool,
}
impl TimeMarker {
    pub fn occurs_at(&self, total: i64) -> bool {
        match self.period {
            Some(p) => self.ticks == total.rem_euclid(p),
            None => self.ticks == total,
        }
    }
    pub fn resolve(&self, total: i64) -> Option<i64> {
        match self.period {
            Some(p) => {
                let d = self.ticks - total.rem_euclid(p);
                total.checked_add(if d > 0 { d } else { p + d })
            }
            None => Some(self.ticks),
        }
    }
}

/// Why the game refused a mod's adjustment; the clock then moves to vanilla's morning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refused {
    /// The time is before the clock's time now (`setTimeAddition` refuses a lower one).
    BeforeNow,
    /// The level's clock defines no such marker.
    UndefinedMarker,
    /// The time does not fit the clock (past `i64::MAX`).
    OutOfRange,
}

impl Game<'_> {
    /// The time marker `id` of `clock`. The timelines' markers belong to the Overworld's clock:
    /// no other clock defines one today, as `/time set <marker>` reads them.
    fn clock_marker(&self, clock: &str, id: &Identifier) -> Option<TimeMarker> {
        if clock != OVERWORLD_CLOCK {
            return None;
        }
        let id = format!("{}:{}", id.namespace(), id.path());
        self.session.markers.get(&id).copied()
    }

    /// The level's clock, its time now and where sleeping moves it: vanilla's morning, or the
    /// marker's time when it resolves; None when no time fits (the clock is at the end of its range).
    pub(super) fn sleep_finished_move(&mut self) -> Option<(String, i64, i64)> {
        let clock = self.level_clock_name(self.dim);
        let current = self.command_clock_time(&clock);
        let morning = vanilla_morning(current);
        let adjustment =
            ClockAdjustment::Marker(Identifier::parse(WAKE_UP_FROM_SLEEP).expect("a valid id"));
        let target = wake_time(current, &adjustment, |id| self.clock_marker(&clock, id))
            .ok()
            .or(morning);
        target.map(|target| (clock, current, target))
    }
}

/// The morning after `current`, as the game woke sleepers before the markers: the next whole day.
fn vanilla_morning(current: i64) -> Option<i64> {
    current.checked_add(DAY - current.rem_euclid(DAY))
}

/// Where `adjustment` moves the clock from `current`, `marker` giving the level's clock's markers.
fn wake_time(
    current: i64,
    adjustment: &ClockAdjustment,
    marker: impl FnOnce(&Identifier) -> Option<TimeMarker>,
) -> Result<i64, Refused> {
    let time = match adjustment {
        ClockAdjustment::Marker(id) => marker(id).ok_or(Refused::UndefinedMarker)?.resolve(current),
        other => other.resolve(current, |_| None),
    }
    .ok_or(Refused::OutOfRange)?;
    if time < current {
        return Err(Refused::BeforeNow);
    }
    Ok(time)
}

#[cfg(test)]
mod tests {
    use super::{OVERWORLD_CLOCK, Refused, TimeMarker, WAKE_UP_FROM_SLEEP, vanilla_morning, wake_time};
    use myriade_types::ClockAdjustment;
    use myriade_types::Identifier;

    const NOW: i64 = 13_000;

    fn id(s: &str) -> Identifier {
        Identifier::parse(s).unwrap()
    }
    fn marker(s: &str) -> ClockAdjustment {
        ClockAdjustment::Marker(id(s))
    }
    /// The Overworld clock's markers as vanilla ships the two used here.
    fn overworld(id: &Identifier) -> Option<TimeMarker> {
        let at = |ticks| TimeMarker {
            ticks,
            period: Some(24_000),
            show: false,
        };
        match format!("{}:{}", id.namespace(), id.path()).as_str() {
            WAKE_UP_FROM_SLEEP => Some(at(0)),
            "minecraft:noon" => Some(at(6_000)),
            _ => None,
        }
    }
    /// A clock that defines no markers, as every clock but the Overworld's today.
    fn bare(_: &Identifier) -> Option<TimeMarker> {
        None
    }

    #[test]
    fn vanillas_marker_is_the_morning_as_before() {
        for now in [0, 1, NOW, 23_999, 24_000, 48_001] {
            assert_eq!(
                wake_time(now, &marker(WAKE_UP_FROM_SLEEP), overworld).ok(),
                vanilla_morning(now),
                "{now}"
            );
        }
        assert_eq!(vanilla_morning(NOW), Some(24_000));
        assert_eq!(vanilla_morning(24_000), Some(48_000));
    }

    #[test]
    fn a_mods_adjustment_moves_the_clock_where_it_says() {
        let at = |a: ClockAdjustment| wake_time(NOW, &a, overworld);
        assert_eq!(at(ClockAdjustment::Relative(1_000)), Ok(14_000));
        assert_eq!(at(ClockAdjustment::Absolute(20_000)), Ok(20_000));
        assert_eq!(at(ClockAdjustment::Absolute(NOW)), Ok(NOW));
        assert_eq!(at(marker("minecraft:noon")), Ok(30_000));
    }

    #[test]
    fn a_time_before_now_or_an_undefined_marker_is_refused() {
        let at = |a: ClockAdjustment| wake_time(NOW, &a, overworld);
        assert_eq!(
            at(ClockAdjustment::Absolute(NOW - 1)),
            Err(Refused::BeforeNow)
        );
        assert_eq!(at(ClockAdjustment::Relative(-5)), Err(Refused::BeforeNow));
        assert_eq!(
            at(ClockAdjustment::Relative(i64::MIN)),
            Err(Refused::BeforeNow)
        );
        assert_eq!(
            at(marker("example:mod_dawn")),
            Err(Refused::UndefinedMarker)
        );
    }

    /// The wake marker has no special case: a clock without it refuses it like any marker, and
    /// the sleep then falls back to the plain morning.
    #[test]
    fn a_clock_without_markers_refuses_even_the_wake_marker() {
        assert_eq!(
            wake_time(NOW, &marker(WAKE_UP_FROM_SLEEP), bare),
            Err(Refused::UndefinedMarker)
        );
        assert_eq!(
            wake_time(NOW, &ClockAdjustment::Relative(1_000), bare),
            Ok(14_000)
        );
    }

    /// Mod data at the ends of the range never overflows: a time past `i64::MAX` is refused, and
    /// a clock a mod left at `i64::MAX` has no morning to move to.
    #[test]
    fn times_at_the_end_of_the_range_are_refused_not_wrapped() {
        assert_eq!(
            wake_time(NOW, &ClockAdjustment::Absolute(i64::MAX), overworld),
            Ok(i64::MAX)
        );
        assert_eq!(
            wake_time(NOW, &ClockAdjustment::Relative(i64::MAX), overworld),
            Err(Refused::OutOfRange)
        );
        let end = i64::MAX;
        assert_eq!(vanilla_morning(end), None);
        assert_eq!(vanilla_morning(end - 5), None);
        assert_eq!(
            wake_time(end, &marker(WAKE_UP_FROM_SLEEP), overworld),
            Err(Refused::OutOfRange)
        );
        assert_eq!(
            wake_time(end - 5, &marker("minecraft:noon"), overworld),
            Err(Refused::OutOfRange)
        );
        assert_eq!(
            wake_time(end, &ClockAdjustment::Relative(0), overworld),
            Ok(end)
        );
        // The last morning that fits.
        let last = end - end.rem_euclid(24_000);
        assert_eq!(vanilla_morning(last - 1), Some(last));
    }
}
