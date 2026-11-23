//! LocalPlayer sprint decisions around KeyboardInput.tick. Conditions are the
//! current player queries; setting sprint dispatches to the live attribute state.
fn length(v: [f32; 2]) -> f32 {
    f64::from(v[0] * v[0] + v[1] * v[1]).sqrt() as f32
}
fn scale(v: [f32; 2], factor: f32) -> [f32; 2] {
    [v[0] * factor, v[1] * factor]
}
/// The move vector of the pressed keys, normalised (`KeyboardInput.tick`).
pub fn keyboard_direction(forward: bool, backward: bool, left: bool, right: bool) -> [f32; 2] {
    let impulse = |positive: bool, negative: bool| {
        if positive == negative {
            0.0
        } else if positive {
            1.0
        } else {
            -1.0
        }
    };
    let v = [impulse(left, right), impulse(forward, backward)];
    let magnitude = length(v);
    if magnitude < 1.0e-4 {
        [0.0, 0.0]
    } else {
        [v[0] / magnitude, v[1] / magnitude]
    }
}

/// LocalPlayer.modifyInput with already selected multipliers. `item_use` is
/// Some only when using an item and not riding; `sneaking` only when moving
/// slowly. Zero input returns before either multiplier is applied.
pub fn modify_input(input: [f32; 2], item_use: Option<f32>, sneaking: Option<f32>) -> [f32; 2] {
    if input[0] * input[0] + input[1] * input[1] == 0.0 {
        return input;
    }
    let mut v = scale(input, 0.98);
    if let Some(multiplier) = item_use {
        v = scale(v, multiplier);
    }
    if let Some(multiplier) = sneaking {
        v = scale(v, multiplier);
    }
    square_movement(v)
}

fn square_movement(v: [f32; 2]) -> [f32; 2] {
    let magnitude = length(v);
    if magnitude <= 0.0 {
        return v;
    }
    let unit = scale(v, 1.0 / magnitude);
    let x = unit[0].abs();
    let y = unit[1].abs();
    let ratio = if y > x { x / y } else { y / x };
    let distance = f64::from(1.0 + ratio * ratio).sqrt() as f32;
    let speed = magnitude * distance;
    // Java Math.min propagates NaN, unlike Rust's f32::min.
    scale(
        unit,
        if speed.is_nan() || speed < 1.0 {
            speed
        } else {
            1.0
        },
    )
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Keys {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub shift: bool,
    pub sprint: bool,
    /// A movement-input listener explicitly holds sprint off this local tick.
    /// Server prediction instead replays the client's resulting sprint state.
    pub sprint_suppressed: bool,
    /// A controller stick's move vector (`[left, forward]`, Horizon's own),
    /// used instead of the keys' direction while it is pushed.
    pub analog: Option<[f32; 2]>,
}
impl Keys {
    pub fn direction(self) -> [f32; 2] {
        match self.analog {
            Some(v) => v,
            None => keyboard_direction(self.forward, self.backward, self.left, self.right),
        }
    }
    /// Add a stick's move vector: the direction flags code reading keys
    /// (sprint, vehicles, the server's input packet) see are set past a
    /// third of the travel.
    pub fn with_stick(mut self, stick: Option<[f32; 2]>) -> Self {
        if let Some([left, forward]) = stick {
            self.forward |= forward > 0.3;
            self.backward |= forward < -0.3;
            self.left |= left > 0.3;
            self.right |= left < -0.3;
            self.analog = Some([left, forward]);
        }
        self
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct SprintState {
    pub trigger_time: i32,
    pub previous_forward: bool,
    pub previous_shift: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Conditions {
    pub sprinting: bool,
    pub mobility_restricted: bool,
    pub passenger: bool,
    pub vehicle_can_sprint: bool,
    pub vehicle_local: bool,
    pub enough_food: bool,
    pub may_fly: bool,
    pub flying: bool,
    pub shallow_water: bool,
    pub slow_item: bool,
    pub fall_flying: bool,
    pub under_water: bool,
    pub moving_slowly: bool,
    pub swimming: bool,
    pub in_water: bool,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub minor_horizontal_collision: bool,
}
impl Conditions {
    fn possible(self, allow_shallow_water: bool) -> bool {
        !self.mobility_restricted
            && if self.passenger {
                self.vehicle_can_sprint && self.vehicle_local
            } else {
                self.enough_food || self.may_fly
            }
            && (allow_shallow_water || !self.shallow_water)
    }
    fn can_start(self, forward: bool) -> bool {
        !self.sprinting
            && forward
            && self.possible(self.flying)
            && !self.slow_item
            && (!self.fall_flying || self.under_water)
            && (!self.moving_slowly || self.under_water)
    }
    fn should_stop(self, forward: bool, shift: bool) -> bool {
        if self.swimming {
            !self.possible(true) || !self.in_water || (!forward && !self.on_ground && !shift)
        } else {
            !self.possible(self.flying)
                || !forward
                || (self.horizontal_collision && !self.minor_horizontal_collision)
        }
    }
}
impl SprintState {
    /// `LocalPlayer.aiStep`'s sprint phase, with a mod's optional tick suppression.
    /// The default vanilla window is seven ticks; options can select 0..10.
    /// The setter runs for every native request, including repeated true requests.
    pub fn tick<E>(
        &mut self,
        keys: Keys,
        conditions: Conditions,
        window: i32,
        mut set_sprinting: impl FnMut(bool) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.trigger_time > 0 {
            self.trigger_time -= 1;
        }
        let old_forward = self.previous_forward;
        let old_shift = self.previous_shift;
        let forward = keys.direction()[1] > 1.0e-5;
        self.previous_forward = forward;
        self.previous_shift = keys.shift;
        if old_shift || (conditions.slow_item && !conditions.passenger) || keys.backward {
            self.trigger_time = 0;
        }
        if keys.sprint_suppressed {
            self.trigger_time = 0;
            if conditions.sprinting {
                set_sprinting(false)?;
            }
            return Ok(());
        }
        let mut sprinting = conditions.sprinting;
        if conditions.can_start(forward) {
            if !old_forward {
                if self.trigger_time > 0 {
                    set_sprinting(true)?;
                    sprinting = true;
                } else {
                    self.trigger_time = window;
                }
            }
            if keys.sprint {
                set_sprinting(true)?;
                sprinting = true;
            }
        }
        if sprinting && conditions.should_stop(forward, keys.shift) {
            set_sprinting(false)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standing() -> Conditions {
        Conditions {
            enough_food: true,
            on_ground: true,
            ..Conditions::default()
        }
    }

    fn step(state: &mut SprintState, keys: Keys, sprinting: &mut bool) {
        let current = *sprinting;
        let mut next = current;
        state
            .tick(
                keys,
                Conditions {
                    sprinting: current,
                    ..standing()
                },
                7,
                |value| {
                    next = value;
                    Ok::<(), ()>(())
                },
            )
            .unwrap();
        *sprinting = next;
    }

    /// Vanilla sprint is a state. Releasing the sprint key, including on the
    /// jump tick, keeps it while forward is held; releasing forward stops it.
    #[test]
    fn sprint_survives_releasing_the_key_through_a_jump() {
        let mut state = SprintState::default();
        let mut sprinting = false;
        step(
            &mut state,
            Keys {
                forward: true,
                sprint: true,
                ..Keys::default()
            },
            &mut sprinting,
        );
        assert!(sprinting);
        step(
            &mut state,
            Keys {
                forward: true,
                jump: true,
                ..Keys::default()
            },
            &mut sprinting,
        );
        assert!(
            sprinting,
            "a jump after the sprint key comes up must stay a sprint"
        );
        step(
            &mut state,
            Keys {
                jump: true,
                ..Keys::default()
            },
            &mut sprinting,
        );
        assert!(!sprinting);
    }
}
