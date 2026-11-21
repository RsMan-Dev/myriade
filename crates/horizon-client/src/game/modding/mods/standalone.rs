//! Client-only standalone model registration.
use super::{ExecutionRole, Game, RegistrationContext};
use mod_api::__private::game::RegistrationContextNew;

pub(super) fn collect_standalone_models(
    role: ExecutionRole,
) -> Option<horizon_client::models::StandaloneModels> {
    if !role.is_client() {
        return None;
    }
    let mut names = Vec::new();
    let mut forms = Vec::new();
    Game::native_standalone_models(&RegistrationContext::new(), &mut names, &mut forms);
    Some(horizon_client::models::StandaloneModels {
        names,
        forms: forms
            .into_iter()
            .map(|entry| horizon_client::models::StandaloneFormRegistration {
                owner: entry.mod_id,
                form: entry.answer,
            })
            .collect(),
    })
}
