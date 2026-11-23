//! Online mode: the session file, the answer to the server's key, and the join the session service records.
use crate::state::uuid_of;
use horizon_protocol::{
    crypto::{generate_secret, server_hash},
    packets::login,
    rsa::RsaPublicKey,
};
use std::{
    io::Write,
    process::{Command, Stdio},
};

/// The account the launcher wrote: a name, a UUID and the token the session service checks.
pub struct Account {
    pub name: String,
    pub uuid: u128,
    pub token: String,
}

/// Reads the session file, `{"name","uuid","access_token",...}` with the UUID dashed, then deletes it:
/// the game reads it once.
pub fn read_account(path: &str) -> Result<Account, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let field = |key: &str| {
        json.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("{path}: no {key}"))
    };
    let account = Account {
        name: field("name")?,
        uuid: uuid_of(&field("uuid")?),
        token: field("access_token")?,
    };
    std::fs::remove_file(path).map_err(|e| format!("{path}: {e}"))?;
    Ok(account)
}

/// The answer to the server's `Hello` in online mode: the join is recorded with the session service
/// first, then the secret and the challenge go to the server encrypted with its key. Returns the
/// secret, which turns encryption on.
pub fn answer(account: &Account, hello: &login::Hello, session_url: &str) -> Result<(login::Key, [u8; 16]), String> {
    let key = RsaPublicKey::from_der(&hello.public_key).map_err(|e| e.to_string())?;
    let secret = generate_secret().map_err(|e| e.to_string())?;
    let hash = server_hash(&hello.server_id, &secret, &hello.public_key);
    join_online(account, &hash, session_url)?;
    let answer = login::Key {
        key: key.encrypt(&secret).map_err(|e| e.to_string())?,
        challenge: key.encrypt(&hello.challenge).map_err(|e| e.to_string())?,
    };
    Ok((answer, secret))
}

/// POSTs the join to `{session_url}/session/minecraft/join` and expects 204. The HTTP client is `curl`
/// (no TLS crate in the dependencies), with the body on its stdin so the token stays out of the arguments.
fn join_online(account: &Account, hash: &str, session_url: &str) -> Result<(), String> {
    let body = serde_json::json!({
        "accessToken": account.token,
        "selectedProfile": format!("{:032x}", account.uuid),
        "serverId": hash,
    })
    .to_string();
    let url = format!("{}/session/minecraft/join", session_url.trim_end_matches('/'));
    let mut child = Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", "-H", "Content-Type: application/json"])
        .args(["--data-binary", "@-", &url])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;
    child
        .stdin
        .take()
        .ok_or("curl: no stdin")?
        .write_all(body.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let code = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if code == "204" {
        Ok(())
    } else {
        Err(format!("session service refused the join: HTTP {code}"))
    }
}
