//! Calculs de la pose aléatoire d'un chameau (`RandomSitting`, `camel.rs:386-420`) : le délai avant le changement, la condition de changement, la panique.

/// The ticks before the next change: the time left until the pose is `min_pose_time` old, plus the geometric `draw`, at least 1.
pub fn change_delay(min_pose_time: u64, pose_time: i64, draw: i32) -> i32 {
    let left = (min_pose_time as i64 - pose_time).max(0);
    (left + i64::from(draw)).max(1) as i32
}

/// `canUse` without the draw: free to move (`free`) and the pose is `min_pose_time` old.
pub fn can_change(free: bool, pose_time: i64, min_pose_time: u64) -> bool {
    free && pose_time >= min_pose_time as i64
}

/// `should_panic` (`goals.rs:1089-1093`): a panic-cause hit within 40 ticks (`hit` is its panic flag and tick), or burning until `burning_until`.
pub fn should_panic(hit: Option<(bool, u64)>, burning_until: Option<i64>, now: u64) -> bool {
    hit.is_some_and(|(panic, at)| panic && now.saturating_sub(at) <= 40) || burning_until.is_some_and(|until| until > now as i64)
}
