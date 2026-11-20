//! What never reaches the agent, its replies or the step log from a page: card numbers, private keys and the tokens services
//! hand out. The page's own fields are judged by what they are (a password field's value never leaves the page, `act.js`);
//! this reads the words themselves, so a number or a key written in a paragraph, a title or a field's value is hidden too.
//!
//! Shapes, not guesses: a card number is 13 to 19 digits that pass the Luhn sum, a key is a word with a prefix a service
//! uses, a JWT, a private-key block, or a long word that mixes upper case, lower case and digits (a hex hash is none of those:
//! commit ids and checksums stay readable).

const CARD: &str = "[card number hidden]";
const SECRET: &str = "[secret hidden]";
const KEY_BLOCK: &str = "[private key hidden]";

/// The prefixes services give their tokens, with the fewest characters a whole token has.
const PREFIXES: [(&str, usize); 25] = [
    ("sk-", 20),
    ("sk_live_", 20),
    ("sk_test_", 20),
    ("pk_live_", 20),
    ("rk_live_", 20),
    ("ghp_", 20),
    ("gho_", 20),
    ("ghu_", 20),
    ("ghs_", 20),
    ("ghr_", 20),
    ("github_pat_", 30),
    ("glpat-", 20),
    ("xox", 20),
    ("xapp-", 20),
    ("npm_", 30),
    ("hf_", 30),
    ("shpat_", 30),
    ("SG.", 30),
    ("whsec_", 20),
    ("rk_test_", 20),
    ("ya29.", 20),
    ("dop_v1_", 30),
    ("pypi-", 30),
    ("ntn_", 30),
    ("lin_api_", 30),
];

/// `text` with every card number, private key and token in it replaced by a short note saying what was there.
pub fn mask(text: &str) -> String {
    // Invisible characters would split a number or a token into harmless pieces; odd spaces would split a card number.
    let text: String = text
        .chars()
        .filter(|c| !matches!(c, '\u{200b}'..='\u{200f}' | '\u{ad}' | '\u{2060}' | '\u{feff}' | '\u{202a}'..='\u{202e}' | '\u{e0000}'..='\u{e007f}'))
        .map(|c| if matches!(c, '\u{a0}' | '\u{2009}' | '\u{202f}') { ' ' } else { c })
        .collect();
    let text = keys_blocks(&text);
    let text = cards(&text);
    tokens(&text)
}

fn keys_blocks(text: &str) -> String {
    const BEGIN: &str = "-----BEGIN ";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(BEGIN) {
        let after = &rest[start..];
        let Some(label_end) = after[BEGIN.len()..].find("-----").map(|i| BEGIN.len() + i) else { break };
        if !after[BEGIN.len()..label_end].contains("PRIVATE KEY") {
            out.push_str(&rest[..start + BEGIN.len()]);
            rest = &rest[start + BEGIN.len()..];
            continue;
        }
        out.push_str(&rest[..start]);
        out.push_str(KEY_BLOCK);
        // To the end line of the block, or to the end of the text when it was cut before it.
        rest = match after[label_end..].find("-----END ") {
            Some(end) => {
                let from = label_end + end + "-----END ".len();
                after[from..].find("-----").map_or("", |close| &after[from + close + 5..])
            }
            None => "",
        };
    }
    out.push_str(rest);
    out
}

fn luhn(digits: &[u8]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, d)| {
            let d = u32::from(*d - b'0');
            if i % 2 == 1 { if d * 2 > 9 { d * 2 - 9 } else { d * 2 } } else { d }
        })
        .sum();
    sum.is_multiple_of(10)
}

fn cards(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut copied) = (0, 0);
    while i < bytes.len() {
        // A run starts at a digit that is not the tail of a longer word (`abc123`, `v2.4`).
        if !bytes[i].is_ascii_digit() || (i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_')) {
            i += 1;
            continue;
        }
        // Groups of digits joined by one or two spaces, dashes, dots or line breaks: a card is some whole groups of the run
        // (a quantity or a security code beside it must not hide it).
        let (mut groups, mut end) = (Vec::new(), i);
        loop {
            let start = end;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            groups.push((start, end));
            let gap = bytes[end..].iter().take_while(|b| matches!(b, b' ' | b'-' | b'.' | b'\n')).count();
            if (1..=2).contains(&gap) && bytes.get(end + gap).is_some_and(u8::is_ascii_digit) {
                end += gap;
            } else {
                break;
            }
        }
        let glued = bytes.get(end).is_some_and(|b| b.is_ascii_alphanumeric());
        let hit = (0..groups.len()).find_map(|a| {
            (a..groups.len()).rev().find_map(|z| {
                let digits: Vec<u8> = groups[a..=z].iter().flat_map(|&(s, e)| bytes[s..e].iter().copied()).collect();
                let whole_end = z + 1 == groups.len();
                ((13..=19).contains(&digits.len()) && !(whole_end && glued) && luhn(&digits)).then_some((groups[a].0, groups[z].1))
            })
        });
        if let Some((from, to)) = hit {
            out.push_str(&text[copied..from]);
            out.push_str(CARD);
            copied = to;
            i = to;
        } else {
            i = end.max(i + 1);
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// A word with the shape a service gives its tokens.
fn shaped(word: &str) -> bool {
    let n = word.len();
    if n >= 20 && word.starts_with("eyJ") && word.matches('.').count() == 2 {
        return true;
    }
    if (word.starts_with("AKIA") || word.starts_with("ASIA"))
        && n == 20
        && word.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return true;
    }
    if word.starts_with("AIza") && n == 39 {
        return true;
    }
    if PREFIXES.iter().any(|(prefix, least)| word.starts_with(prefix) && n >= *least) {
        return true;
    }
    false
}

/// Shaped, or long and mixing upper case, lower case and digits.
fn secret(word: &str) -> bool {
    shaped(word)
        || word.len() >= 40
            && word.bytes().any(|b| b.is_ascii_lowercase())
            && word.bytes().any(|b| b.is_ascii_uppercase())
            && word.bytes().any(|b| b.is_ascii_digit())
}

fn tokens(text: &str) -> String {
    let word_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+' | '/');
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        // A sentence's full stop is not part of the token.
        let trimmed = word.trim_end_matches(['.', '/']);
        // The token may follow a path, a key name or a joiner inside the word (`/reset/ghp_...`, `key_sk-...`).
        let from = if trimmed.len() > 2000 {
            secret(trimmed).then_some(0)
        } else {
            // The whole word, or the tail after a joiner, when it has a token's shape; else a long mixed word as a whole.
            std::iter::once(0)
                .chain(trimmed.match_indices(['/', '=', '_', '-', '.', ':']).map(|(i, _)| i + 1))
                .find(|&k| shaped(&trimmed[k..]))
                .or_else(|| secret(trimmed).then_some(0))
        };
        if let Some(k) = from {
            out.push_str(&trimmed[..k]);
            out.push_str(SECRET);
            out.push_str(&word[trimmed.len()..]);
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for c in text.chars() {
        if word_char(c) {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn cards_that_add_up_are_hidden_and_other_numbers_stay() {
        assert_eq!(mask("pay 4242 4242 4242 4242 now"), "pay [card number hidden] now");
        assert_eq!(mask("4111-1111-1111-1111."), "[card number hidden].");
        assert_eq!(mask("5555555555554444"), "[card number hidden]");
        assert_eq!(mask("Card 1 4242 4242 4242 4242"), "Card 1 [card number hidden]", "a quantity beside it");
        assert_eq!(mask("4242 4242 4242 4242 123"), "[card number hidden] 123", "and its security code");
        assert_eq!(mask("4242\u{a0}4242  4242.4242"), "[card number hidden]", "odd spaces");
        // Fails the sum, too short, too long and glued to letters: ordinary numbers.
        for plain in
            ["4242 4242 4242 4241", "order 1234567890", "id 12345678901234567890123", "ref a4242424242424242", "call +55 11 91234 5678"]
        {
            assert_eq!(mask(plain), plain, "{plain}");
        }
    }

    #[test]
    fn keys_and_tokens_are_hidden_and_hashes_and_words_stay() {
        assert_eq!(mask("key sk-live-4eC39HqLyjWDarjtT1zdp7dc ok"), "key [secret hidden] ok");
        assert_eq!(mask("token=ghp_16C7e42F292c6912E7710c838347Ae178B4a."), "token=[secret hidden].");
        assert_eq!(mask("AKIAIOSFODNN7EXAMPLE"), "[secret hidden]");
        assert_eq!(mask("https://x.com/reset/ghp_16C7e42F292c6912E7710c838347Ae178B4a"), "https://x.com/reset/[secret hidden]");
        assert_eq!(mask("key_sk-live-4eC39HqLyjWDarjtT1zdp7dc"), "key_[secret hidden]");
        assert_eq!(mask("ghp_16C7e42F292c\u{200b}6912E7710c838347Ae178B4a"), "[secret hidden]", "zero-width characters do not split it");
        assert_eq!(mask("Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.dBjftJeZ4CVPmB92K27uhbUJU1p1r"), "Bearer [secret hidden]");
        assert_eq!(mask("k: Zm9vYmFyQmF6UXV4MTIzNDU2Nzg5MGFiY2RlZkdISUpLTE1O"), "k: [secret hidden]");
        for plain in [
            "commit 3f786850e387550fdab836ed7e6dc881de23001b",
            "sha256 2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae",
            "the sk-learn package",
            "https://docs.example.org/guide/getting-started-with-the-configuration-file",
            "ghp_short",
        ] {
            assert_eq!(mask(plain), plain, "{plain}");
        }
    }

    #[test]
    fn a_private_key_block_goes_whole_and_a_public_one_stays() {
        let pem = "before\n-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\nabc\n-----END RSA PRIVATE KEY-----\nafter";
        assert_eq!(mask(pem), "before\n[private key hidden]\nafter");
        assert_eq!(mask("-----BEGIN PRIVATE KEY-----\nMIIE"), "[private key hidden]", "cut before its end");
        let public = "-----BEGIN PUBLIC KEY-----\nMIIB\n-----END PUBLIC KEY-----";
        assert_eq!(mask(public), public);
        assert_eq!(mask(&mask(pem)), mask(pem), "masking twice changes nothing");
    }
}
