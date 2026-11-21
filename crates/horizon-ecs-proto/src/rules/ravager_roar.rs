//! Calcul du recul du rugissement (`Ravager.roar`) : la poussée loin du ravageur sur le plan, avec un soulèvement fixe.

/// `Ravager.roar`'s push: away from the ravager on the plane, a fixed lift of 0.2, the strength `s` (`dd` at least 0.001).
pub fn fling(from: (f64, f64), to: (f64, f64), s: f64) -> [f64; 3] {
    let (xd, zd) = (to.0 - from.0, to.1 - from.1);
    let dd = (xd * xd + zd * zd).max(0.001);
    [xd / dd * s, 0.2, zd / dd * s]
}
