//! Narrow cold lookup views for API-owned registration tests.
/// The immutable controller identities, copied without exposing Installed.
#[cfg(any(test, feature = "testing"))]
pub fn testing_controllers() -> Vec<super::ControllerInfoInput> {
    crate::parts::installed_controllers()
        .iter()
        .map(Into::into)
        .collect()
}
/// The immutable command argument declarations, copied without exposing Installed.
#[cfg(any(test, feature = "testing"))]
pub fn testing_command_arguments() -> Vec<super::CommandArgumentInfoInput> {
    crate::parts::installed_command_arguments()
        .iter()
        .map(Into::into)
        .collect()
}
/// The immutable entity selector declarations, copied without exposing Installed.
#[cfg(any(test, feature = "testing"))]
pub fn testing_entity_selectors() -> Vec<super::EntitySelectorInfoInput> {
    crate::parts::installed_entity_selectors()
        .iter()
        .map(Into::into)
        .collect()
}
/// Probe only whether a hook may access Parts in the current game context.
#[cfg(any(test, feature = "testing"))]
pub fn check_parts(
    hook: crate::HookId,
    name: &'static str,
) -> Result<(), crate::parts::MissingResolvedParts> {
    crate::parts::try_parts(hook, name).map(|_| ())
}
