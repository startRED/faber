//! Numbers given at compose time. Two changes written at once would both take the next decision or notes number, so a
//! Writer never picks one: it writes `{{next:<sequence>}}` where the number goes (`{{next:<sequence>:<label>}}` when it
//! needs a second one), and the engine puts in the next free number of the current head when it applies the patch.
//!
//! Only the candidate's own references change: a placeholder on a line the file already had before the patch (an
//! example in the documentation, say) is left as it is. The same placeholder gets the same number in every file, and
//! a number once given is kept, so a resume after a crash, or a repair, never numbers the same entry twice.
use super::rules::Sequence;
use std::collections::{BTreeMap, BTreeSet};

const OPEN: &str = "{{next:";
const CLOSE: &str = "}}";

/// The numbers given so far, by `<sequence>` or `<sequence>:<label>`.
pub type Given = BTreeMap<String, u64>;

/// The placeholders in `text`, as (byte range, key). A key names a sequence and an optional label.
fn placeholders(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let mut out = vec![];
    let mut from = 0;
    while let Some(start) = text[from..].find(OPEN).map(|i| from + i) {
        let inner = start + OPEN.len();
        let Some(end) = text[inner..].find(CLOSE).map(|i| inner + i) else { break };
        let key = &text[inner..end];
        let valid = !key.is_empty()
            && key.len() <= 64
            && key.splitn(2, ':').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        if valid {
            out.push((start..end + CLOSE.len(), key.to_string()));
            from = end + CLOSE.len();
        } else {
            from = inner;
        }
    }
    out
}

/// The highest number of `sequence` written in `text`.
pub fn highest(sequence: &Sequence, text: &str) -> u64 {
    let mut best = 0;
    for (at, _) in text.match_indices(&sequence.before) {
        let rest = &text[at + sequence.before.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() || digits.len() > 9 || !rest[digits.len()..].starts_with(&sequence.after) {
            continue;
        }
        best = best.max(digits.parse().unwrap_or(0));
    }
    best
}

/// Whether `path` is one of the sequence's files.
pub fn holds(sequence: &Sequence, path: &str) -> bool {
    sequence.files.iter().any(|f| {
        let f = f.trim_end_matches('/');
        path.eq_ignore_ascii_case(f) || path.to_lowercase().starts_with(&format!("{}/", f.to_lowercase()))
    })
}

/// One file the patch writes: its text on the head (none for a new file) and the text the Writer sent.
pub struct Written<'a> {
    pub before: Option<&'a str>,
    pub after: &'a str,
}

/// Number the placeholders the candidate added. `head` is every file of the sequences as it is on the current head,
/// `given` the numbers already given to this candidate (it grows with the new ones). Returns the new texts in the order
/// of `writes`, unchanged where there was nothing to number.
pub fn assign(sequences: &[Sequence], head: &[(String, String)], writes: &[Written], given: &mut Given) -> Vec<String> {
    let names: BTreeSet<&str> = sequences.iter().map(|s| s.name.as_str()).collect();
    let mut next: BTreeMap<&str, u64> = BTreeMap::new();
    for sequence in sequences {
        let on_head = head.iter().filter(|(path, _)| holds(sequence, path)).map(|(_, text)| highest(sequence, text)).max().unwrap_or(0);
        let mine =
            given.iter().filter(|(key, _)| key.split(':').next() == Some(sequence.name.as_str())).map(|(_, n)| *n).max().unwrap_or(0);
        next.insert(&sequence.name, on_head.max(mine) + 1);
    }
    let mut out = vec![];
    for write in writes {
        let old: BTreeSet<&str> = write.before.map(|t| t.lines().collect()).unwrap_or_default();
        let mut text = String::with_capacity(write.after.len());
        for line in write.after.split_inclusive('\n') {
            let bare = line.trim_end_matches(['\r', '\n']);
            if old.contains(bare) {
                text.push_str(line);
                continue;
            }
            let mut last = 0;
            for (range, key) in placeholders(line) {
                let sequence = key.split(':').next().unwrap_or_default();
                let Some(&name) = names.get(sequence) else { continue };
                let number = *given.entry(key).or_insert_with(|| {
                    let slot = next.get_mut(name).expect("every sequence has a next number");
                    *slot += 1;
                    *slot - 1
                });
                text.push_str(&line[last..range.start]);
                text.push_str(&number.to_string());
                last = range.end;
            }
            text.push_str(&line[last..]);
        }
        out.push(text);
    }
    out
}

/// The brief's words on numbered entries, or none when the project has no sequences.
pub fn brief(sequences: &[Sequence]) -> Option<String> {
    if sequences.is_empty() {
        return None;
    }
    let list: Vec<String> =
        sequences.iter().map(|s| format!("{} (in {}, written {}<number>{})", s.name, s.files.join(", "), s.before, s.after)).collect();
    Some(format!(
        "This project numbers some entries in sequence: {}. Never choose one of these numbers. Write {OPEN}<name>{CLOSE} where the number goes, the same text everywhere it is mentioned, and {OPEN}<name>:<label>{CLOSE} for a second new entry of the same kind; the engine puts in the next free number when it applies your patch.",
        list.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decisions() -> Sequence {
        Sequence { name: "decision".into(), files: vec!["docs/decisions.md".into()], before: "| R".into(), after: " |".into() }
    }
    fn notes() -> Sequence {
        Sequence { name: "notes".into(), files: vec!["ui/locales/".into()], before: "'notes.".into(), after: ".".into() }
    }

    const HEAD: &str = "| R1 | a | Owner |\n| R160 | b | AI |\n";

    #[test]
    fn the_next_free_number_of_the_head_replaces_the_candidates_placeholder_everywhere() {
        let head = vec![
            ("docs/decisions.md".to_string(), HEAD.to_string()),
            ("ui/locales/en.ts".to_string(), "  'notes.15.fixed': 'x',\n".to_string()),
            ("ui/locales/de.ts".to_string(), "  'notes.14.new': 'y',\n".to_string()),
        ];
        let decided = format!("{HEAD}| R{{{{next:decision}}}} | c | AI |\n");
        let doc = "See R{{next:decision}} and R{{next:decision:b}}.\n";
        let en = "  'notes.15.fixed': 'x',\n  'notes.{{next:notes}}.new': 'z',\n";
        let de = "  'notes.14.new': 'y',\n  'notes.{{next:notes}}.new': 'w',\n";
        let mut given = Given::new();
        let out = assign(
            &[decisions(), notes()],
            &head,
            &[
                Written { before: Some(HEAD), after: &decided },
                Written { before: None, after: doc },
                Written { before: Some(&head[1].1), after: en },
                Written { before: Some(&head[2].1), after: de },
            ],
            &mut given,
        );
        assert_eq!(out[0], format!("{HEAD}| R161 | c | AI |\n"));
        assert_eq!(out[1], "See R161 and R162.\n");
        assert!(out[2].ends_with("'notes.16.new': 'z',\n") && out[3].ends_with("'notes.16.new': 'w',\n"), "one number in every file");
        assert_eq!(given.get("decision"), Some(&161));
        assert_eq!(given.get("notes"), Some(&16));
    }

    #[test]
    fn a_placeholder_the_file_already_had_is_not_the_candidates_and_stays() {
        let before = "Writers write {{next:decision}} where the number goes.\n";
        let after = format!("{before}New line R{{{{next:decision}}}}.\n");
        let mut given = Given::new();
        let out = assign(&[decisions()], &[], &[Written { before: Some(before), after: &after }], &mut given);
        assert_eq!(out[0], format!("{before}New line R1.\n"));
    }

    #[test]
    fn numbers_once_given_are_kept_on_a_resume_or_a_repair() {
        let head = vec![("docs/decisions.md".to_string(), HEAD.to_string())];
        let mut given = Given::new();
        let first = assign(&[decisions()], &head, &[Written { before: None, after: "R{{next:decision}}" }], &mut given);
        assert_eq!(first[0], "R161");
        // The patch is half applied: the head the engine reads now holds R161, yet the same placeholder keeps its number.
        let moved = vec![("docs/decisions.md".to_string(), format!("{HEAD}| R161 | c | AI |\n"))];
        let again =
            assign(&[decisions()], &moved, &[Written { before: None, after: "R{{next:decision}} R{{next:decision:x}}" }], &mut given);
        assert_eq!(again[0], "R161 R162");
    }

    #[test]
    fn unknown_sequences_odd_text_and_crlf_lines_are_left_alone() {
        let mut given = Given::new();
        let text = "a {{next:other}} b {{next:}} c {{next:decision\r\nR{{next:decision}}\r\n";
        let out = assign(&[decisions()], &[], &[Written { before: None, after: text }], &mut given);
        assert_eq!(out[0], "a {{next:other}} b {{next:}} c {{next:decision\r\nR1\r\n");
        assert_eq!(highest(&decisions(), "| R12x | R7 | | R1234567890 |"), 7);
        assert!(brief(&[]).is_none());
        assert!(brief(&[decisions()]).unwrap().contains("{{next:<name>}}"));
    }
}
