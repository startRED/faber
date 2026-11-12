//! Look around, step 4: a local count of how often each sign is offered and used, per area, in `look.jsonl` in the data
//! folder. A line holds an event (`offered` or `used`), the sign's key (`look.fixed`, `look.sameText`), the area and a time:
//! never a word, a path or an id of the person's work. It stays on the PC, and nothing here sends it anywhere.
use crate::model::Area;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

pub const FILE: &str = "look.jsonl";
/// About 2,500 lines, many weeks of use; past it the log stops growing (shortcut: no rotation, add one if a longer
/// measure is ever needed).
const MAX: u64 = 200_000;

/// The leading words of the request the "Look around" button writes, in each language of the screen (`look.ask` in
/// `ui/locales`; a test keeps them in step). A message that starts with one of them is that request sent.
pub const REQUESTS: [&str; 5] = [
    "Look for the same kind of mistake",
    "Procure o mesmo tipo de erro",
    "Busca el mismo tipo de error",
    "Cherchez le même genre d’erreur",
    "Such dieselbe Art von Fehler",
];

/// True when `text` is the look-around request, sent as the button wrote it (a draft may follow).
pub fn is_request(text: &str) -> bool {
    let text = text.trim_start();
    REQUESTS.iter().any(|r| text.starts_with(r))
}

/// One line of the log: `event` is `offered` or `used`, `sign` the offer's key.
pub fn note(dir: &Path, at: u64, event: &str, sign: &str, area: Area) {
    crate::store::append_bounded(&dir.join(FILE), MAX, &json!({ "at": at, "event": event, "sign": sign, "area": area }));
}

/// How many times a sign was offered and used in an area, as `(sign, area) -> (offered, used)`. A line that does not
/// parse is skipped.
pub fn counts(dir: &Path) -> BTreeMap<(String, String), (u32, u32)> {
    let text = std::fs::read_to_string(dir.join(FILE)).unwrap_or_default();
    let mut out: BTreeMap<(String, String), (u32, u32)> = BTreeMap::new();
    for line in text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
        let field = |k: &str| line.get(k).and_then(Value::as_str).map(str::to_string);
        let (Some(event), Some(sign), Some(area)) = (field("event"), field("sign"), field("area")) else { continue };
        let entry = out.entry((sign, area)).or_default();
        match event.as_str() {
            "offered" => entry.0 += 1,
            "used" => entry.1 += 1,
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_holds_only_the_event_the_sign_the_area_and_the_time() {
        let dir = tempfile::tempdir().unwrap();
        note(dir.path(), 7, "offered", "look.fixed", Area::Project);
        let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        let line: Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(line, json!({ "at": 7, "event": "offered", "sign": "look.fixed", "area": "project" }));
    }

    #[test]
    fn counts_come_per_sign_and_area_and_skip_what_does_not_parse() {
        let dir = tempfile::tempdir().unwrap();
        for (event, sign, area) in [
            ("offered", "look.fixed", Area::Chat),
            ("offered", "look.fixed", Area::Chat),
            ("used", "look.fixed", Area::Chat),
            ("offered", "look.sameText", Area::Project),
        ] {
            note(dir.path(), 1, event, sign, area);
        }
        let path = dir.path().join(FILE);
        std::fs::write(&path, format!("{}not json\n", std::fs::read_to_string(&path).unwrap())).unwrap();
        let counts = counts(dir.path());
        assert_eq!(counts.get(&("look.fixed".into(), "chat".into())), Some(&(2, 1)));
        assert_eq!(counts.get(&("look.sameText".into(), "project".into())), Some(&(1, 0)));
        assert_eq!(counts.len(), 2);
    }

    #[test]
    fn the_log_stops_growing_at_its_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        crate::store::append_bounded(&path, 60, &json!({ "n": 1 }));
        let after_first = std::fs::metadata(&path).unwrap().len();
        for n in 0..50 {
            crate::store::append_bounded(&path, 60, &json!({ "n": n, "pad": "xxxxxxxxxxxxxxxxxxxx" }));
        }
        let len = std::fs::metadata(&path).unwrap().len();
        assert!(len > after_first && len < 60 + 60, "grew to {len}, past the limit by at most one line");
    }

    #[test]
    fn only_the_request_the_button_writes_is_recognised() {
        assert!(is_request("Look for the same kind of mistake you just fixed in the other places it could be."));
        assert!(is_request("  Procure o mesmo tipo de erro que você acabou de corrigir.\n\nAlso the tests"));
        assert!(!is_request("Please look for the same kind of mistake"));
        assert!(!is_request(""));
    }
}
