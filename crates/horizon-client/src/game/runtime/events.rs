//! What happened in the game, for agents (`events`, `wait-for`; docs/agent-control.md): the
//! state is compared once per event loop turn, after the game ran, so watching never changes
//! what the game does.
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};

#[derive(Default)]
pub(super) struct Events {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_take_since_and_limit_in_any_order() {
        assert_eq!(parse_events(&[]).unwrap(), (0, None));
        assert_eq!(
            parse_events(&["--limit", "5", "--since", "42"]).unwrap(),
            (42, Some(5))
        );
        assert!(parse_events(&["--limit"]).is_err());
        assert!(parse_events(&["--since", "x"]).is_err());
    }

    fn snap(screen: &'static str, chat: u64, logs: i64) -> Snapshot {
        Snapshot {
            screen,
            world: Some("w".into()),
            dimension: "minecraft:overworld".into(),
            chat,
            counts: BTreeMap::from([("minecraft:oak_log".to_owned(), logs)]),
            vitals: Some((20.0, 20)),
            hit: None,
        }
    }

    #[test]
    fn a_servers_damage_event_names_the_cause() {
        let mut events = Events::default();
        events.note_hit(
            "minecraft:mob_attack".into(),
            Some("minecraft:drowned".into()),
        );
        let before = snap("playing", 0, 0);
        let mut now = snap("playing", 0, 0);
        now.vitals = Some((17.0, 20));
        now.hit = events.remote_hit.clone();
        let hurt = &diff(&before, &now, &[])[0];
        assert_eq!(hurt["cause"], "minecraft:mob_attack");
        assert_eq!(hurt["by"], "minecraft:drowned");
        assert!(
            hurt.get("damage").is_none(),
            "the server's client only knows the health"
        );
        // The next hit of the same kind is another hit.
        events.note_hit(
            "minecraft:mob_attack".into(),
            Some("minecraft:drowned".into()),
        );
        assert_ne!(
            events.remote_hit.as_ref().unwrap().0,
            now.hit.as_ref().unwrap().0
        );
    }

    #[test]
    fn health_and_food_changes_are_events() {
        let before = snap("playing", 0, 0);
        let mut now = snap("playing", 0, 0);
        now.vitals = Some((17.0, 19));
        now.hit = Some((40, "mob_attack".into(), Some(3.0), Some("Zombie".into())));
        let events = diff(&before, &now, &[]);
        assert_eq!(
            events[0],
            json!({ "event": "hurt", "amount": 3.0, "health": 17.0,
                    "cause": "mob_attack", "damage": 3.0, "by": "Zombie" })
        );
        assert_eq!(
            events[1],
            json!({ "event": "food", "delta": -1, "food": 19 })
        );
        // The same hit is not named again; a respawn's refill is no heal.
        let mut healed = snap("playing", 0, 0);
        healed.hit = now.hit.clone();
        assert_eq!(diff(&now, &healed, &[])[0]["event"], "heal");
        let mut dead = now;
        dead.screen = "dead";
        assert!(
            diff(&dead, &snap("playing", 0, 0), &[])
                .iter()
                .all(|e| e["event"] != "heal")
        );
    }

    #[test]
    fn diffs_name_what_changed() {
        let events = diff(
            &snap("playing", 3, 1),
            &snap("dead", 5, 0),
            &["newest".into(), "older".into()],
        );
        let kinds: Vec<&str> = events
            .iter()
            .map(|e| e["event"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, ["screen", "death", "chat", "chat", "item"]);
        assert_eq!(events[2]["text"], "older", "chat oldest first");
        assert_eq!(events[4]["delta"], -1);
        let back = diff(&snap("dead", 5, 0), &snap("playing", 5, 0), &[]);
        assert_eq!(back[1]["event"], "respawn");
        let mut closed = snap("title", 5, 0);
        closed.world = None;
        closed.counts.clear();
        let left = diff(&snap("playing", 5, 4), &closed, &[]);
        assert_eq!(
            left[0],
            json!({ "event": "world", "state": "closed", "name": "w" })
        );
        assert!(
            left.iter().all(|e| e["event"] != "item"),
            "leaving drops no items"
        );
    }

    #[test]
    fn keeps_the_newest_events_numbered() {
        let mut e = Events::default();
        for i in 0..300 {
            e.push(i, json!({ "event": "chat" }));
        }
        assert_eq!(e.log.len(), KEPT);
        assert_eq!(e.since(295).count(), 5);
        assert_eq!(e.since(0).next().unwrap()["seq"], 45);
    }

    #[test]
    fn parses_conditions_and_timeouts() {
        let (c, t, since) =
            parse_wait(&["chat", "hello", "--since", "7", "there", "--timeout", "5"]).unwrap();
        assert!(matches!(c, Condition::Chat(ref s) if s == "hello there"));
        assert_eq!((t, since), (Duration::from_secs(5), Some(7)));
        assert!(matches!(
            parse_wait(&["loaded"]).unwrap().0,
            Condition::Loaded
        ));
        assert!(parse_wait(&["screen"]).is_err());
        assert!(parse_wait(&["soon"]).is_err());
    }
}
