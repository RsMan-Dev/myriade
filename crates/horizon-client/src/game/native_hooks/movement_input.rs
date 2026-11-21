use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// Local player movement input, after reading it and before applying it.
    pub(in crate::game) fn native_movement_input(
        &self,
        call: mod_api::__private::MovementInputCall<'_>,
    ) {
        self.fire_movement_input(call);
    }
}

impl Game<'_> {

    #[cfg(test)]
    pub(in crate::game) fn movement_dispatch_fixture(&self, fire: bool) -> f32 {
        let mut keys = horizon_client::sprint::Keys::default();
        let (mut forward, mut strafe) = (1.0, 0.0);
        let call = mod_api::__private::MovementInputCall {
            forward: &mut forward,
            strafe: &mut strafe,
            jump: &mut keys.jump,
            sneak: &mut keys.shift,
            forward_key: &mut keys.forward,
            backward_key: &mut keys.backward,
            left_key: &mut keys.left,
            right_key: &mut keys.right,
            sprint: &mut keys.sprint,
        };
        if fire {
            self.native_movement_input(call);
        } else {
            mod_api::dispatch_movement_input(self, call);
        }
        forward
    }
}
