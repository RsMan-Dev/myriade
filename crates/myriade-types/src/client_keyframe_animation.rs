use crate::*;


/// What a channel moves (`AnimationChannel.Targets`): its sampled value is added to the part's
/// position, rotation or scale (`ModelPart.offsetPos`, `offsetRotation`, `offsetScale`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationTarget {
    /// The part's position, in texels (`minecraft:position`).
    Position,
    /// The part's rotation, in radians (`minecraft:rotation`).
    Rotation,
    /// The part's scale, as an offset from 1 (`minecraft:scale`).
    Scale,
    /// A target a mod registered (`register_json_animation_types`): its index in the
    /// animation's [`AnimationHolder::types`].
    Registered(u16),
}

/// How a channel reaches a keyframe from the one before (`AnimationChannel.Interpolations`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    /// A straight line from the previous keyframe (`minecraft:linear`).
    Linear,
    /// A Catmull-Rom spline through the neighbouring keyframes (`minecraft:catmullrom`).
    CatmullRom,
    /// An interpolation a mod registered (`register_json_animation_types`): its index in the
    /// animation's [`AnimationHolder::types`].
    Registered(u16),
}

/// One keyframe (`Keyframe`): at `time` seconds, the value arriving (`pre`) and the value leaving
/// (`post`), and how the channel moves into it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe {
    /// When, in seconds from the animation's start.
    pub time: f32,
    /// The value the channel arrives at.
    pub pre: [f32; 3],
    /// The value it leaves from.
    pub post: [f32; 3],
    /// How the channel moves into this keyframe from the one before.
    pub interpolation: Interpolation,
}

/// One bone's keyframes for one target (`AnimationChannel`), in time order.
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationChannel {
    /// What it moves.
    pub target: AnimationTarget,
    /// Its keyframes, in time order.
    pub keyframes: Vec<Keyframe>,
}

impl AnimationChannel {
    /// `KeyframeAnimation.Entry.apply`'s interpolation: the channel's value at `seconds`, times
    /// `scale`. Before the first keyframe it holds the first one, after the last the last. `None`
    /// at a segment into a registered interpolation, whose answer is a mod's code: such a channel
    /// plays through [`TypedKeyframeAnimation`].
    pub fn sample(&self, seconds: f32, scale: f32) -> Option<[f32; 3]> {
        let mut registered = false;
        let value = self.sample_by(seconds, scale, |_, _, _, _, _, _| {
            registered = true;
            [0.0; 3]
        });
        (!registered).then_some(value)
    }
    /// [`Self::sample`], with `registered` answering a segment into a registered interpolation:
    /// its index, the keyframes, the segment's ends, how far along it is and the scale.
    fn sample_by(
        &self,
        seconds: f32,
        scale: f32,
        mut registered: impl FnMut(u16, &[Keyframe], usize, usize, f32, f32) -> [f32; 3],
    ) -> [f32; 3] {
        let k = &self.keyframes;
        if k.is_empty() {
            return [0.0; 3];
        }
        // Mth.binarySearch(0, len, i -> seconds <= k[i].time) - 1
        let first = k.iter().position(|f| seconds <= f.time).unwrap_or(k.len());
        let prev = first.saturating_sub(1);
        let next = (prev + 1).min(k.len() - 1);
        let delta = seconds - k[prev].time;
        let alpha = if next != prev {
            (delta / (k[next].time - k[prev].time)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        match k[next].interpolation {
            Interpolation::CatmullRom => {
                let p0 = k[prev.saturating_sub(1)].post;
                let p1 = k[prev].post;
                let p2 = k[next].post;
                let p3 = k[(next + 1).min(k.len() - 1)].post;
                std::array::from_fn(|i| catmullrom(alpha, p0[i], p1[i], p2[i], p3[i]) * scale)
            }
            Interpolation::Linear => {
                let a = k[prev].post;
                let b = k[next].pre;
                std::array::from_fn(|i| (a[i] + (b[i] - a[i]) * alpha) * scale)
            }
            Interpolation::Registered(index) => registered(index, k, prev, next, alpha, scale),
        }
    }
}

/// `Mth.catmullrom`.
fn catmullrom(t: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    0.5 * (2.0 * p1
        + (p2 - p0) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t * t * t)
}
