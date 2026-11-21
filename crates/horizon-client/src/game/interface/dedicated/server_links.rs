//! The server's links to the players (`ClientCommonPacketListenerImpl.handleServerLinks`): only
//! the links that are absolute `http`/`https` URIs are sent.
use horizon_protocol::packets::configuration::ServerLink;

/// `Util.parseAndValidateUntrustedUri`: an absolute `http`/`https` URI
/// (java.net.URI's scheme then the allowlist; whitespace-free).
pub(crate) fn valid_untrusted_uri(uri: &str) -> bool {
    let Some(colon) = uri.find(':') else {
        return false;
    };
    let scheme = &uri[..colon];
    let mut chars = scheme.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return false;
    }
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return false;
    }
    !uri.chars().any(|c| c.is_control() || c == ' ')
}

/// Keeps only the links that validate, warning about the dropped ones.
pub(crate) fn trusted(links: Vec<ServerLink>) -> Vec<ServerLink> {
    links
        .into_iter()
        .filter(|l| {
            let ok = valid_untrusted_uri(&l.link);
            if !ok {
                log::warn!("Horizon: server sent an invalid link: {}", l.link);
            }
            ok
        })
        .collect()
}
