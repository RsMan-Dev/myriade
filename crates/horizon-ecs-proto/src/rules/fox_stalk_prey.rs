//! The stalk's distance tests: the prey is still too far to crouch, or close enough to crouch.

/// The fox is too far from the prey to crouch: the squared distance is above `still_sqr`.
pub fn beyond_still(d2: f64, still_sqr: f64) -> bool {
    d2 > still_sqr
}

/// The fox is close enough to crouch: the squared distance is at most `still_sqr`.
pub fn close_enough(d2: f64, still_sqr: f64) -> bool {
    d2 <= still_sqr
}
