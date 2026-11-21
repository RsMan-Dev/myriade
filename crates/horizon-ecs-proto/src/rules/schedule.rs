/// The time of day (0..24000) of an absolute day time, which counts past 24000 (the restock counts days from it).
pub fn time_of_day(absolute: i64) -> i64 {
    absolute.rem_euclid(24000)
}
