//! Far terrain data: voxel columns for distances beyond the resident chunk area, and their
//! baked and cached samples (`bake`, `cache`). Nothing here is drawn.
use crate::chunk::MIN_Y;
use horizon_world::registry::BlockStateId;

pub mod bake;
pub mod cache;

/// Columns on a tile's side.
pub const CELLS: usize = 16;
/// How many resident chunks may wait for the worker's summary at once.
pub const OBSERVE_AHEAD: usize = 8;
/// The farthest far terrain reaches, in chunks.
pub const MAX_CHUNKS: u32 = 1 << 22;

/// Top solid face and optional fluid face, in absolute block coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Surface {
    pub height: i32,
    pub state: BlockStateId,
    pub water: Option<(i32, BlockStateId)>,
    pub biome: u16,
}
/// A half-open material run; gaps between runs remain air.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub min: i16,
    pub max: i16,
    pub state: BlockStateId,
    pub biome: u16,
    pub light: u16,
}
/// A column's material runs. Almost every far column has one to four, so
/// they live inline: the sampler builds millions of columns and a heap
/// allocation each dominated its memory and allocator time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spans(smallvec::SmallVec<[Span; 4]>);
impl Spans {
    pub const fn new() -> Self {
        Self(smallvec::SmallVec::new_const())
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self(smallvec::SmallVec::with_capacity(capacity))
    }
    pub fn push(&mut self, span: Span) {
        self.0.push(span);
    }
    pub fn reserve(&mut self, additional: usize) {
        self.0.reserve(additional);
    }
    pub fn as_slice(&self) -> &[Span] {
        &self.0
    }
}
impl std::ops::Deref for Spans {
    type Target = [Span];
    fn deref(&self) -> &[Span] {
        &self.0
    }
}
impl std::ops::DerefMut for Spans {
    fn deref_mut(&mut self) -> &mut [Span] {
        &mut self.0
    }
}
impl From<Vec<Span>> for Spans {
    fn from(spans: Vec<Span>) -> Self {
        Self(spans.into())
    }
}
impl FromIterator<Span> for Spans {
    fn from_iter<I: IntoIterator<Item = Span>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}
impl IntoIterator for Spans {
    type Item = Span;
    type IntoIter = smallvec::IntoIter<[Span; 4]>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
impl<'a> IntoIterator for &'a Spans {
    type Item = &'a Span;
    type IntoIter = std::slice::Iter<'a, Span>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
impl<'a> IntoIterator for &'a mut Spans {
    type Item = &'a mut Span;
    type IntoIter = std::slice::IterMut<'a, Span>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}
impl<const N: usize> PartialEq<[Span; N]> for Spans {
    fn eq(&self, other: &[Span; N]) -> bool {
        self.as_slice() == other
    }
}
impl PartialEq<Vec<Span>> for Spans {
    fn eq(&self, other: &Vec<Span>) -> bool {
        self.as_slice() == other.as_slice()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Column {
    pub surface: Option<Surface>,
    pub spans: Spans,
}
impl Column {
    /// Reduce an entire horizontal footprint, rather than inflating its corner
    /// column. A height is drawn where any column of the footprint holds a
    /// block there, in the material most of them hold: anything joined to
    /// the ground stays joined at every width. Drawing only heights most of
    /// the footprint held dropped trunks (one column of 16 or 256) under the
    /// crowns they hold up, and the crowns hung in the air as slabs.
    pub fn reduced(columns: &[Column; 256], x: usize, z: usize, width: usize) -> Self {
        if width == 1 {
            return columns[z * 16 + x].clone();
        }
        let mut events = Vec::new();
        for dz in z..z + width {
            for dx in x..x + width {
                for span in &columns[dz * 16 + dx].spans {
                    let material = (span.state, span.biome, span.light);
                    events.push((span.min, material, 1));
                    events.push((span.max, material, -1));
                }
            }
        }
        events.sort_unstable_by_key(|event| event.0);
        let mut result = Self::default();
        let mut votes = Vec::<((BlockStateId, u16), i32)>::new();
        let mut sky = [0i32; 16];
        let mut block = [0i32; 16];
        let mut occupied = 0;
        let mut bottom = MIN_Y as i16;
        for (y, material, change) in events {
            if bottom < y {
                if occupied > 0
                    && let Some(&((state, biome), _)) =
                        votes.iter().max_by_key(|&(material, count)| {
                            (*count, std::cmp::Reverse((material.0.0, material.1)))
                        })
                {
                    // Lighting must not split votes for the same material.
                    // Reduction can expose buried faces; retain the brightest
                    // actual light in this occupied footprint, per channel.
                    let light = (sky.iter().rposition(|&n| n > 0).unwrap_or(0) << 4)
                        | (block.iter().rposition(|&n| n > 0).unwrap_or(0) << 12);
                    result.push(Span {
                        min: bottom,
                        max: y,
                        state,
                        biome,
                        light: light as u16,
                    });
                }
                bottom = y;
            }
            occupied += change;
            sky[usize::from((material.2 >> 4) & 15)] += change;
            block[usize::from((material.2 >> 12) & 15)] += change;
            let material = (material.0, material.1);
            if let Some(vote) = votes.iter_mut().find(|vote| vote.0 == material) {
                vote.1 += change;
            } else {
                votes.push((material, change));
            }
        }
        result
    }
    pub fn from_surface(surface: Option<Surface>) -> Self {
        let mut column = Self {
            surface,
            spans: Spans::new(),
        };
        if let Some(s) = surface {
            if s.height > MIN_Y {
                column.spans.push(Span {
                    min: MIN_Y as i16,
                    max: s.height as i16,
                    state: s.state,
                    biome: s.biome,
                    light: 240,
                });
            }
            if let Some((height, state)) = s.water {
                column.spans.push(Span {
                    min: s.height as i16,
                    max: height as i16,
                    state,
                    biome: s.biome,
                    light: 240,
                });
            }
        }
        column
    }
    pub fn push(&mut self, span: Span) {
        if let Some(last) = self.spans.last_mut()
            && last.max == span.min
            && last.state == span.state
            && last.biome == span.biome
            && last.light == span.light
        {
            last.max = span.max;
        } else {
            self.spans.push(span);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_material_votes_ignore_light_and_retain_exposure() {
        let mut columns: [Column; 256] = std::array::from_fn(|_| Column::default());
        for i in 0..16 {
            columns[(i / 4) * 16 + i % 4].spans.push(Span {
                min: 60,
                max: 64,
                state: BlockStateId(if i < 10 { 2 } else { 1 }),
                biome: 0,
                light: if i < 10 { (i as u16 + 6) << 4 } else { 0 },
            });
        }
        let coarse = Column::reduced(&columns, 0, 0, 4);
        assert_eq!(coarse.spans.len(), 1);
        assert_eq!(
            coarse.spans[0].state,
            BlockStateId(2),
            "majority grass must beat dark dirt"
        );
        assert_eq!(coarse.spans[0].light, 240, "retain sampled sky exposure");
        for column in &mut columns {
            for span in &mut column.spans {
                span.light = 0;
            }
        }
        assert_eq!(
            Column::reduced(&columns, 0, 0, 4).spans[0].light,
            0,
            "unlit caves stay unlit"
        );
        columns[0].spans[0].light = 7 << 12;
        assert_eq!(
            Column::reduced(&columns, 0, 0, 4).spans[0].light,
            7 << 12,
            "block light does not invent skylight"
        );
    }
    #[test]
    fn coarse_voxels_keep_what_any_column_holds_so_crowns_stay_on_trunks() {
        let span = |min, max, state| Span {
            min,
            max,
            state: BlockStateId(state),
            biome: 0,
            light: 240,
        };
        let mut columns = std::array::from_fn(|_| Column {
            surface: None,
            spans: vec![span(0, 4, 1)].into(),
        });
        // One trunk column under a crown over the whole footprint.
        columns[0].spans.push(span(4, 12, 2));
        for z in 0..4 {
            for x in 0..4 {
                columns[z * 16 + x].spans.push(span(12, 14, 3));
            }
        }
        assert_eq!(
            Column::reduced(&columns, 0, 0, 4).spans,
            [span(0, 4, 1), span(4, 12, 2), span(12, 14, 3)]
        );
        assert_eq!(Column::reduced(&columns, 0, 0, 1), columns[0]);
    }
}
