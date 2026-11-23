//! `myriade-bot`. Without arguments it is a protocol bot driven by JSON lines: one request per line on
//! stdin (`{"id","verb","args"}`), one reply per line on stdout, logs on stderr. With arguments it is the
//! load generator of `load.rs`, with the same flags and report as before.
mod far;
mod load;
mod online;
mod session;
mod state;
mod verbs;
mod verbs2;

use serde_json::{Value, json};
use session::Session;
use std::{
    io::{BufRead, Write},
    sync::mpsc::{self, TryRecvError},
    thread,
    time::Duration,
};

fn main() {
    if std::env::args().len() > 1 {
        load::run();
        return;
    }
    let (tx, lines) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut bot: Option<Session> = None;
    loop {
        // While a bot is connected, its packets are pumped between the lines it is asked to run.
        let line = match bot.as_mut() {
            Some(s) if !s.closed => {
                if let Err(e) = s.pump(Duration::from_millis(5)) {
                    eprintln!("myriade-bot: {e}");
                    s.st.error.get_or_insert(e);
                    s.closed = true;
                }
                match lines.try_recv() {
                    Ok(line) => line,
                    Err(TryRecvError::Empty) => continue,
                    Err(TryRecvError::Disconnected) => break,
                }
            }
            _ => match lines.recv() {
                Ok(line) => line,
                Err(_) => break,
            },
        };
        if line.trim().is_empty() {
            continue;
        }
        println!("{}", handle_line(&mut bot, &line));
        let _ = std::io::stdout().flush();
    }
}

/// One request in, one reply out: `{"id", "ok": true, ...}` or `{"id", "ok": false, "error"}`.
fn handle_line(bot: &mut Option<Session>, line: &str) -> Value {
    let req: Value = match serde_json::from_str(line) {
        Ok(req) => req,
        Err(e) => return json!({"id": null, "ok": false, "error": format!("bad request: {e}")}),
    };
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let verb = req.get("verb").and_then(Value::as_str).unwrap_or("");
    let args = req.get("args").cloned().unwrap_or_else(|| json!({}));
    match dispatch(bot, verb, &args) {
        Ok(Value::Object(fields)) => {
            let mut out = json!({"id": id, "ok": true});
            if let Value::Object(o) = &mut out {
                o.extend(fields);
            }
            out
        }
        Ok(_) => json!({"id": id, "ok": true}),
        Err(e) => json!({"id": id, "ok": false, "error": e}),
    }
}

fn dispatch(bot: &mut Option<Session>, verb: &str, args: &Value) -> Result<Value, String> {
    if verb == "join" {
        let (s, reply) = verbs::verb_join(args)?;
        *bot = Some(s);
        return Ok(reply);
    }
    let s = bot.as_mut().ok_or("not joined: send join first")?;
    let allowed_after_loss = matches!(verb, "report" | "state" | "leave");
    if let (Some(e), false) = (&s.st.error, allowed_after_loss) {
        return Err(e.clone());
    }
    match verb {
        "leave" => verbs::verb_leave(s, args),
        "chat" => verbs::verb_chat(s, args),
        "command" => verbs::verb_command(s, args),
        "wait_ticks" => verbs::verb_wait_ticks(s, args),
        "wait_for" => verbs::verb_wait_for(s, args),
        "state" => verbs::verb_state(s, args),
        "messages" => verbs::verb_messages(s, args),
        "entities" => verbs::verb_entities(s, args),
        "report" => verbs::verb_report(s, args),
        "move" => verbs2::verb_move(s, args),
        "look" => verbs2::verb_look(s, args),
        "look_at" => verbs2::verb_look_at(s, args),
        "attack" => verbs2::verb_attack(s, args),
        "interact" => verbs2::verb_interact(s, args),
        "use_item" => verbs2::verb_use_item(s, args),
        "release_use_item" => verbs2::verb_release_use_item(s, args),
        "set_carried_item" => verbs2::verb_set_carried_item(s, args),
        "player_abilities" => verbs2::verb_player_abilities(s, args),
        "use_block" => verbs2::verb_use_block(s, args),
        "break" => verbs2::verb_break(s, args),
        "inventory" => verbs2::verb_inventory(s, args),
        "menu" => verbs2::verb_menu(s, args),
        "click_slot" => verbs2::verb_click_slot(s, args),
        "close_menu" => verbs2::verb_close_menu(s, args),
        "quiet" => verbs2::verb_quiet(s, args),
        "capture" => verbs2::verb_capture(s, args),
        "cheat" => verbs2::verb_cheat(s, args),
        "tp_ack" => verbs2::verb_tp_ack(s, args),
        "pack_status" => verbs2::verb_pack_status(s, args),
        "vehicle_move" => verbs2::verb_vehicle_move(s, args),
        "paddle" => verbs2::verb_paddle(s, args),
        "player_input" => verbs2::verb_player_input(s, args),
        other => Err(format!("unknown verb: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_that_is_not_json_gets_an_error_reply_without_id() {
        let reply = handle_line(&mut None, "{not json");
        assert_eq!(reply["ok"], json!(false));
        assert_eq!(reply["id"], Value::Null);
        assert!(reply["error"].as_str().unwrap().starts_with("bad request"));
    }

    #[test]
    fn a_verb_before_join_is_refused_with_its_request_id() {
        let reply = handle_line(&mut None, r#"{"id":7,"verb":"state","args":{}}"#);
        assert_eq!(reply["id"], json!(7));
        assert_eq!(reply["ok"], json!(false));
        assert_eq!(reply["error"], json!("not joined: send join first"));
    }
}
