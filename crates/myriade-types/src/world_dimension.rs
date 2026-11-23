use crate::*;


/// Where one biome sits in a multi-noise biome source: the climate it is chosen for, as
/// `Climate.ParameterPoint`. Every interval is `(min, max)` inside -2 to 2, with `min` not above
/// `max`; a single value is `(v, v)`. The biome nearest to the level's climate at a position wins,
/// and `offset` (0 to 1) makes this one count as farther away.
#[derive(Clone, Debug, PartialEq)]
pub struct BiomePoint {
    /// The biome's id (`mymod:glowing_forest`, or a vanilla or data pack biome's).
    pub biome: String,
    /// `temperature`, -2 to 2.
    pub temperature: (f32, f32),
    /// `humidity`, -2 to 2.
    pub humidity: (f32, f32),
    /// `continentalness`, -2 to 2: low is ocean, high is inland.
    pub continentalness: (f32, f32),
    /// `erosion`, -2 to 2.
    pub erosion: (f32, f32),
    /// `depth`, -2 to 2: 0 is the surface and above 1 is underground.
    pub depth: (f32, f32),
    /// `weirdness`, -2 to 2.
    pub weirdness: (f32, f32),
    /// `offset`, 0 to 1.
    pub offset: f32,
}

impl BiomePoint {
    /// Whether the point is one the codec accepts: finite intervals inside -2 to 2, ordered, and
    /// an offset inside 0 to 1.
    pub fn is_valid(&self) -> bool {
        let interval = |(min, max): (f32, f32)| {
            (-2.0..=2.0).contains(&min) && (-2.0..=2.0).contains(&max) && min <= max
        };
        !self.biome.is_empty()
            && [
                self.temperature,
                self.humidity,
                self.continentalness,
                self.erosion,
                self.depth,
                self.weirdness,
            ]
            .into_iter()
            .all(interval)
            && (0.0..=1.0).contains(&self.offset)
    }
}
