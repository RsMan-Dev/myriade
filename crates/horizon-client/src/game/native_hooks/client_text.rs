use super::*;
impl Game<'_> {
    pub(in crate::game) fn text_string(
        &self,
        text: &horizon_client::text::Text,
    ) -> Result<String, mod_api::TextFormError> {
        Ok(text.plain())
    }

}
