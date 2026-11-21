use super::*;
impl Game<'_> {
    pub(in crate::game) fn text_string(
        &self,
        text: &horizon_client::text::Text,
    ) -> Result<String, mod_api::TextFormError> {
        Ok(text.plain())
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::api_values::TryIntoEngine;

    #[test]
    #[ignore = "daily"]
    fn plain_text_reads_resolve_current_language_arguments_keys_and_siblings() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            let key = mod_api::Text::new(mod_api::text::TextContent::Keybind(
                "key.keyboard.space".into(),
            ));
            let mut text =
                mod_api::Text::translate("test.plain", vec![mod_api::Text::literal("Cake"), key]);
            text.style.bold = Some(true);
            let mut sibling = mod_api::Text::literal(" + ");
            sibling
                .children
                .push(mod_api::Text::translate("test.tail", vec![]));
            text.children.push(sibling);
            let host: &dyn mod_api::ClientHost = game;
            for (space, tail, expected) in [
                ("Space", "tail", "Space / Cake + tail"),
                ("Espace", "fin", "Espace / Cake + fin"),
            ] {
                horizon_client::lang::scoped(
                    horizon_client::lang::Language::of(
                        "test",
                        [
                            ("test.plain".into(), "%2$s / %1$s".into()),
                            ("key.keyboard.space".into(), space.into()),
                            ("test.tail".into(), tail.into()),
                        ],
                    ),
                    || assert_eq!(host.text_string(&text).unwrap(), expected),
                );
            }
            assert_eq!(
                host.text_string(&mod_api::Text::literal("§aX")).unwrap(),
                "§aX"
            );
            assert_eq!(host.text_string(&mod_api::Text::literal("")).unwrap(), "");
            let fallback = mod_api::Text::new(mod_api::text::TextContent::Translatable {
                key: "test.missing.plain".into(),
                fallback: Some("Fallback %s".into()),
                arguments: vec![mod_api::Text::literal("X")],
            });
            assert_eq!(host.text_string(&fallback).unwrap(), "Fallback X");
        });
    }

}
