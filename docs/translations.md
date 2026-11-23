# Translations

The server speaks English only. Its own messages (chat moderation, mod checks, `/tempban`, `/modprofile`, the
world-opening refusal) come from `crates/horizon-client/assets/horizon/lang/en_us.json`, whose keys are `horizon.*`.
The server does not translate them: it sends each message as a `translate` text component with its English as the
fallback, and every client resolves the key in its own selected language with its own lang files. Vanilla's text
comes from the player's own Minecraft install and is not translated here.

## Where the text lives

| | Files | Keys | Read by |
|---|---|---|---|
| Server and Horizon client | `crates/horizon-client/assets/horizon/lang/en_us.json` | `horizon.<group>.<name>` | `crates/horizon-client-core/src/lang.rs` (`Bundled`, `Text::tr`, `lang::get`) |
| A crate's own text | `crates/<name>/assets/lang/<code>.json`, the same shape | the crate's own keys, never one of the engine's | the same, once the crate registers them ([layers.md](layers.md#lang-files-horizon_platformlang)) |

## Adding a message

1. Write the English text in `en_us.json`, with a `horizon.<group>.<name>` key, placeholders as `%s` or `%1$s`.
2. In the code, send it as a `translate` component with the English as fallback
   (`Text::translate_with_fallback(key, english, args)`), so a client without the key still reads English.
3. The `lang.rs` test `every_horizon_key_the_code_names_is_in_en_us` fails if the code names a key `en_us.json` lacks.

Keep the placeholders the code fills in, in the same order, in any translation a client ships.

## Rules

- A key is never built at run time: the test finds the keys by reading the sources.
- A key the code no longer names stays in `en_us.json` until it is removed by hand; no test lists it.
- Server text is English on purpose: a client translates what it receives, so a server never picks the language of
  a player.
